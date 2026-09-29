use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ZoteroAnnotation {
    pub text: String,
    pub comment: Option<String>,
    pub color: Option<String>,
    pub page_label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ZoteroItem {
    pub key: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<String>,
    pub doi: Option<String>,
    pub abstract_note: Option<String>,
    pub item_type: String,
    pub tags: Vec<String>,
    pub url: Option<String>,
}

/// Try common Zotero data directory locations on Windows.
pub fn find_zotero_db() -> Option<std::path::PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    let candidates = [
        format!("{}\\Zotero\\zotero.sqlite", profile),
        format!("{}\\AppData\\Roaming\\Zotero\\Zotero\\zotero.sqlite", profile),
        format!("{}\\Documents\\Zotero\\zotero.sqlite", profile),
    ];
    for path in &candidates {
        let p = std::path::PathBuf::from(path);
        if p.exists() { return Some(p); }
    }
    None
}

/// Read items directly from the local Zotero SQLite database.
/// Uses SQLite's `immutable=1` URI mode to bypass all locking — works even when
/// Zotero is running and holds an OS-level exclusive lock (Windows error 32).
pub fn read_local(db_path: &str) -> anyhow::Result<Vec<ZoteroItem>> {
    let conn = open_db(db_path)?;

    // Get all non-attachment items with key fields
    let mut stmt = conn.prepare(
        "SELECT i.itemID, i.key, it.typeName
         FROM items i
         JOIN itemTypes it ON i.itemTypeID = it.itemTypeID
         WHERE it.typeName NOT IN ('attachment','note','annotation')
           AND i.itemID NOT IN (SELECT itemID FROM deletedItems)
         ORDER BY i.dateModified DESC
         LIMIT 200",
    )?;

    let item_ids: Vec<(i64, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut items = Vec::new();

    for (id, key, item_type) in item_ids {
        let title = field_value(&conn, id, "title").unwrap_or_default();
        if title.is_empty() { continue; }

        let doi     = field_value(&conn, id, "DOI");
        let date    = field_value(&conn, id, "date");
        let abs     = field_value(&conn, id, "abstractNote");
        let url     = field_value(&conn, id, "url");

        let year = date.as_deref()
            .and_then(|d| d.split(['-', '/']).next().map(|y| y.trim().to_string()))
            .filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));

        let authors = creators(&conn, id);
        let tags    = item_tags(&conn, id);

        items.push(ZoteroItem {
            key,
            title,
            authors,
            year,
            doi,
            abstract_note: abs,
            item_type,
            tags,
            url,
        });
    }

    Ok(items)
}

fn field_value(conn: &rusqlite::Connection, item_id: i64, field: &str) -> Option<String> {
    conn.query_row(
        "SELECT idv.value
         FROM itemData id
         JOIN fields f      ON id.fieldID  = f.fieldID
         JOIN itemDataValues idv ON id.valueID = idv.valueID
         WHERE id.itemID = ?1 AND f.fieldName = ?2",
        rusqlite::params![item_id, field],
        |row| row.get(0),
    ).ok()
}

fn creators(conn: &rusqlite::Connection, item_id: i64) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT c.lastName, c.firstName
         FROM creators c
         JOIN itemCreators ic ON c.creatorID = ic.creatorID
         WHERE ic.itemID = ?1
         ORDER BY ic.orderIndex",
    ) { Ok(s) => s, Err(_) => return vec![] };
    stmt.query_map(rusqlite::params![item_id], |row| {
        let last: String  = row.get(0)?;
        let first: String = row.get(1).unwrap_or_default();
        Ok(if first.is_empty() { last } else { format!("{last}, {}", &first[..1.min(first.len())]) })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
    .unwrap_or_default()
}

fn item_tags(conn: &rusqlite::Connection, item_id: i64) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT t.name FROM tags t
         JOIN itemTags it ON t.tagID = it.tagID
         WHERE it.itemID = ?1",
    ) { Ok(s) => s, Err(_) => return vec![] };
    stmt.query_map(rusqlite::params![item_id], |row| row.get::<_, String>(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        .unwrap_or_default()
}

fn open_db(db_path: &str) -> anyhow::Result<rusqlite::Connection> {
    use rusqlite::{Connection, OpenFlags};
    let fwd = db_path.replace('\\', "/");
    let path = fwd.trim_start_matches('/');
    let encoded = path.replace(' ', "%20").replace('#', "%23");
    let uri = format!("file:///{}?immutable=1", encoded);
    Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).map_err(|e| anyhow::anyhow!("Impossible d'ouvrir zotero.sqlite: {e}"))
}

/// Return PDF highlights/annotations for a Zotero item (identified by its key).
/// Requires Zotero 6+ with the built-in PDF reader; returns empty if the table doesn't exist.
pub fn fetch_annotations(db_path: &str, zotero_key: &str) -> anyhow::Result<Vec<ZoteroAnnotation>> {
    let conn = open_db(db_path)?;

    // Find item ID for this key
    let item_id: i64 = match conn.query_row(
        "SELECT itemID FROM items WHERE key = ?1",
        [zotero_key],
        |row| row.get(0),
    ) {
        Ok(id) => id,
        Err(_) => return Ok(vec![]),
    };

    // Get highlights from PDF attachments of this item
    let mut stmt = match conn.prepare(
        "SELECT ia.text, ia.comment, ia.color, ia.pageLabel
         FROM itemAnnotations ia
         JOIN items att ON ia.parentItemID = att.itemID
         JOIN itemAttachments atd ON att.itemID = atd.itemID
         WHERE atd.parentItemID = ?1
           AND ia.type = 'highlight'
           AND ia.text IS NOT NULL AND ia.text != ''
         ORDER BY ia.sortIndex
         LIMIT 80",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]),   // table doesn't exist in older Zotero
    };

    let result = stmt.query_map([item_id], |row| {
        Ok(ZoteroAnnotation {
            text:       row.get(0)?,
            comment:    row.get(1).ok().flatten(),
            color:      row.get(2).ok().flatten(),
            page_label: row.get(3).ok().flatten(),
        })
    })?.filter_map(|r| r.ok()).collect();

    Ok(result)
}

