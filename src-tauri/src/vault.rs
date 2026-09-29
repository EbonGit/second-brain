use crate::{Note, NoteMeta};
use crate::zotero::ZoteroItem;
use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct GhostLink {
    pub title: String,
    pub cited_by: Vec<String>,
}

pub fn scan_vault(vault_path: &str) -> Result<Vec<NoteMeta>> {
    let vault = Path::new(vault_path);
    if !vault.exists() {
        fs::create_dir_all(vault)?;
        return Ok(Vec::new());
    }
    let mut notes: Vec<NoteMeta> = WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|x| x == "md").unwrap_or(false)
                && !e.path().components().any(|c| c.as_os_str() == "templates")
        })
        .filter_map(|e| read_note_meta(e.path()).ok())
        .collect();
    notes.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(notes)
}

pub fn read_note_meta(path: &Path) -> Result<NoteMeta> {
    let raw = fs::read_to_string(path)?;
    let (fm, body) = parse_frontmatter(&raw);

    let title = fm
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_str().unwrap_or("Untitled"))
        .to_string();

    let note_type = detect_note_type(path);

    let tags: Vec<String> = fm
        .get("tags")
        .and_then(|v| v.as_sequence())
        .map(|s| s.iter()
            .filter_map(|t| t.as_str())
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect())
        .unwrap_or_default();

    let links = extract_wikilinks(&raw);
    let status = fm.get("status").and_then(|v| v.as_str()).map(String::from);
    let interest = fm.get("interest").and_then(|v| v.as_u64()).map(|v| v as u8);

    let meta = fs::metadata(path)?;
    let modified = meta
        .modified()
        .map(|t| chrono::DateTime::<Utc>::from(t).to_rfc3339())
        .unwrap_or_default();

    let created = fm
        .get("created")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let maturity_score = compute_maturity(links.len(), tags.len(), body.len(), interest);

    Ok(NoteMeta {
        id: path.to_string_lossy().to_string(),
        title,
        note_type,
        path: path.to_string_lossy().to_string(),
        tags,
        links,
        status,
        interest,
        created,
        modified,
        maturity_score,
    })
}

fn compute_maturity(links: usize, tags: usize, body_len: usize, interest: Option<u8>) -> u8 {
    let link_score  = (links.min(10) * 5) as u8;            // 0–50
    let tag_score   = (tags.min(5) * 4) as u8;              // 0–20
    let len_score   = ((body_len / 500).min(10) * 2) as u8; // 0–20
    let int_score   = interest.unwrap_or(0) * 2;            // 0–10
    link_score + tag_score + len_score + int_score
}

pub fn read_note_file(path: &str) -> Result<Note> {
    let raw = fs::read_to_string(path)?;
    let (_, body) = parse_frontmatter(&raw);
    let meta = read_note_meta(Path::new(path))?;
    Ok(Note { meta, content: body, raw })
}

pub fn create_note_file(vault_path: &str, note_type: &str, title: &str) -> Result<String> {
    let dir = Path::new(vault_path).join(note_type_to_dir(note_type));
    fs::create_dir_all(&dir)?;
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let slug = slugify(title);
    let path = dir.join(format!("{}-{}.md", date, slug));
    fs::write(&path, generate_template(note_type, title, &date))?;
    Ok(path.to_string_lossy().to_string())
}

