use crate::NoteMeta;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MODEL: &str = "nomic-embed-text";
const BASE: &str = "http://127.0.0.1:11434";
const THRESHOLD: f32 = 0.50;
const TOP_K: usize = 5;
const MAX_TEXT: usize = 8_000; // chars — keeps embed latency low

// ── API types ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EmbedReq<'a> { model: &'a str, prompt: &'a str }

#[derive(Deserialize)]
struct TagsResp { models: Vec<OllamaModel> }

#[derive(Deserialize)]
struct OllamaModel { name: String }

// ── Disk cache ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
struct Cached { mtime: String, emb: Vec<f32> }

type Cache = HashMap<String, Cached>;

fn cache_path(vault_path: &str) -> std::path::PathBuf {
    std::path::Path::new(vault_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(".jb_embed_cache.json")
}

fn load_cache(vault_path: &str) -> Cache {
    std::fs::read_to_string(cache_path(vault_path))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(vault_path: &str, cache: &Cache) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = std::fs::write(cache_path(vault_path), json);
    }
}

fn note_mtime(path: &str) -> String {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| format!("{t:?}"))
        .unwrap_or_default()
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Reqwest client that bypasses the Windows system proxy for loopback connections.
/// Without this, WinHTTP proxy settings can block 127.0.0.1 in release builds.
fn make_client() -> reqwest::Client {
    reqwest::ClientBuilder::new()
        .no_proxy()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Try to start the Ollama server if not already running.
/// Looks for the binary in known Windows install locations, then falls back to PATH.
pub fn try_start_ollama() {
    use std::process::{Command, Stdio};
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let candidates = [
        format!("{local}\\Programs\\Ollama\\ollama.exe"),
        "C:\\Program Files\\Ollama\\ollama.exe".to_string(),
        "ollama".to_string(),
    ];
    for path in &candidates {
        if path.ends_with(".exe") && !std::path::Path::new(path).exists() {
            continue;
        }
        let mut cmd = Command::new(path);
        cmd.arg("serve")
           .stdout(Stdio::null())
           .stderr(Stdio::null())
           .stdin(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
        if cmd.spawn().is_ok() {
            eprintln!("[ollama] démarré depuis: {path}");
            return;
        }
    }
    eprintln!("[ollama] introuvable — installe Ollama depuis https://ollama.com");
}

pub async fn is_available() -> bool {
    make_client()
        .get(format!("{BASE}/api/version"))
        .timeout(std::time::Duration::from_secs(2))
        .send().await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn has_embed_model() -> bool {
    let Ok(resp) = make_client()
        .get(format!("{BASE}/api/tags"))
        .timeout(std::time::Duration::from_secs(3))
        .send().await
    else { return false };

    resp.json::<TagsResp>().await
        .map(|t| t.models.iter().any(|m| m.name.starts_with("nomic-embed-text")))
        .unwrap_or(false)
}

/// Full status check: availability + version + model list.
/// Used by the Tauri command so the frontend never calls Ollama directly
/// (direct fetch from tauri://localhost is blocked by WebView2 cross-origin rules).
pub async fn get_status() -> (bool, Option<String>, Vec<String>) {
    let client = make_client();
    let Ok(r) = client
        .get(format!("{BASE}/api/version"))
        .timeout(std::time::Duration::from_secs(2))
        .send().await
    else { return (false, None, vec![]); };

    if !r.status().is_success() { return (false, None, vec![]); }

    let version = r.json::<serde_json::Value>().await
        .ok()
        .and_then(|v| v["version"].as_str().map(str::to_string));

    let models = match client
        .get(format!("{BASE}/api/tags"))
        .timeout(std::time::Duration::from_secs(3))
        .send().await
    {
        Ok(r) => r.json::<TagsResp>().await
            .map(|t| t.models.into_iter().map(|m| m.name).collect())
            .unwrap_or_default(),
        Err(_) => vec![],
    };

    (true, version, models)
}

async fn embed_one(client: &reqwest::Client, text: &str) -> anyhow::Result<Vec<f32>> {
    let truncated = if text.len() > MAX_TEXT { &text[..MAX_TEXT] } else { text };

    // Try new endpoint first (/api/embed, Ollama ≥ 0.1.26)
    let val: serde_json::Value = client
        .post(format!("{BASE}/api/embed"))
        .json(&serde_json::json!({ "model": MODEL, "input": truncated }))
        .timeout(std::time::Duration::from_secs(30))
        .send().await?
        .json().await
        .unwrap_or(serde_json::Value::Null);

    // New format: { "embeddings": [[0.1, 0.2, ...]] }
    if let Some(arr) = val["embeddings"].as_array().and_then(|a| a.first()).and_then(|v| v.as_array()) {
        let emb: Vec<f32> = arr.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
        if !emb.is_empty() { return Ok(emb); }
    }

    // Fallback: old endpoint /api/embeddings with prompt field (Ollama < 0.1.26)
    let val2: serde_json::Value = client
        .post(format!("{BASE}/api/embeddings"))
        .json(&EmbedReq { model: MODEL, prompt: truncated })
        .timeout(std::time::Duration::from_secs(30))
        .send().await?
        .json().await
        .unwrap_or(serde_json::Value::Null);

    // Old format: { "embedding": [0.1, 0.2, ...] }
    if let Some(arr) = val2["embedding"].as_array() {
        let emb: Vec<f32> = arr.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
        if !emb.is_empty() { return Ok(emb); }
    }

    // Surface any error message Ollama returned
    let err = val["error"].as_str()
        .or_else(|| val2["error"].as_str())
        .unwrap_or("réponse inattendue");
    Err(anyhow::anyhow!("{err}"))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

/// Compute semantic similarities using Ollama's nomic-embed-text model.
/// Embeddings are cached to disk — only recomputed when a note's mtime changes.
/// Returns (source_id, target_id, similarity) pairs above THRESHOLD.
pub async fn compute_similarities(
    vault_path: &str,
    notes: &[NoteMeta],
) -> Vec<(String, String, f32)> {
    if notes.len() < 2 { return vec![]; }

    let client = make_client();
    let mut cache = load_cache(vault_path);
    let mut dirty = false;

    let mut embeddings: Vec<Option<Vec<f32>>> = Vec::with_capacity(notes.len());

    for note in notes {
        let mtime = note_mtime(&note.path);

        // Cache hit — skip re-embedding
        if let Some(entry) = cache.get(&note.path) {
            if entry.mtime == mtime && !entry.emb.is_empty() {
                embeddings.push(Some(entry.emb.clone()));
                continue;
            }
        }

        // Build embed text: title + tags + body (capped)
        let body = std::fs::read_to_string(&note.path).unwrap_or_default();
        let tags_str = note.tags.join(" ");
        let text = format!("{}\n{}\n{}", note.title, tags_str, body);

        match embed_one(&client, &text).await {
            Ok(emb) => {
                cache.insert(note.path.clone(), Cached { mtime, emb: emb.clone() });
                embeddings.push(Some(emb));
                dirty = true;
            }
            Err(e) => {
                eprintln!("[ollama] embed failed for {}: {e}", note.title);
                embeddings.push(None);
            }
        }
    }

    if dirty { save_cache(vault_path, &cache); }

    // Pairwise cosine similarities — top K per note, canonical pairs only
    let mut results: Vec<(String, String, f32)> = vec![];
    let mut emitted = std::collections::HashSet::new();

    for i in 0..notes.len() {
        let Some(ei) = &embeddings[i] else { continue };

        let mut sims: Vec<(usize, f32)> = (0..notes.len())
            .filter(|&j| j != i)
            .filter_map(|j| {
                let ej = embeddings[j].as_ref()?;
                let s = cosine(ei, ej);
                if s >= THRESHOLD { Some((j, s)) } else { None }
            })
            .collect();
        sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (j, sim) in sims.into_iter().take(TOP_K) {
            let (a, b) = if i < j { (i, j) } else { (j, i) };
            let key = (notes[a].id.clone(), notes[b].id.clone());
            if emitted.insert(key) {
                results.push((notes[a].id.clone(), notes[b].id.clone(), sim));
            }
        }
    }

    results
}

/// Detect the first available Ollama text-generation model (not an embed model).
async fn find_text_model(client: &reqwest::Client) -> Option<String> {
    let resp = client
        .get(format!("{BASE}/api/tags"))
        .timeout(std::time::Duration::from_secs(3))
        .send().await.ok()?;
    let tags: TagsResp = resp.json().await.ok()?;
    // Prefer small / known-good models
    let prefs = ["llama3.2", "llama3", "mistral", "phi3", "phi4", "gemma2", "gemma3",
                 "qwen2.5", "qwen2", "deepseek", "llama2"];
    for pref in prefs {
        if let Some(m) = tags.models.iter().find(|m| {
            m.name.starts_with(pref) && !m.name.contains("embed") && !m.name.contains("nomic")
        }) { return Some(m.name.clone()); }
    }
    // Fallback: any non-embed model
    tags.models.iter()
        .find(|m| !m.name.contains("embed") && !m.name.contains("nomic"))
        .map(|m| m.name.clone())
}

/// k-means clustering on cached Ollama embeddings.
/// Returns note_path → cluster_id (0..k). Empty map if embeddings not yet computed.
pub fn compute_clusters(vault_path: &str, notes: &[NoteMeta], k: usize) -> HashMap<String, usize> {
    if notes.is_empty() || k == 0 { return HashMap::new(); }
    let cache = load_cache(vault_path);

    let embedded: Vec<(&NoteMeta, &Vec<f32>)> = notes.iter()
        .filter_map(|n| {
            let c = cache.get(&n.path)?;
            if c.emb.is_empty() { return None; }
            Some((n, &c.emb))
        })
        .collect();

    let k = k.min(embedded.len());
    if k < 2 { return HashMap::new(); }

    // Initial centroids: evenly spaced
    let step = embedded.len() / k;
    let mut centroids: Vec<Vec<f32>> = (0..k).map(|i| embedded[i * step].1.clone()).collect();
    let mut assignments = vec![0usize; embedded.len()];

    for _ in 0..15 {
        let mut changed = false;
        for (i, (_, emb)) in embedded.iter().enumerate() {
            let best = (0..k).max_by(|&a, &b|
                cosine(emb, &centroids[a]).partial_cmp(&cosine(emb, &centroids[b]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            ).unwrap_or(0);
            if assignments[i] != best { assignments[i] = best; changed = true; }
        }
        if !changed { break; }

        for c in 0..k {
            let members: Vec<&Vec<f32>> = embedded.iter().zip(&assignments)
                .filter(|(_, &cl)| cl == c).map(|((_, e), _)| *e).collect();
            if members.is_empty() { continue; }
            let dim = members[0].len();
            let mut centroid = vec![0.0f32; dim];
            for m in &members {
                for (j, &v) in m.iter().enumerate() { centroid[j] += v; }
            }
            let n = members.len() as f32;
            for v in &mut centroid { *v /= n; }
            centroids[c] = centroid;
        }
    }

    embedded.iter().zip(&assignments)
        .map(|((note, _), &cl)| (note.path.clone(), cl))
        .collect()
}

/// Embed `query` and return notes sorted by cosine similarity (uses cached embeddings).
pub async fn semantic_search(
    vault_path: &str,
    query: &str,
    notes: &[NoteMeta],
) -> anyhow::Result<Vec<(String, String, f32)>> {
    let client = make_client();
    let query_emb = embed_one(&client, query).await?;
    let cache = load_cache(vault_path);

    let mut results: Vec<(String, String, f32)> = notes.iter()
        .filter_map(|n| {
            let emb = &cache.get(&n.path)?.emb;
            if emb.is_empty() { return None; }
            let score = cosine(&query_emb, emb);
            if score < 0.25 { return None; }
            Some((n.path.clone(), n.title.clone(), score))
        })
        .collect();

    results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results.into_iter().take(12).collect())
}

/// Ask the available Ollama text model to suggest tags for a note.
/// Returns empty vec if no text model is installed.
pub async fn suggest_tags(note_title: &str, note_body: &str) -> Vec<String> {
    let client = make_client();
    let Some(model) = find_text_model(&client).await else { return vec![] };

    let content = if note_body.len() > 1_500 { &note_body[..1_500] } else { note_body };
    let prompt = format!(
        "Suggest 3-5 short, lowercase, hyphenated tags for this research note.\n\
        Return ONLY a comma-separated list of tags, nothing else. No explanations.\n\
        Examples: cognitive-science, embodied-cognition, predictive-processing\n\n\
        Title: {note_title}\n\nContent:\n{content}"
    );

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "options": { "temperature": 0.2, "num_predict": 60 }
    });

    let Ok(resp) = client
        .post(format!("{BASE}/api/generate"))
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send().await
    else { return vec![] };

    let Ok(json) = resp.json::<serde_json::Value>().await else { return vec![] };

    let text = json["response"].as_str().unwrap_or("");
    text.split(',')
        .map(|t| t.trim().to_lowercase()
            .replace(' ', "-")
            .chars().filter(|c| c.is_alphanumeric() || *c == '-').collect::<String>())
        .filter(|t| !t.is_empty() && t.len() >= 2 && t.len() <= 40)
        .take(5)
        .collect()
}

/// Return the top semantic keywords from the embedding cache (no API calls).
/// Each tag is weighted by the sum of cosine similarities of notes that carry it.
pub fn get_central_keywords(vault_path: &str, notes: &[NoteMeta]) -> Vec<String> {
    if notes.is_empty() { return vec![]; }

    let cache = load_cache(vault_path);
    let embeddings: Vec<Option<Vec<f32>>> = notes.iter()
        .map(|n| cache.get(&n.path).map(|c| c.emb.clone()))
        .collect();

    // Centrality = sum of similarities above threshold to all other notes
    let centralities: Vec<f32> = (0..notes.len()).map(|i| {
        let Some(ei) = &embeddings[i] else { return 0.0 };
        (0..notes.len()).filter(|&j| j != i)
            .filter_map(|j| embeddings[j].as_ref().map(|ej| {
                let s = cosine(ei, ej);
                if s >= THRESHOLD { s } else { 0.0 }
            }))
            .sum()
    }).collect();

    let mut tag_scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for (i, note) in notes.iter().enumerate() {
        let c = centralities[i];
        if c > 0.0 {
            for tag in &note.tags {
                *tag_scores.entry(tag.clone()).or_default() += c;
            }
        }
    }

    let mut sorted: Vec<(String, f32)> = tag_scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.into_iter().take(12).map(|(k, _)| k).collect()
}

/// After saving a note, find other recently-modified notes that are semantically very close
/// but not yet explicitly linked. Returns (src_path, src_title, tgt_path, tgt_title, score).
pub async fn find_new_connections(
    vault_path: &str,
    notes: &[NoteMeta],
    saved_path: &str,
) -> Vec<(String, String, String, String, f32)> {
    const THRESHOLD: f32 = 0.72;
    const MAX_AGE_DAYS: i64 = 30;
    const MAX_RESULTS: usize = 5;

    let cache = load_cache(vault_path);

    let Some(saved_note) = notes.iter().find(|n| n.path == saved_path) else { return vec![] };
    let Some(saved_cached) = cache.get(saved_path) else { return vec![] };
    if saved_cached.emb.is_empty() { return vec![] }

    let cutoff = chrono::Utc::now() - chrono::TimeDelta::days(MAX_AGE_DAYS);

    // Titles already explicitly linked from/to the saved note (lowercase for comparison)
    let linked_titles: std::collections::HashSet<String> = saved_note.links.iter()
        .map(|l| l.to_lowercase()).collect();
    let links_to_saved: std::collections::HashSet<String> = notes.iter()
        .filter(|n| n.links.iter().any(|l| l.to_lowercase() == saved_note.title.to_lowercase()))
        .map(|n| n.path.clone()).collect();

    let mut results = Vec::new();

    for note in notes {
        if note.path == saved_path { continue; }
        // Only notes modified within MAX_AGE_DAYS
        if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&note.modified) {
            if modified.with_timezone(&chrono::Utc) < cutoff { continue; }
        }
        // Skip already-linked pairs
        if linked_titles.contains(&note.title.to_lowercase()) { continue; }
        if links_to_saved.contains(&note.path) { continue; }

        let Some(cached) = cache.get(&note.path) else { continue };
        if cached.emb.is_empty() { continue; }

        let score = cosine(&saved_cached.emb, &cached.emb);
        if score >= THRESHOLD {
            results.push((
                saved_path.to_string(), saved_note.title.clone(),
                note.path.clone(), note.title.clone(),
                score,
            ));
        }
    }

    results.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(MAX_RESULTS);
    results
}