/// Return the local filesystem path to the PDF for a Zotero item.
/// Supports stored files (linkMode=0) and linked files (linkMode=1).
pub fn get_pdf_path(db_path: &str, zotero_key: &str) -> anyhow::Result<Option<String>> {
    let conn = open_db(db_path)?;

    let item_id: i64 = match conn.query_row(
        "SELECT itemID FROM items WHERE key = ?1",
        [zotero_key],
        |row| row.get(0),
    ) {
        Ok(id) => id,
        Err(_) => return Ok(None),
    };

    // Find a PDF attachment
    let row: Option<(String, Option<String>, i64)> = conn.query_row(
        "SELECT i.key, ia.path, ia.linkMode
         FROM itemAttachments ia
         JOIN items i ON ia.itemID = i.itemID
         WHERE ia.parentItemID = ?1
           AND ia.contentType = 'application/pdf'
           AND ia.linkMode IN (0, 1)
         LIMIT 1",
        [item_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).ok();

    let Some((att_key, path, link_mode)) = row else { return Ok(None) };
    let profile = std::env::var("USERPROFILE").unwrap_or_default();

    // linkMode 0 = imported file, 1 = imported URL — both stored in ~/Zotero/storage/<att_key>/
    // linkMode 2 = linked file (absolute path), 3 = linked URL
    let pdf_path = if link_mode == 0 || link_mode == 1 {
        let storage_dir = format!("{}\\Zotero\\storage\\{}", profile, att_key);
        let raw_path = path.as_deref().unwrap_or("");
        // Zotero prefixes stored-file paths with "storage:" — strip it
        let filename = raw_path.trim_start_matches("storage:").split(['\\', '/']).last().unwrap_or("");

        let candidate = if !filename.is_empty() {
            let p = format!("{}\\{}", storage_dir, filename);
            if std::path::Path::new(&p).exists() { Some(p) } else { None }
        } else {
            None
        };

        // Fallback: scan the storage folder for any PDF (handles truncated/mismatched filenames)
        candidate.unwrap_or_else(|| {
            std::fs::read_dir(&storage_dir)
                .ok()
                .and_then(|entries| {
                    entries.filter_map(|e| e.ok())
                        .find(|e| e.path().extension().map(|x| x == "pdf").unwrap_or(false))
                        .map(|e| e.path().to_string_lossy().to_string())
                })
                .unwrap_or_default()
        })
    } else {
        // linkMode 2: absolute path to a file not in Zotero storage
        path.unwrap_or_default()
    };

    eprintln!("[get_pdf_path] key={zotero_key} att_key={att_key} path={pdf_path}");

    if pdf_path.is_empty() || !std::path::Path::new(&pdf_path).exists() {
        return Ok(None);
    }
    Ok(Some(pdf_path))
}

/// Kept for users who DO have a Zotero account and prefer cloud sync.
pub async fn fetch_remote(api_key: &str, user_id: &str) -> anyhow::Result<Vec<ZoteroItem>> {
    let url = format!(
        "https://api.zotero.org/users/{}/items\
         ?format=json&limit=50&sort=dateModified&direction=desc",
        user_id
    );
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header("Zotero-API-Key", api_key)
        .header("User-Agent", "JadsBrain/0.1")
        .send().await?.json().await?;

    let items = resp.as_array().cloned().unwrap_or_default();
    let mut result = Vec::new();
    for item in &items {
        let data = &item["data"];
        let t = data["itemType"].as_str().unwrap_or("");
        if matches!(t, "attachment"|"note"|"annotation") { continue; }
        let title = data["title"].as_str().unwrap_or("").to_string();
        if title.is_empty() { continue; }
        let key = item["key"].as_str().unwrap_or("").to_string();
        let authors: Vec<String> = data["creators"].as_array().unwrap_or(&vec![])
            .iter().filter_map(|c| c["lastName"].as_str().map(String::from)).collect();
        let year = data["date"].as_str()
            .and_then(|d| d.split('-').next().map(String::from))
            .filter(|y| y.len() == 4);
        let doi  = data["DOI"].as_str().map(String::from).filter(|s| !s.is_empty());
        let abs  = data["abstractNote"].as_str().map(String::from).filter(|s| !s.is_empty());
        let url  = data["url"].as_str().map(String::from).filter(|s| !s.is_empty());
        let tags = data["tags"].as_array().unwrap_or(&vec![])
            .iter().filter_map(|t| t["tag"].as_str().map(String::from)).collect();
        result.push(ZoteroItem { key, title, authors, year, doi, abstract_note: abs,
            item_type: t.to_string(), tags, url });
    }
    Ok(result)
}
