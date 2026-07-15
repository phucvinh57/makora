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

#[derive(Debug, Clone, Serialize)]
pub struct CategoryRule {
    pub id: i64,
    pub key_kind: String,
    pub pattern: String,
    pub category_id: i64,
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

    pub fn create_category(&self, name: &str, color: &str) -> Result<Category> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO categories (name, color) VALUES (?1, ?2)", params![name, color])?;
        Ok(Category {
            id: conn.last_insert_rowid(),
            name: name.to_string(),
            color: color.to_string(),
        })
    }

    pub fn update_category(&self, id: i64, name: &str, color: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE categories SET name = ?1, color = ?2 WHERE id = ?3",
            params![name, color, id],
        )?;
        Ok(())
    }

    /// Deletes a category along with every rule and override that points at
    /// it, so sessions matching those patterns fall back to "Uncategorized"
    /// instead of leaving dangling foreign keys behind.
    pub fn delete_category(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM category_rules WHERE category_id = ?1", params![id])?;
        tx.execute("DELETE FROM category_overrides WHERE category_id = ?1", params![id])?;
        tx.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_rules(&self) -> Result<Vec<CategoryRule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, key_kind, pattern, category_id FROM category_rules ORDER BY key_kind, pattern")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CategoryRule {
                    id: row.get(0)?,
                    key_kind: row.get(1)?,
                    pattern: row.get(2)?,
                    category_id: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_rule(&self, key_kind: &str, pattern: &str, category_id: i64) -> Result<CategoryRule> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO category_rules (key_kind, pattern, category_id) VALUES (?1, ?2, ?3)",
            params![key_kind, pattern, category_id],
        )?;
        Ok(CategoryRule {
            id: conn.last_insert_rowid(),
            key_kind: key_kind.to_string(),
            pattern: pattern.to_string(),
            category_id,
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM category_rules WHERE id = ?1", params![id])?;
        Ok(())
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

    /// Same as [`Self::set_override`] but for many sessions at once — for
    /// batch-categorizing a filtered set of history rows from the UI.
    /// Sessions are looked up and written inside a single transaction so a
    /// large selection doesn't leave the database half-updated if one id is
    /// bad.
    pub fn set_overrides(&self, session_ids: &[i64], category_id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for session_id in session_ids {
            let (key_kind, key): (String, String) = tx.query_row(
                "SELECT category_key_kind, category_key FROM sessions WHERE id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            tx.execute(
                "INSERT INTO category_overrides (key_kind, pattern, category_id) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key_kind, pattern) DO UPDATE SET category_id = excluded.category_id",
                params![key_kind, key, category_id],
            )?;
        }
        tx.commit()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    fn temp_store() -> (CategoryStore, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("makora-category-store-test-{}.sqlite", uuid_ish()));
        let conn = open_connection(&path).expect("opening test database");
        (CategoryStore::new(conn), path)
    }

    // Good enough to avoid collisions between parallel test runs without
    // pulling in a `uuid` dependency just for test fixtures.
    fn uuid_ish() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64
    }

    #[test]
    fn category_and_rule_crud_round_trips_through_sqlite() {
        let (store, path) = temp_store();

        let seeded = store.list_categories().unwrap();
        assert!(!seeded.is_empty(), "default categories should be seeded");

        let created = store.create_category("Focus", "#123456").unwrap();
        assert_eq!(created.name, "Focus");
        assert!(store.list_categories().unwrap().iter().any(|c| c.id == created.id));

        store.update_category(created.id, "Deep Work", "#654321").unwrap();
        let updated = store
            .list_categories()
            .unwrap()
            .into_iter()
            .find(|c| c.id == created.id)
            .unwrap();
        assert_eq!(updated.name, "Deep Work");
        assert_eq!(updated.color, "#654321");

        let rule = store.create_rule("exe", "obs64.exe", created.id).unwrap();
        assert!(store.list_rules().unwrap().iter().any(|r| r.id == rule.id));

        store.delete_rule(rule.id).unwrap();
        assert!(!store.list_rules().unwrap().iter().any(|r| r.id == rule.id));

        let rule = store.create_rule("domain", "example.com", created.id).unwrap();
        store.delete_category(created.id).unwrap();
        assert!(!store.list_categories().unwrap().iter().any(|c| c.id == created.id));
        assert!(
            !store.list_rules().unwrap().iter().any(|r| r.id == rule.id),
            "deleting a category should cascade to its rules"
        );

        let _ = std::fs::remove_file(&path);
    }
}
