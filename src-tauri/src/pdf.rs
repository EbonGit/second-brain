use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PdfSearchResult {
    pub zotero_key: String,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct PdfIndex {
    /// zotero_key → (title, text)
    entries: HashMap<String, (String, String)>,
}

fn index_path(vault_path: &str) -> std::path::PathBuf {
    Path::new(vault_path).parent()
        .unwrap_or(Path::new(vault_path))
        .join(".jb_pdf_index.json")
}

pub fn build_index(vault_path: &str, db_path: &str) -> Result<usize> {
    let items = crate::zotero::read_local(db_path)?;
    let mut index = load_index(vault_path);
    let mut added = 0usize;

    for item in &items {
        if index.entries.contains_key(&item.key) { continue; }
        if let Some(pdf_path) = crate::zotero::get_pdf_path(db_path, &item.key)? {
            eprintln!("[pdf] extracting {}", pdf_path);
            // pdf-extract can panic on unusual color spaces — catch_unwind to skip gracefully
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                pdf_extract::extract_text(&pdf_path)
            }));
            match result {
                Ok(Ok(text)) => {
                    let truncated = text.chars().take(200_000).collect::<String>();
                    eprintln!("[pdf] indexed '{}' ({} chars)", item.title, truncated.len());
                    index.entries.insert(item.key.clone(), (item.title.clone(), truncated));
                    added += 1;
                }
                Ok(Err(e)) => eprintln!("[pdf] extract error {}: {e}", pdf_path),
                Err(_)     => eprintln!("[pdf] extract panic skipped (complex PDF): {}", pdf_path),
            }
        }
    }

    save_index(vault_path, &index)?;
    Ok(added)
}

pub fn search_fulltext(vault_path: &str, query: &str) -> Result<Vec<PdfSearchResult>> {
    let index = load_index(vault_path);
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for (key, (title, text)) in &index.entries {
        let text_lower = text.to_lowercase();
        if let Some(pos) = text_lower.find(&query_lower) {
            // Work with char vectors to avoid panicking on multibyte UTF-8 (accents, symbols)
            let chars: Vec<char> = text.chars().collect();
            // Convert byte offset to char index using text_lower (same char layout as text)
            let char_pos = text_lower[..pos].chars().count();
            let start_char = char_pos.saturating_sub(80);
            let end_char   = (char_pos + query_lower.chars().count() + 80).min(chars.len());

            // Extend to word boundaries
            let start_char = (0..start_char).rev()
                .find(|&i| chars[i] == ' ')
                .map(|i| i + 1)
                .unwrap_or(start_char);
            let end_char = (end_char..chars.len())
                .find(|&i| chars[i] == ' ')
                .unwrap_or(chars.len());

            // Reconstruct excerpt from char slice
            let excerpt_text: String = chars[start_char..end_char].iter()
                .collect::<String>()
                .replace('\n', " ");
            let excerpt = format!("…{excerpt_text}…");

            results.push(PdfSearchResult {
                zotero_key: key.clone(),
                title: title.clone(),
                excerpt,
            });
        }
    }

    Ok(results)
}

pub fn get_pdf_entry(vault_path: &str, zotero_key: &str) -> Result<(String, String)> {
    let index = load_index(vault_path);
    index.entries.get(zotero_key)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("PDF non indexé pour la clé «{zotero_key}» — clique sur «Indexer les PDFs» d'abord"))
}

pub fn extract_references(vault_path: &str, zotero_key: &str) -> Result<Vec<String>> {
    let (_, text) = get_pdf_entry(vault_path, zotero_key)?;
    let text_lower = text.to_lowercase();

    // Find the references section (search from the end)
    let markers = ["references\n", "bibliography\n", "références\n", "literature cited\n", "works cited\n"];
    let start = markers.iter()
        .filter_map(|m| text_lower.rfind(m).map(|pos| pos + m.len()))
        .max()
        .unwrap_or_else(|| text.len().saturating_sub(6000));

    let refs_text = &text[start..];

    // Split by numbered reference patterns: [1], 1., (1)
    let re_num = Regex::new(r"(?m)^\s*(?:\[\d+\]|\d+\.|\(\d+\))\s+").unwrap();
    let mut refs: Vec<String> = re_num.split(refs_text)
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.split('\n').next().unwrap_or("").trim().to_string())
        .filter(|s| s.len() > 10)
        .take(50)
        .collect();

    if refs.is_empty() {
        // Fallback: lines that look like reference entries (contain comma + year pattern)
        let re_year = Regex::new(r"\b(19|20)\d{2}\b").unwrap();
        refs = refs_text.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.len() > 20 && l.len() < 400 && l.contains(',') && re_year.is_match(l))
            .take(40)
            .collect();
    }

    Ok(refs)
}

fn load_index(vault_path: &str) -> PdfIndex {
    let p = index_path(vault_path);
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_index(vault_path: &str, index: &PdfIndex) -> Result<()> {
    let p = index_path(vault_path);
    let json = serde_json::to_string(index)?;
    fs::write(p, json)?;
    Ok(())
}
