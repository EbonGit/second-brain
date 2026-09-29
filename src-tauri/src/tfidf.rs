use crate::NoteMeta;
use std::collections::{HashMap, HashSet};

const STOP_WORDS: &[&str] = &[
    // French
    "les", "des", "une", "est", "que", "pour", "pas", "sur", "avec", "dans",
    "par", "qui", "son", "ses", "leur", "leurs", "mais", "donc", "car", "ces",
    "aux", "ont", "cette", "tout", "plus", "bien", "aussi", "comme", "lors",
    // English
    "the", "and", "for", "are", "was", "this", "that", "from", "with", "have",
    "not", "but", "they", "were", "been", "has", "had", "its", "our", "can",
    "will", "all", "one", "two", "may", "use", "also", "than", "each", "such",
];

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 4)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

fn cosine_sim(a: &HashMap<String, f32>, b: &HashMap<String, f32>) -> f32 {
    let dot: f32 = a.iter().filter_map(|(k, v)| b.get(k).map(|u| v * u)).sum();
    let norm_a: f32 = a.values().map(|v| v * v).sum::<f32>().sqrt();
    let norm_b: f32 = b.values().map(|v| v * v).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

/// Compute TF-IDF cosine similarities between all notes.
/// Returns (source_id, target_id, similarity) for pairs above threshold,
/// at most top_k similar notes per note (canonical pairs only, i < j in result).
pub fn compute_similarities(notes: &[NoteMeta]) -> Vec<(String, String, f32)> {
    if notes.len() < 2 { return vec![]; }

    // Read bodies from disk (path stored in NoteMeta.path)
    let bodies: Vec<String> = notes.iter()
        .map(|n| std::fs::read_to_string(&n.path).unwrap_or_default())
        .collect();

    let token_lists: Vec<Vec<String>> = bodies.iter().map(|b| tokenize(b)).collect();

    // Document frequency per term
    let n = notes.len() as f32;
    let mut df: HashMap<String, f32> = HashMap::new();
    for tokens in &token_lists {
        let unique: HashSet<&String> = tokens.iter().collect();
        for t in unique { *df.entry(t.clone()).or_insert(0.0) += 1.0; }
    }

    // IDF (smoothed)
    let idf: HashMap<String, f32> = df.iter()
        .map(|(t, d)| (t.clone(), ((n + 1.0) / (d + 1.0)).ln() + 1.0))
        .collect();

    // TF-IDF vectors
    let vecs: Vec<HashMap<String, f32>> = token_lists.iter().map(|tokens| {
        let total = tokens.len() as f32;
        if total == 0.0 { return HashMap::new(); }
        let mut tf: HashMap<String, f32> = HashMap::new();
        for t in tokens { *tf.entry(t.clone()).or_insert(0.0) += 1.0; }
        tf.iter().map(|(t, c)| {
            let v = (c / total) * idf.get(t).copied().unwrap_or(1.0);
            (t.clone(), v)
        }).collect()
    }).collect();

    let threshold = 0.08_f32;
    let top_k = 3_usize;
    let mut results: Vec<(String, String, f32)> = vec![];
    let mut emitted: HashSet<(String, String)> = HashSet::new();

    for i in 0..notes.len() {
        let mut sims: Vec<(usize, f32)> = (0..notes.len())
            .filter(|&j| j != i)
            .filter_map(|j| {
                let s = cosine_sim(&vecs[i], &vecs[j]);
                if s >= threshold { Some((j, s)) } else { None }
            })
            .collect();
        sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (j, sim) in sims.into_iter().take(top_k) {
            // Canonical pair: always smaller index first
            let (a, b) = if i < j { (i, j) } else { (j, i) };
            let key = (notes[a].id.clone(), notes[b].id.clone());
            if emitted.insert(key) {
                results.push((notes[a].id.clone(), notes[b].id.clone(), sim));
            }
        }
    }

    results
}
