use anyhow::{anyhow, bail, Context, Result};
use rusqlite::params;
use serde::Serialize;

use super::sqlite::SharedConnection;
use crate::core::category::{subtree_ids_sql, KeyKind, MatchKind};

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryRule {
    pub id: i64,
    pub key_kind: String,
    pub pattern: String,
    pub match_kind: String,
    pub category_id: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RecategorizeOutcome {
    pub pins_cleared: i64,
}

pub struct CategoryStore {
    conn: SharedConnection,
}

/// Names only have to be unique among siblings, so the raw constraint error would be
/// misleading — the same name elsewhere in the tree is fine.
fn name_clash(err: rusqlite::Error, name: &str, parent_id: Option<i64>) -> anyhow::Error {
    let unique_violation = matches!(
        err.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    ) && err.to_string().contains("UNIQUE");
    if !unique_violation {
        return err.into();
    }
    match parent_id {
        Some(_) => anyhow!("a subcategory named \"{name}\" already exists under this parent"),
        None => anyhow!("a top-level category named \"{name}\" already exists"),
    }
}

/// Turns the foreign key's opaque "constraint failed" into something the UI can show.
fn ensure_exists(conn: &rusqlite::Connection, category_id: i64) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)",
        params![category_id],
        |row| row.get(0),
    )?;
    if !exists {
        bail!("category {category_id} does not exist");
    }
    Ok(())
}

