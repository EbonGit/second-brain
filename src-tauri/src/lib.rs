use std::fs;
use std::sync::{Arc, Mutex};
use tauri::Manager;

mod db;
mod graph;
mod vault;
mod tfidf;
mod ollama;
mod zotero;
mod claude;
mod pdf;

pub use graph::{GraphData, GraphEdge, GraphNode};
pub use zotero::{ZoteroItem, ZoteroAnnotation};
pub use claude::{ChatMessage, ClaudeReply};
pub use vault::GhostLink;
pub use pdf::PdfSearchResult;

// ── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct NoteMeta {
    pub id: String,
    pub title: String,
    pub note_type: String,
    pub path: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub status: Option<String>,
    pub interest: Option<u8>,
    pub created: String,
    pub modified: String,
    pub maturity_score: u8,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Note {
    pub meta: NoteMeta,
    pub content: String,
    pub raw: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct Settings {
    /// true (default) = use `claude --print` CLI (subscription), false = use API key below
    pub use_claude_cli: Option<bool>,
    pub anthropic_api_key: Option<String>,
    /// Optional: custom path to zotero.sqlite. Auto-detected if absent.
    pub zotero_db_path: Option<String>,
    /// Optional: Zotero REST API credentials (only needed if no local DB)
    pub zotero_api_key: Option<String>,
    pub zotero_user_id: Option<String>,
}

/// Pending connection suggestion produced by the background incubator (C1).
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct IncubatorNotif {
    pub source_path: String,
    pub source_title: String,
    pub target_path: String,
    pub target_title: String,
    pub score: f32,
}

pub struct AppState {
    pub vault_path: Mutex<String>,
    pub settings: Mutex<Settings>,
    /// Paths of notes saved since app launch (for daily note summary)
    pub modified_today: Mutex<Vec<String>>,
    /// Background-produced connection suggestions (C1 incubator).
    /// Arc so it can be cloned into tokio::spawn without holding State<'_>.
    pub incubator_notifs: Arc<Mutex<Vec<IncubatorNotif>>>,
}

// CARGO_MANIFEST_DIR = src-tauri/ (compile-time constant, always correct)
const DEV_VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\..\\vault");

// ── Vault registry ────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct VaultEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct VaultRegistry {
    pub vaults: Vec<VaultEntry>,
    pub active: Option<String>,
}

fn registry_path() -> std::path::PathBuf {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let dir = std::path::Path::new(&local).join("jads-brain");
    let _ = fs::create_dir_all(&dir);
    dir.join("vaults.json")
}

fn load_registry() -> VaultRegistry {
    let path = registry_path();
    if path.exists() {
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(reg) = serde_json::from_str::<VaultRegistry>(&raw) {
                return reg;
            }
        }
    }
    VaultRegistry::default()
}

fn save_registry(reg: &VaultRegistry) {
    if let Ok(json) = serde_json::to_string_pretty(reg) {
        let _ = fs::write(registry_path(), json);
    }
}

