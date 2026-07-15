use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::sqlite::SharedConnection;

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
}

/// Read/write access to the category tables (`categories`, `category_rules`,
/// `category_overrides`). Shares a connection with `SessionStore` (the
/// tracker's write path) and `HistoryReader` (session reads) rather than
/// opening its own — SQLite only allows one writer at a time regardless of
/// connection count, so separate connections wouldn't add write concurrency,
/// only the risk of a `SQLITE_BUSY` error when two of them write at once.
pub struct CategoryStore {
    conn: SharedConnection,
}

impl CategoryStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, color FROM categories ORDER BY name")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Manually assigns `category_id` to every past and future session that
    /// shares this session's categorization key (e.g. all `chrome.exe` rows,
    /// or all `github.com` tab rows) — not just this one row. That mirrors
    /// how the built-in rules work and is what a user picking "always put
    /// GitHub under Development" actually expects.
    pub fn set_override(&self, session_id: i64, category_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let (key_kind, key): (String, String) = conn.query_row(
            "SELECT category_key_kind, category_key FROM sessions WHERE id = ?1",
            params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        conn.execute(
            "INSERT INTO category_overrides (key_kind, pattern, category_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(key_kind, pattern) DO UPDATE SET category_id = excluded.category_id",
            params![key_kind, key, category_id],
        )?;
        Ok(())
    }

    /// Removes a manual override, letting the built-in rules (or
    /// "Uncategorized") take back over for this session's key.
    pub fn clear_override(&self, session_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let found: Option<(String, String)> = conn
            .query_row(
                "SELECT category_key_kind, category_key FROM sessions WHERE id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((key_kind, key)) = found else {
            return Ok(());
        };
        conn.execute(
            "DELETE FROM category_overrides WHERE key_kind = ?1 AND pattern = ?2",
            params![key_kind, key],
        )?;
        Ok(())
    }
}