impl CategoryStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    /// Every category, flat — the caller (the UI) assembles the tree from `parent_id`.
    /// Siblings come out in the order they should be shown under their parent.
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, color, parent_id FROM categories ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    parent_id: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_category(
        &self,
        name: &str,
        color: &str,
        parent_id: Option<i64>,
    ) -> Result<Category> {
        let conn = self.conn.lock().unwrap();
        if let Some(parent_id) = parent_id {
            ensure_exists(&conn, parent_id)?;
        }
        conn.execute(
            "INSERT INTO categories (name, color, parent_id) VALUES (?1, ?2, ?3)",
            params![name, color, parent_id],
        )
        .map_err(|err| name_clash(err, name, parent_id))?;
        Ok(Category {
            id: conn.last_insert_rowid(),
            name: name.to_string(),
            color: color.to_string(),
            parent_id,
        })
    }

    pub fn update_category(
        &self,
        id: i64,
        name: &str,
        color: &str,
        parent_id: Option<i64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if let Some(parent_id) = parent_id {
            ensure_exists(&conn, parent_id)?;
            // Re-parenting a category under itself or under one of its own descendants would
            // cut that whole branch loose from every root.
            let cycles: bool = conn.query_row(
                &format!("SELECT ?2 IN ({})", subtree_ids_sql("SELECT ?1")),
                params![id, parent_id],
                |row| row.get(0),
            )?;
            if cycles {
                bail!("a category cannot be moved inside itself");
            }
        }
        conn.execute(
            "UPDATE categories SET name = ?1, color = ?2, parent_id = ?3 WHERE id = ?4",
            params![name, color, parent_id, id],
        )
        .map_err(|err| name_clash(err, name, parent_id))?;
        Ok(())
    }

    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its rules and pins with it.
    pub fn delete_category(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // Deepest first: a parent still referenced by one of its children cannot be deleted.
        let doomed: Vec<i64> = {
            let mut stmt = tx.prepare(
                "WITH RECURSIVE subtree(id, depth) AS (
                    SELECT ?1, 0
                    UNION
                    SELECT c.id, subtree.depth + 1 FROM categories c JOIN subtree ON c.parent_id = subtree.id
                )
                SELECT id FROM subtree ORDER BY depth DESC",
            )?;
            let ids = stmt
                .query_map(params![id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        for category_id in &doomed {
            tx.execute(
                "DELETE FROM category_rules WHERE category_id = ?1",
                params![category_id],
            )?;
            tx.execute(
                "UPDATE sessions SET category_id = NULL WHERE category_id = ?1",
                params![category_id],
            )?;
            tx.execute(
                "DELETE FROM work_session_categories WHERE category_id = ?1",
                params![category_id],
            )?;
            tx.execute("DELETE FROM categories WHERE id = ?1", params![category_id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_rules(&self) -> Result<Vec<CategoryRule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, key_kind, pattern, match_kind, category_id FROM category_rules ORDER BY key_kind, pattern",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CategoryRule {
                    id: row.get(0)?,
                    key_kind: row.get(1)?,
                    pattern: row.get(2)?,
                    match_kind: row.get(3)?,
                    category_id: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_rule(
        &self,
        key_kind: &str,
        pattern: &str,
        match_kind: &str,
        category_id: i64,
    ) -> Result<CategoryRule> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            bail!("rule pattern must not be empty");
        }
        KeyKind::parse(key_kind).ok_or_else(|| anyhow!("invalid key kind: {key_kind}"))?;
        let kind = MatchKind::parse(match_kind)
            .ok_or_else(|| anyhow!("invalid match kind: {match_kind}"))?;
        if kind == MatchKind::Regex {
            regex::Regex::new(pattern)
                .with_context(|| format!("invalid regex pattern: {pattern}"))?;
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id) VALUES (?1, ?2, ?3, ?4)",
            params![key_kind, pattern, kind.as_str(), category_id],
        )?;
        Ok(CategoryRule {
            id: conn.last_insert_rowid(),
            key_kind: key_kind.to_string(),
            pattern: pattern.to_string(),
            match_kind: kind.as_str().to_string(),
            category_id,
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM category_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn set_row_category(&self, session_id: i64, category_id: Option<i64>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET category_id = ?1 WHERE id = ?2",
            params![category_id, session_id],
        )?;
        Ok(())
    }

    pub fn set_row_categories(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for session_id in session_ids {
            tx.execute(
                "UPDATE sessions SET category_id = ?1 WHERE id = ?2",
                params![category_id, session_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn recategorize_all(&self) -> Result<RecategorizeOutcome> {
        let pins_cleared = self.conn.lock().unwrap().execute(
            "UPDATE sessions SET category_id = NULL WHERE category_id IS NOT NULL",
            [],
        )?;
        Ok(RecategorizeOutcome {
            pins_cleared: pins_cleared as i64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    fn temp_store() -> (CategoryStore, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("makora-category-store-test-{}.sqlite", uuid_ish()));
        let conn = open_connection(&path).expect("opening test database");
        (CategoryStore::new(conn), path)
    }

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

        assert!(
            store.list_categories().unwrap().is_empty(),
            "a fresh database starts with no taxonomy at all — the default rules are generated, not seeded here"
        );

        let created = store.create_category("Focus", "#123456", None).unwrap();
        assert_eq!(created.name, "Focus");
        assert!(store
            .list_categories()
            .unwrap()
            .iter()
            .any(|c| c.id == created.id));

        store
            .update_category(created.id, "Deep Work", "#654321", None)
            .unwrap();
        let updated = store
            .list_categories()
            .unwrap()
            .into_iter()
            .find(|c| c.id == created.id)
            .unwrap();
        assert_eq!(updated.name, "Deep Work");
        assert_eq!(updated.color, "#654321");

        let rule = store
            .create_rule("exe", "obs64.exe", "exact", created.id)
            .unwrap();
        assert!(store.list_rules().unwrap().iter().any(|r| r.id == rule.id));

        store.delete_rule(rule.id).unwrap();
        assert!(!store.list_rules().unwrap().iter().any(|r| r.id == rule.id));

        let rule = store
            .create_rule("domain", "example.com", "exact", created.id)
            .unwrap();
        store.delete_category(created.id).unwrap();
        assert!(!store
            .list_categories()
            .unwrap()
            .iter()
            .any(|c| c.id == created.id));
        assert!(
            !store.list_rules().unwrap().iter().any(|r| r.id == rule.id),
            "deleting a category should cascade to its rules"
        );

        let _ = std::fs::remove_file(&path);
    }

    fn insert_session(conn: &SharedConnection, exe: &str, start_time: &str) -> i64 {
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time)
             VALUES (?1, ?1, 'title', ?1, 'exe', ?2)",
            params![exe, start_time],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_browser_session(conn: &SharedConnection, url: &str, start_time: &str) -> i64 {
        let (key_kind, key) = crate::core::category::category_key("chrome.exe", Some(url));
        let url_key = crate::core::category::url_key(url);
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key, start_time)
             VALUES ('chrome.exe', 'chrome.exe', 'title', ?1, ?2, ?3, ?4, ?5)",
            params![url, key, key_kind.as_str(), url_key, start_time],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn recategorize_all_erases_pins_and_leaves_rules_deciding() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let ruled = store.create_category("Ruled", "#111111", None).unwrap();
        let manual = store.create_category("Manual", "#222222", None).unwrap();
        store
            .create_rule("exe", "ruled.exe", "exact", ruled.id)
            .unwrap();

        let by_rule = insert_session(&conn, "ruled.exe", "2024-06-15T00:00:00Z");
        let by_pin = insert_session(&conn, "pinned.exe", "2024-06-15T02:00:00Z");
        let pinned_over_rule = insert_session(&conn, "ruled.exe", "2024-06-15T03:00:00Z");
        store.set_row_category(by_pin, Some(manual.id)).unwrap();
        store
            .set_row_category(pinned_over_rule, Some(manual.id))
            .unwrap();

        assert_eq!(store.recategorize_all().unwrap().pins_cleared, 2);

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(by_rule), Some(ruled.id));
        assert_eq!(category_of(by_pin), None);
        assert_eq!(
            category_of(pinned_over_rule),
            Some(ruled.id),
            "a pinned row falls back to what the rules say, not to nothing"
        );

        assert!(store
            .list_rules()
            .unwrap()
            .iter()
            .any(|r| r.pattern == "ruled.exe"));

        assert_eq!(store.recategorize_all().unwrap().pins_cleared, 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn category_resolution_prefers_row_pin_over_rule() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let rule_cat = store.create_category("RuleCat", "#111111", None).unwrap();
        let pin_cat = store.create_category("PinCat", "#333333", None).unwrap();
        store
            .create_rule("exe", "shared.exe", "exact", rule_cat.id)
            .unwrap();

        let a = insert_session(&conn, "shared.exe", "2024-06-15T00:00:00Z");
        let b = insert_session(&conn, "shared.exe", "2024-06-16T00:00:00Z");

        let category_of = |rows: &[crate::core::store::queries::SessionRow], id: i64| {
            rows.iter().find(|r| r.id == id).unwrap().category_id
        };

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, a), Some(rule_cat.id));
        assert_eq!(category_of(&rows, b), Some(rule_cat.id));

        store.set_row_category(b, Some(pin_cat.id)).unwrap();
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, a), Some(rule_cat.id));
        assert_eq!(category_of(&rows, b), Some(pin_cat.id));

        store.set_row_category(b, None).unwrap();
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, b), Some(rule_cat.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_row_categories_pins_only_the_given_sessions() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let english = store.create_category("English", "#111111", None).unwrap();
        let a = insert_browser_session(&conn, "https://chatgpt.com/c/AAA", "2024-06-15T00:00:00Z");
        let b = insert_browser_session(&conn, "https://chatgpt.com/c/BBB", "2024-06-15T01:00:00Z");
        let c = insert_browser_session(&conn, "https://chatgpt.com/c/CCC", "2024-06-15T02:00:00Z");

        store.set_row_categories(&[a, b], Some(english.id)).unwrap();

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(a), Some(english.id));
        assert_eq!(category_of(b), Some(english.id));
        assert_eq!(
            category_of(c),
            None,
            "an unselected row sharing the same key must be left alone"
        );
        assert!(
            !store
                .list_rules()
                .unwrap()
                .iter()
                .any(|r| r.pattern == "chatgpt.com"),
            "pinning rows must not create a key-wide rule"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recategorize_all_clears_bulk_pins_across_every_app_and_site() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let pin_cat = store.create_category("PinCat", "#333333", None).unwrap();
        let a = insert_session(&conn, "pinned.exe", "2024-06-15T00:00:00Z");
        let b = insert_browser_session(&conn, "https://chatgpt.com/c/AAA", "2024-06-15T01:00:00Z");
        store.set_row_categories(&[a, b], Some(pin_cat.id)).unwrap();

        assert_eq!(store.recategorize_all().unwrap().pins_cleared, 2);

        let still_pinned: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE category_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(still_pinned, 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn substring_rule_matches_any_key_containing_the_pattern() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let cat = store.create_category("Games", "#111111", None).unwrap();
        store
            .create_rule("exe", "steam", "substring", cat.id)
            .unwrap();

        let a = insert_session(&conn, "steamwebhelper.exe", "2024-06-15T00:00:00Z");
        let b = insert_session(&conn, "notepad.exe", "2024-06-15T00:00:00Z");

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(a), Some(cat.id));
        assert_eq!(category_of(b), None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn regex_rule_matches_keys_against_the_compiled_pattern() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let cat = store.create_category("Work", "#222222", None).unwrap();
        store
            .create_rule("domain", r"^[\w-]+\.corp\.example\.com$", "regex", cat.id)
            .unwrap();

        let a = insert_session(&conn, "chrome.exe", "2024-06-15T00:00:00Z");
        {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE sessions SET category_key = 'wiki.corp.example.com', category_key_kind = 'domain' WHERE id = ?1",
                params![a],
            )
            .unwrap();
        }
        let b = insert_session(&conn, "chrome.exe", "2024-06-15T00:00:01Z");
        {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE sessions SET category_key = 'evil-corp.example.com.attacker.net', category_key_kind = 'domain' WHERE id = ?1",
                params![b],
            )
            .unwrap();
        }

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(a), Some(cat.id));
        assert_eq!(
            category_of(b),
            None,
            "regex is anchored, so a lookalike suffix should not match"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_regex_pattern_is_rejected_up_front() {
        let (store, path) = temp_store();
        let cat = store.create_category("Bad", "#333333", None).unwrap();
        assert!(store
            .create_rule("exe", "(unclosed", "regex", cat.id)
            .is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn exact_rule_beats_substring_rule_regardless_of_category_creation_order() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let broad = store.create_category("Broad", "#444444", None).unwrap();
        let specific = store.create_category("Specific", "#555555", None).unwrap();
        store
            .create_rule("exe", "steam", "substring", broad.id)
            .unwrap();
        store
            .create_rule("exe", "steamwebhelper.exe", "exact", specific.id)
            .unwrap();

        let a = insert_session(&conn, "steamwebhelper.exe", "2024-06-15T00:00:00Z");
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(a), Some(specific.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn prefix_rule_beats_substring_and_loses_to_exact() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let substring_cat = store.create_category("Substring", "#111111", None).unwrap();
        let prefix_cat = store.create_category("Prefix", "#222222", None).unwrap();
        let exact_cat = store.create_category("Exact", "#333333", None).unwrap();
        store
            .create_rule("exe", "editor", "substring", substring_cat.id)
            .unwrap();
        store
            .create_rule("exe", "myeditor", "prefix", prefix_cat.id)
            .unwrap();
        store
            .create_rule("exe", "myeditor.exe", "exact", exact_cat.id)
            .unwrap();

        let a = insert_session(&conn, "myeditor.exe", "2024-06-15T00:00:00Z");
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(
            rows.iter().find(|r| r.id == a).unwrap().category_id,
            Some(exact_cat.id)
        );

        store
            .delete_rule(
                store
                    .list_rules()
                    .unwrap()
                    .into_iter()
                    .find(|r| r.category_id == exact_cat.id)
                    .unwrap()
                    .id,
            )
            .unwrap();
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(
            rows.iter().find(|r| r.id == a).unwrap().category_id,
            Some(prefix_cat.id),
            "with the exact rule gone, the prefix rule should win over the substring rule"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn longer_pattern_wins_within_the_same_match_kind() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let short_cat = store.create_category("Short", "#111111", None).unwrap();
        let long_cat = store.create_category("Long", "#222222", None).unwrap();
        store
            .create_rule("domain", "example.net", "substring", short_cat.id)
            .unwrap();
        store
            .create_rule("domain", "corp.example.net", "substring", long_cat.id)
            .unwrap();

        let a = insert_browser_session(
            &conn,
            "https://wiki.corp.example.net/page",
            "2024-06-15T00:00:00Z",
        );
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(
            rows.iter().find(|r| r.id == a).unwrap().category_id,
            Some(long_cat.id)
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn regex_rule_loses_to_any_non_regex_match() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let regex_cat = store.create_category("Regex", "#111111", None).unwrap();
        let substring_cat = store.create_category("Substring", "#222222", None).unwrap();
        store
            .create_rule("exe", r"^my.*\.exe$", "regex", regex_cat.id)
            .unwrap();
        store
            .create_rule("exe", "myeditor", "substring", substring_cat.id)
            .unwrap();

        let a = insert_session(&conn, "myeditor.exe", "2024-06-15T00:00:00Z");
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(
            rows.iter().find(|r| r.id == a).unwrap().category_id,
            Some(substring_cat.id)
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn url_rule_beats_domain_rule_for_the_same_session_and_lets_two_documents_diverge() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let work = store.create_category("Work", "#111111", None).unwrap();
        let english = store.create_category("English study", "#222222", None).unwrap();
        store
            .create_rule("domain", "docs.example.org", "exact", work.id)
            .unwrap();
        store
            .create_rule(
                "url",
                "docs.example.org/document/d/ABC",
                "prefix",
                english.id,
            )
            .unwrap();

        let english_doc = insert_browser_session(
            &conn,
            "https://docs.example.org/document/d/ABC/edit",
            "2024-06-15T00:00:00Z",
        );
        let other_doc = insert_browser_session(
            &conn,
            "https://docs.example.org/document/d/XYZ/edit",
            "2024-06-15T00:01:00Z",
        );

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(english_doc), Some(english.id));
        assert_eq!(
            category_of(other_doc),
            Some(work.id),
            "a document not covered by the url rule should fall back to the domain rule"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn url_rules_never_match_sessions_without_a_url_key() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let cat = store.create_category("Docs", "#111111", None).unwrap();
        store
            .create_rule("url", "myeditor.exe", "substring", cat.id)
            .unwrap();

        let a = insert_session(&conn, "myeditor.exe", "2024-06-15T00:00:00Z");
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(rows.iter().find(|r| r.id == a).unwrap().category_id, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn deleting_a_parent_takes_its_whole_subtree_with_it() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let work = store.create_category("Work", "#111111", None).unwrap();
        let reading = store
            .create_category("Reading", "#222222", Some(work.id))
            .unwrap();
        let papers = store
            .create_category("Papers", "#333333", Some(reading.id))
            .unwrap();
        let personal = store.create_category("Personal", "#444444", None).unwrap();
        store
            .create_rule("exe", "papers.exe", "exact", papers.id)
            .unwrap();
        let pinned = insert_session(&conn, "anything.exe", "2024-06-15T00:00:00Z");
        store.set_row_category(pinned, Some(papers.id)).unwrap();

        store.delete_category(work.id).unwrap();

        let left: Vec<i64> = store
            .list_categories()
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(left, vec![personal.id]);
        assert!(store.list_rules().unwrap().is_empty());
        let still_pinned: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE category_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(still_pinned, 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_category_cannot_be_moved_under_one_of_its_own_descendants() {
        let (store, path) = temp_store();
        let work = store.create_category("Work", "#111111", None).unwrap();
        let reading = store
            .create_category("Reading", "#222222", Some(work.id))
            .unwrap();
        let papers = store
            .create_category("Papers", "#333333", Some(reading.id))
            .unwrap();

        assert!(store
            .update_category(work.id, "Work", "#111111", Some(papers.id))
            .is_err());
        assert!(store
            .update_category(work.id, "Work", "#111111", Some(work.id))
            .is_err());
        assert!(store
            .update_category(work.id, "Work", "#111111", Some(9999))
            .is_err());

        store
            .update_category(papers.id, "Papers", "#333333", Some(work.id))
            .expect("moving a category up to another branch is fine");
        let parent_of_papers = store
            .list_categories()
            .unwrap()
            .into_iter()
            .find(|c| c.id == papers.id)
            .unwrap()
            .parent_id;
        assert_eq!(parent_of_papers, Some(work.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn create_rule_rejects_invalid_key_kind() {
        let (store, path) = temp_store();
        let cat = store.create_category("Cat", "#111111", None).unwrap();
        assert!(store.create_rule("bogus", "x", "exact", cat.id).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