/// Derive a display name from a vault path (parent folder name).
fn vault_name_from_path(path: &std::path::Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Vault".to_string())
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
async fn init_vault(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<String, String> {
    // 1. Determine default vault path (dev vs prod)
    let default_path = if cfg!(debug_assertions) {
        std::path::PathBuf::from(DEV_VAULT)
    } else {
        // In prod: prefer the sibling vault/ next to the exe (portable install),
        // then fall back to AppData.
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let portable = exe_dir.map(|d| d.join("vault"));
        match portable.filter(|p| p.exists()) {
            Some(p) => p,
            None => app.path().app_data_dir().map_err(|e| e.to_string())?.join("vault"),
        }
    };

    // Ensure Ollama is running (non-blocking — spawns in background)
    tokio::spawn(async {
        if !ollama::is_available().await {
            ollama::try_start_ollama();
        }
    });

    // 2. Check registry for an active vault override
    let mut reg = load_registry();
    let vault_path = if let Some(ref active) = reg.active {
        let p = std::path::PathBuf::from(active);
        if p.exists() || fs::create_dir_all(&p).is_ok() { p } else { default_path }
    } else {
        default_path
    };

    let vault_path = vault_path.canonicalize().unwrap_or_else(|_| {
        let _ = fs::create_dir_all(&vault_path);
        vault_path.canonicalize().unwrap_or(vault_path)
    });
    fs::create_dir_all(&vault_path).map_err(|e| e.to_string())?;
    let path_str = vault_path.to_string_lossy().to_string();
    *state.vault_path.lock().unwrap() = path_str.clone();

    // 3. Auto-register this vault if not present
    if !reg.vaults.iter().any(|v| v.path == path_str) {
        reg.vaults.push(VaultEntry {
            name: vault_name_from_path(&vault_path),
            path: path_str.clone(),
        });
    }
    reg.active = Some(path_str.clone());
    save_registry(&reg);

    eprintln!("[jads-brain] vault → {path_str}");
    eprintln!("[jads-brain] APPDATA={:?} USERPROFILE={:?}",
              std::env::var("APPDATA"), std::env::var("USERPROFILE"));
    let claude_cmd = format!("{}\\npm\\claude.cmd",
                             std::env::var("APPDATA").unwrap_or_default());
    eprintln!("[jads-brain] claude.cmd exists={}", std::path::Path::new(&claude_cmd).exists());

    // Create today's daily note if absent
    let _ = vault::ensure_daily_note(&path_str);

    // C2 — build initial vault summary if the file doesn't exist yet
    {
        let vp = path_str.clone();
        tokio::spawn(async move { vault::build_vault_summary(&vp); });
    }

    // Load persisted settings (global, stored next to executable / LOCALAPPDATA)
    let settings_path = std::path::Path::new(
        &std::env::var("LOCALAPPDATA").unwrap_or_default()
    ).join("jads-brain").join("settings.json");
    // Fallback: old per-vault location
    let legacy_path = vault_path.parent()
        .map(|p| p.join(".jb_settings.json"))
        .unwrap_or_else(|| vault_path.join(".jb_settings.json"));
    let load_path = if settings_path.exists() { settings_path }
                    else { legacy_path };
    if load_path.exists() {
        if let Ok(raw) = fs::read_to_string(&load_path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&raw) {
                *state.settings.lock().unwrap() = s;
            }
        }
    }

    Ok(path_str)
}

/// Return all registered vaults (name + path).
#[tauri::command]
async fn list_vaults() -> Vec<VaultEntry> {
    load_registry().vaults
}

/// Add a new vault to the registry (creates directory if absent).
#[tauri::command]
async fn add_vault(name: String, path: String) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| format!("Impossible de créer le dossier : {e}"))?;
    let _ = vault::ensure_daily_note(&path);
    let mut reg = load_registry();
    if !reg.vaults.iter().any(|v| v.path == path) {
        reg.vaults.push(VaultEntry { name, path });
        save_registry(&reg);
    }
    Ok(())
}

/// Rename a vault entry (by path).
#[tauri::command]
async fn rename_vault(path: String, name: String) -> Result<(), String> {
    let mut reg = load_registry();
    if let Some(v) = reg.vaults.iter_mut().find(|v| v.path == path) {
        v.name = name;
        save_registry(&reg);
    }
    Ok(())
}

/// Remove a vault from the registry (does NOT delete files on disk).
#[tauri::command]
async fn remove_vault(path: String) -> Result<(), String> {
    let mut reg = load_registry();
    reg.vaults.retain(|v| v.path != path);
    // If we removed the active vault, set active to first remaining
    if reg.active.as_deref() == Some(&path) {
        reg.active = reg.vaults.first().map(|v| v.path.clone());
    }
    save_registry(&reg);
    Ok(())
}