/// Pre-filter vault notes by keyword overlap with the active note.
/// Returns at most `max` notes most relevant to the active note's content.
/// The active note itself is always excluded from results.
pub fn prefilter_for_explore(
    active_path: &str,
    active_title: &str,
    active_body: &str,
    notes: &[NoteMeta],
    summaries: &HashMap<String, String>,
    max: usize,
) -> Vec<NoteMeta> {
    let stop: HashSet<&str> = [
        "le","la","les","de","du","des","un","une","et","est","en","au","aux",
        "que","qui","je","il","elle","nous","vous","ils","elles","se","sa","son",
        "leur","leurs","ce","ces","par","sur","dans","avec","pour","plus","pas",
        "ne","cette","tout","mais","donc","car","ni","the","a","an","of","in",
        "to","is","and","or","it","that","this","are","was","be","as","at","by",
        "on","from","but","with","have","has","had","not","for","its","also",
        "can","into","when","they","been","more","than","if","so","about",
        "which","he","she","we","our","their","will","would","could","should",
        "may","might","just","only","even","such","use","used","both","each",
        "any","some","all","one","two","new","other","these","those","how",
        "what","where","why","who","were","then","there","while","still","very",
        "well","here","now","time","way","year","work","first","last","long",
        "great","little","own","right","old","large","next","early","important",
    ].iter().cloned().collect();

    let active_text = format!("{} {}", active_title, active_body).to_lowercase();
    let keywords: HashSet<String> = active_text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3 && !stop.contains(*w))
        .map(|w| w.to_string())
        .collect();

    let mut scored: Vec<(usize, usize, NoteMeta)> = notes
        .iter()
        .filter(|n| n.path != active_path)
        .enumerate()
        .map(|(i, note)| {
            let mut searchable = format!("{} {}", note.title, note.tags.join(" ")).to_lowercase();
            if let Some(snippet) = summaries.get(&note.path) {
                let truncated: String = snippet.chars().take(200).collect();
                searchable.push(' ');
                searchable.push_str(&truncated.to_lowercase());
            }
            let score = if keywords.is_empty() { 0 } else {
                keywords.iter().filter(|kw| searchable.contains(kw.as_str())).count()
            };
            (score, i, note.clone())
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(max).map(|(_, _, n)| n).collect()
}

/// Append a new `## section_title` block at the end of a note file.
/// No-op if that section already exists (to avoid duplicates on re-request).
pub fn append_section_to_note(path: &str, section_title: &str, content: &str) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    let header = format!("## {section_title}");
    if raw.contains(&header) {
        return Ok(());
    }
    let trimmed = raw.trim_end();
    let new_content = format!("{trimmed}\n\n{header}\n\n{content}\n");
    fs::write(path, new_content)?;
    Ok(())
}

