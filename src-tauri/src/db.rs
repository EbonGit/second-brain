use anyhow::Result;
use rusqlite::Connection;

pub fn init_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notes (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL,
            note_type   TEXT NOT NULL,
            path        TEXT NOT NULL,
            tags        TEXT DEFAULT '[]',
            updated_at  TEXT
        );
        CREATE TABLE IF NOT EXISTS edges (
            source      TEXT NOT NULL,
            target      TEXT NOT NULL,
            edge_type   TEXT NOT NULL,
            weight      REAL DEFAULT 1.0,
            PRIMARY KEY (source, target, edge_type)
        );",
    )?;
    Ok(conn)
}
