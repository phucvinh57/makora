use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use crate::model::AppInfo;
use crate::store::SessionStore;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    app_name      TEXT NOT NULL,
    exe_path      TEXT NOT NULL,
    window_title  TEXT NOT NULL,
    start_time    TEXT NOT NULL,
    end_time      TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_exe_path ON sessions(exe_path);
CREATE INDEX IF NOT EXISTS idx_sessions_start_time ON sessions(start_time);

CREATE TABLE IF NOT EXISTS tab_sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  INTEGER NOT NULL REFERENCES sessions(id),
    url         TEXT NOT NULL,
    title       TEXT NOT NULL,
    start_time  TEXT NOT NULL,
    end_time    TEXT
);
CREATE INDEX IF NOT EXISTS idx_tab_sessions_session_id ON tab_sessions(session_id);
";

/// A `SessionStore` backed by a local SQLite file. Timestamps are stored as
/// RFC 3339 strings in UTC.
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating database directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening database at {}", path.display()))?;
        conn.execute_batch(SCHEMA).context("initializing schema")?;
        Ok(Self { conn })
    }
}

impl SessionStore for SqliteStore {
    fn start_session(&mut self, app: &AppInfo, window_title: &str, at: DateTime<Utc>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, start_time) VALUES (?1, ?2, ?3, ?4)",
            params![app.app_name, app.exe_path, window_title, at.to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET end_time = ?1 WHERE id = ?2",
            params![at.to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn close_dangling_sessions(&mut self) -> Result<()> {
        // A session still open at startup means the previous run never saw
        // its end (crash, kill, power loss). We can't know the real end
        // time, so close it at its own start rather than "now" — that
        // undercounts one session instead of silently inflating it by
        // however long the machine was off.
        self.conn.execute(
            "UPDATE sessions SET end_time = start_time WHERE end_time IS NULL",
            [],
        )?;
        self.conn.execute(
            "UPDATE tab_sessions SET end_time = start_time WHERE end_time IS NULL",
            [],
        )?;
        Ok(())
    }

    fn start_tab_session(&mut self, session_id: i64, url: &str, title: &str, at: DateTime<Utc>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tab_sessions (session_id, url, title, start_time) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, url, title, at.to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn end_tab_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "UPDATE tab_sessions SET end_time = ?1 WHERE id = ?2",
            params![at.to_rfc3339(), id],
        )?;
        Ok(())
    }
}