pub fn create_note_with_body(vault_path: &str, note_type: &str, title: &str, body: &str) -> Result<String> {
    let dir = Path::new(vault_path).join(note_type_to_dir(note_type));
    fs::create_dir_all(&dir)?;
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let slug = slugify(title);
    let path = dir.join(format!("{}-{}.md", date, slug));
    let content = format!(
        "---\ntitle: \"{title}\"\nnote_type: {note_type}\ntags: []\nstatus: non-lu\ninterest: 0\ncreated: {date}\n---\n\n{body}"
    );
    fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

pub fn create_paper_from_zotero(vault_path: &str, item: &ZoteroItem) -> Result<String> {
    let dir = std::path::Path::new(vault_path).join("papers");
    fs::create_dir_all(&dir)?;
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let slug = slugify(&item.title);
    let path = dir.join(format!("{}-{}.md", date, slug));
    let authors_yaml = item.authors.iter()
        .map(|a| format!("  - {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tags_yaml = item.tags.iter()
        .map(|t| format!("  - {t}"))
        .collect::<Vec<_>>()
        .join("\n");
    let doi = item.doi.as_deref().unwrap_or("");
    let year = item.year.as_deref().unwrap_or("");
    let abstract_body = item.abstract_note.as_deref().unwrap_or("");
    let content = format!(
"---
title: \"{title}\"
note_type: paper
citekey: \"\"
doi: \"{doi}\"
authors:\n{authors_yaml}
year: {year}
status: non-lu
interest: 0
tags:\n{tags_yaml}
created: {date}
zotero_key: \"{zkey}\"
---

## Highlights

## Ma synthèse

{abstract_body}

## Questions ouvertes

## Liens
",
        title = item.title.replace('"', "\\\""),
        zkey = item.key,
    );
    fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

/// Patch `interest` and/or `status` in a note's YAML frontmatter in-place,
/// preserving all other fields and the body exactly.
pub fn update_note_frontmatter(path: &str, interest: Option<u8>, status: Option<&str>) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    if !raw.starts_with("---") { return Ok(()); }
    let rest = &raw[3..];
    let Some(end) = rest.find("\n---") else { return Ok(()) };

    let fm_section = &rest[..end];
    let body = &rest[end + 4..]; // content after closing ---

    let mut lines: Vec<String> = fm_section.lines().map(String::from).collect();
    let mut found_interest = false;
    let mut found_status = false;

    for line in &mut lines {
        // Check booleans first (drops the borrow), then mutate
        let is_interest = line.trim_start().starts_with("interest:");
        let is_status   = line.trim_start().starts_with("status:");
        if interest.is_some() && is_interest {
            *line = format!("interest: {}", interest.unwrap());
            found_interest = true;
        } else if status.is_some() && is_status {
            *line = format!("status: {}", status.unwrap());
            found_status = true;
        }
    }
    if interest.is_some() && !found_interest {
        lines.push(format!("interest: {}", interest.unwrap()));
    }
    if status.is_some() && !found_status {
        lines.push(format!("status: {}", status.unwrap()));
    }

    let new_content = format!("---\n{}\n---{}", lines.join("\n"), body);
    fs::write(path, new_content)?;
    Ok(())
}

/// Append extra tags to the `tags:` list in a note's YAML frontmatter, skipping duplicates.
/// Handles both inline (`tags: [a, b]`) and block-style (`tags:\n  - a`) YAML.
/// Always normalises to lowercase and rewrites as inline format without quotes.
pub fn add_tags_to_note(path: &str, new_tags: &[String]) -> Result<()> {
    if new_tags.is_empty() { return Ok(()); }
    let raw = fs::read_to_string(path)?;
    if !raw.starts_with("---") { return Ok(()); }
    let rest = &raw[3..];
    let Some(fm_end) = rest.find("\n---") else { return Ok(()) };

    let fm_section = &rest[..fm_end];
    let body = &rest[fm_end + 4..];

    let mut lines: Vec<String> = fm_section.lines().map(String::from).collect();
    let mut tags_start: Option<usize> = None;   // index of the "tags:" line
    let mut tags_end:   Option<usize> = None;   // last line of block-style items (inclusive)
    let mut existing: Vec<String> = vec![];

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("tags:") {
            tags_start = Some(i);
            if let Some(bracket) = t.find('[') {
                // Inline: tags: [a, b, c]
                let inner = &t[bracket + 1..];
                if let Some(close) = inner.find(']') {
                    existing = inner[..close].split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            } else {
                // Block-style: collect "  - tag" lines that follow
                let mut j = i + 1;
                while j < lines.len() {
                    let next = lines[j].trim();
                    if let Some(rest_tag) = next.strip_prefix("- ") {
                        existing.push(rest_tag.trim().trim_matches('"').trim_matches('\'').to_lowercase());
                        tags_end = Some(j);
                        j += 1;
                    } else {
                        break;
                    }
                }
            }
            break;
        }
    }

    let to_add: Vec<String> = new_tags.iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty() && !existing.contains(t))
        .collect();
    if to_add.is_empty() { return Ok(()); }

    let mut all_tags = existing;
    all_tags.extend(to_add);
    let new_tags_line = format!("tags: [{}]", all_tags.join(", "));

    match (tags_start, tags_end) {
        (Some(start), Some(end)) => {
            // Block-style: replace the "tags:" line + all "  - item" lines with single inline line
            lines.drain(start..=end);
            lines.insert(start, new_tags_line);
        }
        (Some(start), None) => {
            lines[start] = new_tags_line;
        }
        _ => {
            lines.push(new_tags_line);
        }
    }

    let new_content = format!("---\n{}\n---{}", lines.join("\n"), body);
    fs::write(path, new_content)?;
    Ok(())
}

/// Append `[[link_title]]` to the "## Liens" section of a note, or at the end of the file.
pub fn append_link_to_note(path: &str, link_title: &str) -> Result<()> {
    let mut content = fs::read_to_string(path)?;
    let link_md = format!("- [[{}]]", link_title);

    // Avoid duplicates
    if content.contains(&format!("[[{}]]", link_title)) {
        return Ok(());
    }

    if let Some(pos) = content.find("\n## Liens") {
        // Find where this section ends (next ## or EOF)
        let after = &content[pos + 1..];
        let section_end = after.find("\n## ")
            .map(|p| pos + 1 + p)
            .unwrap_or(content.len());
        // Insert before next section or at end of Liens section
        let insert = if content[pos..section_end].ends_with('\n') {
            format!("{}\n", link_md)
        } else {
            format!("\n{}\n", link_md)
        };
        content.insert_str(section_end, &insert);
    } else {
        if !content.ends_with('\n') { content.push('\n'); }
        content.push_str(&format!("\n## Liens\n\n{}\n", link_md));
    }

    fs::write(path, content)?;
    Ok(())
}

/// Append a Claude insight entry to `vault_path/insights.md`.
pub fn append_insight(vault_path: &str, note_title: &str, insight: &str) -> Result<()> {
    use std::io::Write;
    let path = Path::new(vault_path).join("insights.md");
    let date = chrono::Utc::now().format("%Y-%m-%d");
    let entry = format!("\n## {}\n**[{}]**\n{}\n\n---\n", date, note_title, insight.trim());
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(entry.as_bytes())?;
    Ok(())
}

/// Detect [[Wikilinks]] that don't correspond to any existing note title.
pub fn get_ghost_links(vault_path: &str) -> Result<Vec<GhostLink>> {
    let notes = scan_vault(vault_path)?;
    let title_set: HashSet<String> = notes.iter()
        .map(|n| n.title.to_lowercase())
        .collect();

    let mut ghost_map: HashMap<String, Vec<String>> = HashMap::new();
    for note in &notes {
        for link in &note.links {
            if !title_set.contains(&link.to_lowercase()) {
                ghost_map.entry(link.clone()).or_default().push(note.title.clone());
            }
        }
    }
    Ok(ghost_map.into_iter()
        .map(|(title, cited_by)| GhostLink { title, cited_by })
        .collect())
}

/// Create today's daily note if it doesn't exist yet; return its path.
pub fn ensure_daily_note(vault_path: &str) -> Result<String> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let dir = Path::new(vault_path).join("daily");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.md", today));
    if !path.exists() {
        let content = generate_template("daily", &today, &today);
        fs::write(&path, content)?;
    }
    Ok(path.to_string_lossy().to_string())
}

/// Append the list of today's modified notes to the daily note.
pub fn finalize_daily_note(vault_path: &str, modified_paths: &[String]) -> Result<()> {
    if modified_paths.is_empty() { return Ok(()); }
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let path = Path::new(vault_path).join("daily").join(format!("{}.md", today));
    if !path.exists() { return Ok(()); }

    let mut content = fs::read_to_string(&path)?;
    let titles: Vec<String> = modified_paths.iter()
        .filter_map(|p| read_note_meta(Path::new(p)).ok())
        .map(|m| format!("- [[{}]]", m.title))
        .collect();

    let section = format!("\n## Notes modifiées aujourd'hui\n\n{}\n", titles.join("\n"));
    // Replace existing section if present, else append
    if let Some(pos) = content.find("\n## Notes modifiées aujourd'hui") {
        let end = content[pos + 1..].find("\n## ")
            .map(|p| pos + 1 + p)
            .unwrap_or(content.len());
        content.replace_range(pos..end, &section);
    } else {
        content.push_str(&section);
    }
    fs::write(&path, content)?;
    Ok(())
}

// ── Daily note — auto-logging ─────────────────────────────────────────────────

/// Append `- line` into a named section of today's daily note.
/// Creates the section at the end if not present. No-op if the file doesn't exist.
pub fn log_to_daily(vault_path: &str, section: &str, line: &str) {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let note_path = Path::new(vault_path)
        .join("daily")
        .join(format!("{}.md", today));
    if !note_path.exists() { return; }
    let Ok(raw) = fs::read_to_string(&note_path) else { return; };

    let header = format!("## {}", section);
    let entry  = format!("- {}", line);

    let mut lines: Vec<String> = raw.lines().map(String::from).collect();
    let section_idx = lines.iter().position(|l| l.trim() == header.as_str());

    if let Some(idx) = section_idx {
        // Find first line of the next section (or EOF)
        let next_section = lines[idx + 1..].iter()
            .position(|l| l.starts_with("## "))
            .map(|i| idx + 1 + i)
            .unwrap_or(lines.len());
        // Insert before trailing blank lines
        let mut insert_at = next_section;
        while insert_at > idx + 1 && lines[insert_at - 1].trim().is_empty() {
            insert_at -= 1;
        }
        lines.insert(insert_at, entry);
    } else {
        if lines.last().map(|l| !l.is_empty()).unwrap_or(false) {
            lines.push(String::new());
        }
        lines.push(String::new());
        lines.push(header);
        lines.push(String::new());
        lines.push(entry);
    }

    let _ = fs::write(&note_path, lines.join("\n") + "\n");
}

/// Returns the date string (YYYY-MM-DD) of yesterday's daily note if it exists
/// and its "Ma journée" section is empty — used for the morning reminder.
pub fn check_yesterday_daily(vault_path: &str) -> Option<String> {
    let yesterday = (Utc::now() - chrono::TimeDelta::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let path = Path::new(vault_path)
        .join("daily")
        .join(format!("{}.md", yesterday));
    if !path.exists() { return None; }
    let Ok(content) = fs::read_to_string(&path) else { return None; };

    if let Some(pos) = content.find("## Ma journée") {
        let after = &content[pos + "## Ma journée".len()..];
        let end = after.find("\n## ").unwrap_or(after.len());
        if after[..end].trim().is_empty() {
            return Some(yesterday);
        }
    }
    None
}

// ── C2 : Vault summary (première phrase par note) ────────────────────────────

fn summary_file(vault_path: &str) -> std::path::PathBuf {
    Path::new(vault_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".jb_vault_summary.json")
}

/// First non-trivial line from the note body, capped at 120 chars.
fn extract_first_sentence(body: &str) -> String {
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() { continue; }
        if t.starts_with('#') || t.starts_with('-') || t.starts_with('*')
            || t.starts_with('[') || t.starts_with('!') || t.starts_with('>') { continue; }
        let s: String = t.chars().take(120).collect();
        if s.len() >= 10 { return s; }
    }
    String::new()
}

pub fn load_vault_summary(vault_path: &str) -> HashMap<String, String> {
    fs::read_to_string(summary_file(vault_path))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Read the note at `note_path`, extract its first sentence, and persist into
/// `.jb_vault_summary.json`.  Called in the background after every save.
pub fn update_vault_summary(vault_path: &str, note_path: &str) {
    if let Ok(raw) = fs::read_to_string(note_path) {
        let (_, body) = parse_frontmatter(&raw);
        let sentence = extract_first_sentence(&body);
        let mut map = load_vault_summary(vault_path);
        map.insert(note_path.to_string(), sentence);
        if let Ok(json) = serde_json::to_string(&map) {
            let _ = fs::write(summary_file(vault_path), json);
        }
    }
}

/// One-time build of the summary from all existing notes.  Skipped if the file
/// already exists so restarts are cheap.
pub fn build_vault_summary(vault_path: &str) {
    let path = summary_file(vault_path);
    if path.exists() { return; }
    let mut map: HashMap<String, String> = HashMap::new();
    for entry in WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false)
                    && !e.path().components().any(|c| c.as_os_str() == "templates"))
    {
        if let Ok(raw) = fs::read_to_string(entry.path()) {
            let (_, body) = parse_frontmatter(&raw);
            map.insert(
                entry.path().to_string_lossy().to_string(),
                extract_first_sentence(&body),
            );
        }
    }
    if let Ok(json) = serde_json::to_string(&map) {
        let _ = fs::write(path, json);
    }
}

