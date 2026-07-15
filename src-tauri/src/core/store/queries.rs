use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

/// One row from `sessions`, shaped for the UI's history page. `url` is
/// `Some` when the row represents a browser tab rather than a plain app
/// focus.
#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: i64,
    pub app_name: String,
    pub exe_path: String,
    pub window_title: String,
    pub url: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
}

/// A read-only view over the sessions database, separate from the write path
/// ([`crate::core::store::SqliteStore`]) so UI queries never contend with
/// the tracker's write lock. SQLite supports multiple connections to the
/// same file, so this just opens its own.
///
/// `rusqlite::Connection` is `Send` but not `Sync` (its statement cache uses
/// a `RefCell`), and a Tauri-managed state needs to be both since commands
/// can run on any thread — hence the `Mutex` even though this is read-only.
pub struct HistoryReader {
    conn: Mutex<Connection>,
}

impl HistoryReader {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("opening database at {}", path.display()))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Total number of app-focus sessions, for pagination.
    pub fn count_sessions(&self) -> Result<i64> {
        Ok(self
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?)
    }

    /// A page of activity rows (app focus and browser-tab rows alike),
    /// newest first.
    pub fn list_sessions(&self, limit: i64, offset: i64) -> Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, app_name, exe_path, window_title, url, start_time, end_time
             FROM sessions
             ORDER BY start_time DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(params![limit, offset], |row| {
                Ok(SessionRow {
                    id: row.get(0)?,
                    app_name: row.get(1)?,
                    exe_path: row.get(2)?,
                    window_title: row.get(3)?,
                    url: row.get(4)?,
                    start_time: row.get(5)?,
                    end_time: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
