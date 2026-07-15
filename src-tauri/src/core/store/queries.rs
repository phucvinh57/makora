use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use super::sqlite::SharedConnection;

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: i64,
    pub app_name: String,
    pub exe_path: String,
    pub window_title: String,
    pub url: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub category_is_override: bool,
}
pub struct HistoryReader {
    conn: SharedConnection,
}

impl HistoryReader {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
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
    /// newest first. Category is resolved at read time — an override wins
    /// over a rule via `COALESCE` — rather than stored on the row, so
    /// editing a rule or override re-categorizes historical sessions
    /// immediately instead of only affecting future ones.
    pub fn list_sessions(&self, limit: i64, offset: i64) -> Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.app_name, s.exe_path, s.window_title, s.url, s.start_time, s.end_time,
                    cat.id, cat.name, cat.color, ov.category_id IS NOT NULL
             FROM sessions s
             LEFT JOIN category_overrides ov
                    ON ov.key_kind = s.category_key_kind AND ov.pattern = s.category_key
             LEFT JOIN category_rules r
                    ON r.key_kind = s.category_key_kind AND r.pattern = s.category_key
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(ov.category_id, r.category_id)
             ORDER BY s.start_time DESC
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
                    category_id: row.get(7)?,
                    category_name: row.get(8)?,
                    category_color: row.get(9)?,
                    category_is_override: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