fn parse_frontmatter(raw: &str) -> (serde_yaml::Value, String) {
    if !raw.starts_with("---") {
        return (serde_yaml::Value::Null, raw.to_string());
    }
    let rest = &raw[3..];
    if let Some(end) = rest.find("\n---") {
        let fm = serde_yaml::from_str(&rest[..end]).unwrap_or(serde_yaml::Value::Null);
        return (fm, rest[end + 4..].trim_start().to_string());
    }
    (serde_yaml::Value::Null, raw.to_string())
}

fn extract_wikilinks(content: &str) -> Vec<String> {
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    re.captures_iter(content).map(|c| c[1].to_string()).collect()
}

fn detect_note_type(path: &Path) -> String {
    path.components().rev().nth(1)
        .and_then(|c| c.as_os_str().to_str())
        .map(|s| match s {
            "papers" => "paper", "ideas" => "idea",
            "experiments" => "experiment", "tangle" => "tangle",
            "daily" => "daily", _ => "note",
        })
        .unwrap_or("note")
        .to_string()
}

fn note_type_to_dir(t: &str) -> &str {
    match t {
        "paper" => "papers", "idea" => "ideas",
        "experiment" => "experiments", "tangle" => "tangle",
        "daily" => "daily", _ => "notes",
    }
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn generate_template(note_type: &str, title: &str, date: &str) -> String {
    match note_type {
        "paper" => format!(
"---
title: \"{title}\"
note_type: paper
citekey: \"\"
doi: \"\"
authors: []
year:
status: non-lu
interest: 0
tags: []
created: {date}
---

## Highlights

## Ma synthèse

## Questions ouvertes

## Liens
"),
        "idea" => format!(
"---
title: \"{title}\"
note_type: idea
maturity: seed
tags: []
source_papers: []
created: {date}
---

## L'idée

## Pourquoi c'est intéressant

## Liens
"),
        "experiment" => format!(
"---
title: \"{title}\"
note_type: experiment
status: en-cours
hypothesis: \"\"
tags: []
linked_ideas: []
created: {date}
---

## Protocole

## Résultats

## Interprétation

## Liens
"),
        "tangle" => format!(
"---
title: \"{title}\"
note_type: tangle
theme: \"\"
papers: []
ideas: []
experiments: []
tags: []
created: {date}
---

## Thème central

## Synthèse

## Tensions et contradictions

## Ce que j'en retiens
"),
        "daily" => format!(
"---
title: \"{title}\"
note_type: daily
date: {date}
tags: []
created: {date}
---

## Ma journée

## Ce que j'ai lu

## Idées du jour

## Ce que j'ai fait

## Connexions créées

## Conversations Claude
"),
        _ => format!(
"---
title: \"{title}\"
note_type: note
tags: []
created: {date}
---

"),
    }
}