/// Switch the active vault — updates registry and AppState.
/// The frontend should call window.location.reload() after this.
#[tauri::command]
async fn switch_vault(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    // Ensure directory exists
    fs::create_dir_all(&path).map_err(|e| format!("Vault inaccessible : {e}"))?;
    // Update AppState
    *state.vault_path.lock().unwrap() = path.clone();
    *state.modified_today.lock().unwrap() = Vec::new();
    *state.incubator_notifs.lock().unwrap() = Vec::new();
    // Save to registry
    let mut reg = load_registry();
    if !reg.vaults.iter().any(|v| v.path == path) {
        let p = std::path::PathBuf::from(&path);
        reg.vaults.push(VaultEntry {
            name: vault_name_from_path(&p),
            path: path.clone(),
        });
    }
    reg.active = Some(path);
    save_registry(&reg);
    Ok(())
}

#[tauri::command]
async fn get_vault_notes(state: tauri::State<'_, AppState>) -> Result<Vec<NoteMeta>, String> {
    let path = state.vault_path.lock().unwrap().clone();
    vault::scan_vault(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_note(path: String) -> Result<Note, String> {
    vault::read_note_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_note(
    state: tauri::State<'_, AppState>,
    path: String,
    content: String,
) -> Result<(), String> {
    fs::write(&path, &content).map_err(|e| e.to_string())?;
    {
        let mut modified = state.modified_today.lock().unwrap();
        if !modified.contains(&path) {
            // First save of this note today → log it
            modified.push(path.clone());
            if let Ok(meta) = vault::read_note_meta(std::path::Path::new(&path)) {
                let vault_path = state.vault_path.lock().unwrap().clone();
                vault::log_to_daily(&vault_path, "Ce que j'ai fait",
                    &format!("Modifié [[{}]]", meta.title));
            }
        }
    }
    // C1 + C2 — background tasks after every save
    let vault_path = state.vault_path.lock().unwrap().clone();
    let saved_path = path.clone();
    let notifs_arc = Arc::clone(&state.incubator_notifs);
    tokio::spawn(async move {
        // C2 — keep the vault summary current for this note
        vault::update_vault_summary(&vault_path, &saved_path);

        if let Ok(notes) = vault::scan_vault(&vault_path) {
            let found = ollama::find_new_connections(&vault_path, &notes, &saved_path).await;
            if !found.is_empty() {
                let mut lock = notifs_arc.lock().unwrap();
                for (sp, st, tp, tt, sc) in found {
                    if !lock.iter().any(|e| e.source_path == sp && e.target_path == tp) {
                        lock.push(IncubatorNotif {
                            source_path: sp, source_title: st,
                            target_path: tp, target_title: tt,
                            score: sc,
                        });
                    }
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
async fn create_note(
    state: tauri::State<'_, AppState>,
    note_type: String,
    title: String,
) -> Result<String, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let file_path = vault::create_note_file(&vault_path, &note_type, &title)
        .map_err(|e| e.to_string())?;
    vault::log_to_daily(&vault_path, "Ce que j'ai fait", &format!("Ajouté [[{}]]", title));
    Ok(file_path)
}

#[tauri::command]
async fn delete_note(path: String) -> Result<(), String> {
    fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_graph_data(state: tauri::State<'_, AppState>) -> Result<GraphData, String> {
    let path = state.vault_path.lock().unwrap().clone();
    let notes = vault::scan_vault(&path).map_err(|e| e.to_string())?;
    // Prefer Ollama neural embeddings; fall back to TF-IDF when Ollama is offline
    let similarities = if ollama::is_available().await && ollama::has_embed_model().await {
        ollama::compute_similarities(&path, &notes).await
    } else {
        tfidf::compute_similarities(&notes)
    };
    Ok(graph::build_graph(&notes, &similarities))
}

#[derive(serde::Serialize)]
pub struct OllamaInfo {
    pub available: bool,
    pub has_model: bool,
    pub version: Option<String>,
    pub models: Vec<String>,
}

#[tauri::command]
async fn get_ollama_info() -> OllamaInfo {
    let (available, version, models) = ollama::get_status().await;
    let has_model = models.iter().any(|m| m.starts_with("nomic-embed-text"));
    OllamaInfo { available, has_model, version, models }
}

#[tauri::command]
async fn get_vault_path(state: tauri::State<'_, AppState>) -> Result<String, String> {
    Ok(state.vault_path.lock().unwrap().clone())
}

// ── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    // Global settings: stored in LOCALAPPDATA\jads-brain\settings.json
    let settings_path = std::path::Path::new(
        &std::env::var("LOCALAPPDATA").unwrap_or_default()
    ).join("jads-brain").join("settings.json");
    let _ = fs::create_dir_all(settings_path.parent().unwrap());
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&settings_path, json).map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

// ── Zotero ───────────────────────────────────────────────────────────────────

/// Returns the auto-detected zotero.sqlite path (or empty string if not found).
#[tauri::command]
async fn detect_zotero_db() -> String {
    zotero::find_zotero_db()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[tauri::command]
async fn fetch_zotero(state: tauri::State<'_, AppState>) -> Result<Vec<ZoteroItem>, String> {
    let s = state.settings.lock().unwrap().clone();

    // 1. Try custom path from settings
    // 2. Auto-detect the local SQLite database
    // 3. Fall back to REST API if credentials exist
    let db_path = s.zotero_db_path
        .filter(|p| !p.is_empty())
        .or_else(|| zotero::find_zotero_db().map(|p| p.to_string_lossy().to_string()));

    if let Some(path) = db_path {
        return zotero::read_local(&path).map_err(|e| {
            format!("Erreur lecture Zotero local ({path}): {e}")
        });
    }

    // REST API fallback
    let key = s.zotero_api_key.ok_or(
        "Base Zotero locale introuvable et aucune clé API configurée. \
         Indique le chemin vers zotero.sqlite dans Réglages."
    )?;
    let uid = s.zotero_user_id.ok_or("Zotero user ID non configuré")?;
    zotero::fetch_remote(&key, &uid).await.map_err(|e| e.to_string())
}

/// Patch interest + status in a note's frontmatter without touching the body.
#[tauri::command]
async fn update_note_meta(
    path: String,
    interest: Option<u8>,
    status: Option<String>,
) -> Result<(), String> {
    vault::update_note_frontmatter(&path, interest, status.as_deref())
        .map_err(|e| e.to_string())
}

/// Return top semantic keywords from the Ollama embedding cache (sync, no new embeddings).
#[tauri::command]
async fn get_semantic_keywords(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let path = state.vault_path.lock().unwrap().clone();
    let notes = vault::scan_vault(&path).map_err(|e| e.to_string())?;
    Ok(ollama::get_central_keywords(&path, &notes))
}

/// Import a Zotero item as a paper note in the vault.
#[tauri::command]
async fn import_zotero_item(
    state: tauri::State<'_, AppState>,
    item: ZoteroItem,
) -> Result<String, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    vault::create_paper_from_zotero(&vault_path, &item).map_err(|e| e.to_string())
}

// ── Claude ───────────────────────────────────────────────────────────────────

#[tauri::command]
async fn claude_explore(
    state: tauri::State<'_, AppState>,
    note_path: String,
    note_title: String,
    note_body: String,
) -> Result<ClaudeReply, String> {
    let s = state.settings.lock().unwrap().clone();
    let use_cli = s.use_claude_cli.unwrap_or(true);
    let api_key = s.anthropic_api_key.clone();
    let vault_path = state.vault_path.lock().unwrap().clone();
    eprintln!("[claude_explore] vault={} use_cli={} has_key={}", vault_path, use_cli, api_key.is_some());
    let notes = vault::scan_vault(&vault_path).map_err(|e| e.to_string())?;
    let summary = vault::load_vault_summary(&vault_path);
    let filtered = vault::prefilter_for_explore(&note_path, &note_title, &note_body, &notes, &summary, 15);
    let ctx = claude::vault_context(&filtered, &summary);
    eprintln!("[claude_explore] ctx_len={} notes_total={} notes_filtered={}", ctx.len(), notes.len(), filtered.len());
    let result = claude::explore_links(api_key.as_deref(), use_cli, &note_title, &note_body, &ctx)
        .await.map_err(|e| { eprintln!("[claude_explore] ERROR: {e}"); e.to_string() });
    // C3 — journal d'insights: append to vault/insights.md after every successful explore
    if let Ok(ref reply) = result {
        eprintln!("[claude_explore] OK");
        let _ = vault::append_insight(&vault_path, &note_title, &reply.text);
    }
    result
}

#[tauri::command]
async fn claude_chat(
    state: tauri::State<'_, AppState>,
    history: Vec<ChatMessage>,
) -> Result<ClaudeReply, String> {
    let s = state.settings.lock().unwrap().clone();
    let use_cli = s.use_claude_cli.unwrap_or(true);
    let api_key = s.anthropic_api_key.clone();
    let vault_path = state.vault_path.lock().unwrap().clone();
    let notes = vault::scan_vault(&vault_path).map_err(|e| e.to_string())?;
    let summary = vault::load_vault_summary(&vault_path);
    let ctx = claude::vault_context(&notes, &summary);
    // Log last user question to daily note
    if let Some(last_user) = history.iter().rev().find(|m| m.role == "user") {
        let q = last_user.content.chars().take(120).collect::<String>();
        vault::log_to_daily(&vault_path, "Conversations Claude", &q);
    }
    claude::chat(api_key.as_deref(), use_cli, history, &ctx, &vault_path).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_link_to_note(
    state: tauri::State<'_, AppState>,
    path: String,
    link_title: String,
) -> Result<(), String> {
    vault::append_link_to_note(&path, &link_title).map_err(|e| e.to_string())?;
    let vault_path = state.vault_path.lock().unwrap().clone();
    if let Ok(meta) = vault::read_note_meta(std::path::Path::new(&path)) {
        vault::log_to_daily(&vault_path, "Ce que j'ai fait",
            &format!("Lié [[{}]] et [[{}]]", meta.title, link_title));
    }
    Ok(())
}

#[tauri::command]
async fn fetch_zotero_annotations(
    state: tauri::State<'_, AppState>,
    zotero_key: String,
) -> Result<Vec<ZoteroAnnotation>, String> {
    let s = state.settings.lock().unwrap().clone();
    let db_path = s.zotero_db_path
        .filter(|p| !p.is_empty())
        .or_else(|| zotero::find_zotero_db().map(|p| p.to_string_lossy().to_string()))
        .ok_or("Base Zotero introuvable")?;
    zotero::fetch_annotations(&db_path, &zotero_key).map_err(|e| e.to_string())
}

/// Ghost wikilinks — [[Titles]] that don't match any vault note.
#[tauri::command]
async fn get_ghost_links(state: tauri::State<'_, AppState>) -> Result<Vec<GhostLink>, String> {
    let path = state.vault_path.lock().unwrap().clone();
    vault::get_ghost_links(&path).map_err(|e| e.to_string())
}

/// Paths of notes modified since app launch (for daily summary).
#[tauri::command]
async fn get_modified_today(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.modified_today.lock().unwrap().clone())
}

/// Append today's modified notes to the daily note.
#[tauri::command]
async fn finalize_daily_note(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let paths = state.modified_today.lock().unwrap().clone();
    vault::finalize_daily_note(&vault_path, &paths).map_err(|e| e.to_string())
}

/// Build (or refresh) the PDF full-text index from the local Zotero library.
#[tauri::command]
async fn build_pdf_index(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let s = state.settings.lock().unwrap().clone();
    let vault_path = state.vault_path.lock().unwrap().clone();
    let db_path = s.zotero_db_path
        .filter(|p| !p.is_empty())
        .or_else(|| zotero::find_zotero_db().map(|p| p.to_string_lossy().to_string()))
        .ok_or("Base Zotero introuvable")?;
    pdf::build_index(&vault_path, &db_path).map_err(|e| e.to_string())
}

/// Full-text search across indexed PDFs.
#[tauri::command]
async fn search_pdf_fulltext(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<Vec<PdfSearchResult>, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    pdf::search_fulltext(&vault_path, &query).map_err(|e| e.to_string())
}

/// Cluster notes by semantic similarity (k-means on Ollama embeddings).
/// Returns [{path, cluster}] — empty array if no embeddings cached yet.
#[derive(serde::Serialize)]
pub struct NoteCluster { pub path: String, pub cluster: usize }

#[tauri::command]
async fn get_clusters(state: tauri::State<'_, AppState>) -> Result<Vec<NoteCluster>, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let notes = vault::scan_vault(&vault_path).map_err(|e| e.to_string())?;
    let k = std::cmp::max(2, (notes.len() as f64).sqrt().round() as usize);
    let map = ollama::compute_clusters(&vault_path, &notes, k);
    Ok(map.into_iter().map(|(path, cluster)| NoteCluster { path, cluster }).collect())
}

/// Semantic search across vault notes using Ollama embeddings.
#[derive(serde::Serialize)]
pub struct SemanticResult { pub path: String, pub title: String, pub score: f32 }

#[tauri::command]
async fn search_semantic(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<Vec<SemanticResult>, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let notes = vault::scan_vault(&vault_path).map_err(|e| e.to_string())?;
    let results = ollama::semantic_search(&vault_path, &query, &notes)
        .await.map_err(|e| e.to_string())?;
    Ok(results.into_iter().map(|(path, title, score)| SemanticResult { path, title, score }).collect())
}

/// Ask Ollama to suggest tags for the current note (background — may be slow).
#[tauri::command]
async fn suggest_note_tags(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let note = vault::read_note_file(&path).map_err(|e| e.to_string())?;
    let _ = state; // keep state in scope for future use
    Ok(ollama::suggest_tags(&note.meta.title, &raw).await)
}

/// Apply suggested tags to a note's frontmatter.
#[tauri::command]
async fn apply_note_tags(path: String, tags: Vec<String>) -> Result<(), String> {
    vault::add_tags_to_note(&path, &tags).map_err(|e| e.to_string())
}

// ── C1 Incubateur ─────────────────────────────────────────────────────────────

/// Return pending incubator connection suggestions.
#[tauri::command]
async fn get_incubator_notifs(state: tauri::State<'_, AppState>) -> Result<Vec<IncubatorNotif>, String> {
    Ok(state.incubator_notifs.lock().unwrap().clone())
}

/// Remove one suggestion by index (user dismissed it).
#[tauri::command]
async fn dismiss_incubator_notif(state: tauri::State<'_, AppState>, index: usize) -> Result<(), String> {
    let mut lock = state.incubator_notifs.lock().unwrap();
    if index < lock.len() { lock.remove(index); }
    Ok(())
}

/// Accept a suggestion: add [[target_title]] wikilink to source_path, then remove matching notifs.
#[tauri::command]
async fn accept_incubator_connection(
    state: tauri::State<'_, AppState>,
    source_path: String,
    target_title: String,
) -> Result<(), String> {
    vault::append_link_to_note(&source_path, &target_title).map_err(|e| e.to_string())?;
    {
        let mut lock = state.incubator_notifs.lock().unwrap();
        lock.retain(|n| !(n.source_path == source_path && n.target_title == target_title));
    }
    // Log to daily note
    let vault_path = state.vault_path.lock().unwrap().clone();
    if let Ok(meta) = vault::read_note_meta(std::path::Path::new(&source_path)) {
        vault::log_to_daily(&vault_path, "Connexions créées",
            &format!("[[{}]] ↔ [[{}]] (Ollama)", meta.title, target_title));
    }
    Ok(())
}

/// Returns yesterday's date (YYYY-MM-DD) if its daily note has an empty "Ma journée" section.
/// Used by the frontend to show a morning reminder modal.
#[tauri::command]
async fn check_yesterday_daily(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    Ok(vault::check_yesterday_daily(&vault_path))
}

/// Debug: dump env vars and check claude.cmd existence — call via devtools console.
#[tauri::command]
async fn debug_env() -> String {
    let appdata  = std::env::var("APPDATA").unwrap_or_else(|_| "<not set>".into());
    let userprof = std::env::var("USERPROFILE").unwrap_or_else(|_| "<not set>".into());
    let path_env = std::env::var("PATH").unwrap_or_else(|_| "<not set>".into());
    let claude   = format!("{}\\npm\\claude.cmd", appdata);
    let exists   = std::path::Path::new(&claude).exists();
    format!(
        "APPDATA={}\nUSERPROFILE={}\nclaude.cmd={}\nexists={}\nPATH={}",
        appdata, userprof, claude, exists, &path_env[..path_env.len().min(500)]
    )
}

/// Append a clearly-labelled section to an existing note (used to persist Claude results).
/// Silently skips if the section already exists.
#[tauri::command]
async fn append_note_section(path: String, section_title: String, content: String) -> Result<(), String> {
    vault::append_section_to_note(&path, &section_title, &content)
        .map_err(|e| e.to_string())
}

// ── Phase D commands ─────────────────────────────────────────────────────────

/// Generate 5 Feynman questions for the active note (Claude).
#[tauri::command]
async fn claude_feynman(
    state: tauri::State<'_, AppState>,
    note_title: String,
    note_body: String,
) -> Result<ClaudeReply, String> {
    let (api_key, use_cli) = {
        let s = state.settings.lock().unwrap();
        (s.anthropic_api_key.clone(), s.use_claude_cli.unwrap_or(true))
    };
    claude::feynman(api_key.as_deref(), use_cli, &note_title, &note_body)
        .await.map_err(|e| e.to_string())
}

/// Detect contradictions / tensions around the active note (Claude + vault context).
#[tauri::command]
async fn claude_contradictions(
    state: tauri::State<'_, AppState>,
    note_path: String,
) -> Result<ClaudeReply, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let (api_key, use_cli) = {
        let s = state.settings.lock().unwrap();
        (s.anthropic_api_key.clone(), s.use_claude_cli.unwrap_or(true))
    };
    let note = vault::read_note_file(&note_path).map_err(|e| e.to_string())?;
    let notes = vault::scan_vault(&vault_path).map_err(|e| e.to_string())?;
    let summary = vault::load_vault_summary(&vault_path);
    let filtered = vault::prefilter_for_explore(&note_path, &note.meta.title, &note.content, &notes, &summary, 15);
    let vault_ctx = claude::vault_context(&filtered, &summary);
    claude::contradictions(api_key.as_deref(), use_cli, &note.meta.title, &note.content, &vault_ctx)
        .await.map_err(|e| e.to_string())
}

/// Synthesize selected notes into a new Tangle note (Claude → creates file).
#[tauri::command]
async fn claude_tangle(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
) -> Result<String, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let (api_key, use_cli) = {
        let s = state.settings.lock().unwrap();
        (s.anthropic_api_key.clone(), s.use_claude_cli.unwrap_or(true))
    };
    let mut notes_content = String::new();
    let mut titles: Vec<String> = Vec::new();
    for path in &paths {
        if let Ok(note) = vault::read_note_file(path) {
            titles.push(note.meta.title.clone());
            notes_content.push_str(&format!("### {}\n{}\n\n---\n\n", note.meta.title, note.content));
        }
    }
    if titles.is_empty() { return Err("Aucune note lisible".to_string()); }
    let reply = claude::tangle(api_key.as_deref(), use_cli, &notes_content, Some(&vault_path))
        .await.map_err(|e| e.to_string())?;
    let raw_title = if titles.len() <= 3 {
        format!("Tangle — {}", titles.join(", "))
    } else {
        format!("Tangle — {} notes", titles.len())
    };
    let tangle_title = if raw_title.len() > 80 {
        format!("Tangle — {} notes", paths.len())
    } else { raw_title };
    let path = vault::create_note_with_body(&vault_path, "tangle", &tangle_title, &reply.text)
        .map_err(|e| e.to_string())?;
    vault::log_to_daily(&vault_path, "Ce que j'ai fait", &format!("Ajouté [[{}]]", tangle_title));
    Ok(path)
}

/// Generate a stub note for a ghost link (Claude → creates file).
#[tauri::command]
async fn claude_generate_ghost(
    state: tauri::State<'_, AppState>,
    title: String,
    cited_by: Vec<String>,
) -> Result<String, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let (api_key, use_cli) = {
        let s = state.settings.lock().unwrap();
        (s.anthropic_api_key.clone(), s.use_claude_cli.unwrap_or(true))
    };
    let reply = claude::generate_note_stub(api_key.as_deref(), use_cli, &title, &cited_by)
        .await.map_err(|e| e.to_string())?;
    let path = vault::create_note_with_body(&vault_path, "note", &title, &reply.text)
        .map_err(|e| e.to_string())?;
    vault::log_to_daily(&vault_path, "Ce que j'ai fait", &format!("Ajouté [[{}]]", title));
    Ok(path)
}

/// Deep PDF summary for a Zotero item (Claude, reads from pdf index).
#[tauri::command]
async fn claude_pdf_summary(
    state: tauri::State<'_, AppState>,
    zotero_key: String,
) -> Result<ClaudeReply, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    let (api_key, use_cli) = {
        let s = state.settings.lock().unwrap();
        (s.anthropic_api_key.clone(), s.use_claude_cli.unwrap_or(true))
    };
    let (title, text) = pdf::get_pdf_entry(&vault_path, &zotero_key)
        .map_err(|e| e.to_string())?;
    claude::pdf_summary(api_key.as_deref(), use_cli, &title, &text)
        .await.map_err(|e| e.to_string())
}

/// Extract bibliographic references from a PDF (algorithm-only, no LLM).
#[tauri::command]
async fn extract_pdf_refs(
    state: tauri::State<'_, AppState>,
    zotero_key: String,
) -> Result<Vec<String>, String> {
    let vault_path = state.vault_path.lock().unwrap().clone();
    pdf::extract_references(&vault_path, &zotero_key)
        .map_err(|e| e.to_string())
}

// ── App entry ────────────────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            vault_path: Mutex::new(String::new()),
            settings: Mutex::new(Settings::default()),
            modified_today: Mutex::new(Vec::new()),
            incubator_notifs: Arc::new(Mutex::new(Vec::new())),
        })
        .invoke_handler(tauri::generate_handler![
            init_vault,
            get_vault_notes,
            read_note,
            save_note,
            create_note,
            delete_note,
            get_graph_data,
            get_vault_path,
            get_settings,
            save_settings,
            detect_zotero_db,
            fetch_zotero,
            import_zotero_item,
            claude_explore,
            claude_chat,
            get_ollama_info,
            update_note_meta,
            get_semantic_keywords,
            add_link_to_note,
            fetch_zotero_annotations,
            get_ghost_links,
            get_modified_today,
            finalize_daily_note,
            build_pdf_index,
            search_pdf_fulltext,
            get_clusters,
            search_semantic,
            suggest_note_tags,
            apply_note_tags,
            get_incubator_notifs,
            dismiss_incubator_notif,
            accept_incubator_connection,
            check_yesterday_daily,
            debug_env,
            append_note_section,
            claude_feynman,
            claude_contradictions,
            claude_tangle,
            claude_generate_ghost,
            claude_pdf_summary,
            extract_pdf_refs,
            list_vaults,
            add_vault,
            rename_vault,
            remove_vault,
            switch_vault,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
