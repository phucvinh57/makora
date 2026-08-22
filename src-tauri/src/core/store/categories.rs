use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::sqlite::SharedConnection;
use crate::core::category::{validate_pattern, CategoryTree, KeyKind, MatchKind};
use crate::core::store::resolve;

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent_id: Option<i64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategorySuggestion {
    pub id: i64,
    pub key_kind: String,
    pub key: String,
    pub category_id: i64,
    pub category_name: String,
    pub confidence: f32,
    pub seconds: f64,
    pub model_version: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedTaxonomy {
    pub categories: Vec<SeedCategory>,
    pub rules: Vec<SeedRule>,
    #[serde(default)]
    pub multi_purpose: Vec<SeedMultiPurposeKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedCategory {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
    #[serde(default)]
    pub children: Vec<SeedCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedMultiPurposeKey {
    pub key_kind: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedRule {
    pub key_kind: String,
    pub match_kind: String,
    pub pattern: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryRule {
    pub id: i64,
    pub key_kind: String,
    pub pattern: String,
    pub match_kind: String,
    pub category_id: i64,
    pub inactive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultiPurposeKey {
    pub key_kind: String,
    pub key: String,
    pub source: String,
    pub enabled: bool,
    pub top_share: Option<f32>,
    pub detected_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RecategorizeOutcome {
    pub manual_categories_preserved: i64,
    pub ai_predictions_cleared: i64,
    pub rows_resolved: i64,
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
pub(super) fn ensure_exists(conn: &rusqlite::Connection, category_id: i64) -> Result<()> {
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

pub(super) fn bump_classifier_input_revision(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value)
         VALUES ('ai_categorization_input_revision', '0')",
        [],
    )?;
    conn.execute(
        "UPDATE settings
         SET value = CAST(value AS INTEGER) + 1
         WHERE key = 'ai_categorization_input_revision'",
        [],
    )?;
    Ok(())
}

pub(super) fn load_category_tree(conn: &rusqlite::Connection) -> Result<CategoryTree> {
    let mut stmt = conn.prepare("SELECT id, parent_id FROM categories")?;
    let categories = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(CategoryTree::new(categories))
}

fn insert_seed_category(
    tx: &rusqlite::Transaction<'_>,
    category: &SeedCategory,
    parent_id: Option<i64>,
    category_ids: &mut std::collections::HashMap<String, i64>,
) -> Result<()> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM categories WHERE name = ?1 AND parent_id IS ?2",
            params![category.name, parent_id],
            |row| row.get(0),
        )
        .optional()?;
    let id = if let Some(id) = existing {
        id
    } else {
        tx.execute(
            "INSERT INTO categories (name, color, description, parent_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                category.name,
                category.color,
                category.description,
                parent_id
            ],
        )?;
        tx.last_insert_rowid()
    };
    if category_ids.insert(category.name.clone(), id).is_some() {
        bail!("seed category name is ambiguous: {}", category.name);
    }
    for child in &category.children {
        insert_seed_category(tx, child, Some(id), category_ids)?;
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
            "SELECT id, name, color, parent_id, description FROM categories
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    parent_id: row.get(3)?,
                    description: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    #[allow(dead_code)]
    pub fn create_category(
        &self,
        name: &str,
        color: &str,
        parent_id: Option<i64>,
    ) -> Result<Category> {
        self.create_category_with_description(name, color, None, parent_id)
    }

    pub fn create_category_with_description(
        &self,
        name: &str,
        color: &str,
        description: Option<&str>,
        parent_id: Option<i64>,
    ) -> Result<Category> {
        let conn = self.conn.lock().unwrap();
        if let Some(parent_id) = parent_id {
            ensure_exists(&conn, parent_id)?;
        }
        conn.execute(
            "INSERT INTO categories (name, color, parent_id, description)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, color, parent_id, description],
        )
        .map_err(|err| name_clash(err, name, parent_id))?;
        bump_classifier_input_revision(&conn)?;
        Ok(Category {
            id: conn.last_insert_rowid(),
            name: name.to_string(),
            color: color.to_string(),
            parent_id,
            description: description.map(str::to_string),
        })
    }

    #[allow(dead_code)]
    pub fn update_category(
        &self,
        id: i64,
        name: &str,
        color: &str,
        parent_id: Option<i64>,
    ) -> Result<()> {
        self.update_category_with_description(id, name, color, None, parent_id)
    }

    pub fn update_category_with_description(
        &self,
        id: i64,
        name: &str,
        color: &str,
        description: Option<&str>,
        parent_id: Option<i64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if let Some(parent_id) = parent_id {
            ensure_exists(&conn, parent_id)?;
            // Re-parenting a category under itself or under one of its own descendants would
            // cut that whole branch loose from every root.
            let tree = load_category_tree(&conn)?;
            if tree.descendants(id).contains(&parent_id) {
                bail!("a category cannot be moved inside itself");
            }
        }
        conn.execute(
            "UPDATE categories
             SET name = ?1, color = ?2, description = ?3, parent_id = ?4
             WHERE id = ?5",
            params![name, color, description, parent_id, id],
        )
        .map_err(|err| name_clash(err, name, parent_id))?;
        bump_classifier_input_revision(&conn)?;
        Ok(())
    }

    pub fn list_suggestions(&self) -> Result<Vec<CategorySuggestion>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.key_kind, s.key, s.category_id, c.name,
                    s.confidence, s.seconds, s.model_version, s.created_at
             FROM category_suggestions s
             JOIN categories c ON c.id = s.category_id
             WHERE s.status = 'pending'
             ORDER BY s.seconds * s.confidence DESC, s.id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CategorySuggestion {
                    id: row.get(0)?,
                    key_kind: row.get(1)?,
                    key: row.get(2)?,
                    category_id: row.get(3)?,
                    category_name: row.get(4)?,
                    confidence: row.get(5)?,
                    seconds: row.get(6)?,
                    model_version: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn list_multi_purpose_keys(&self) -> Result<Vec<MultiPurposeKey>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key_kind, key, source, enabled, top_share, detected_at
             FROM multi_purpose_keys
             ORDER BY key_kind, key",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MultiPurposeKey {
                    key_kind: row.get(0)?,
                    key: row.get(1)?,
                    source: row.get(2)?,
                    enabled: row.get::<_, i64>(3)? != 0,
                    top_share: row.get(4)?,
                    detected_at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_activity_clusters(&self) -> Result<Vec<super::clusters::ActivityCluster>> {
        super::clusters::list_activity_clusters(&self.conn)
    }

    pub fn assign_activity_cluster(
        &self,
        cluster_id: i64,
        category_id: i64,
        member_ids: &[i64],
    ) -> Result<super::clusters::ClusterAssignment> {
        super::clusters::assign_activity_cluster(&self.conn, cluster_id, category_id, member_ids)
    }

    pub fn set_multi_purpose_key(&self, key_kind: &str, key: &str, enabled: bool) -> Result<()> {
        let key_kind = KeyKind::parse(key_kind)
            .filter(|kind| matches!(kind, KeyKind::Exe | KeyKind::Domain))
            .ok_or_else(|| anyhow!("multi-purpose keys must be exe or domain"))?;
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() {
            bail!("multi-purpose key must not be empty");
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO multi_purpose_keys (key_kind, key, source, enabled)
             VALUES (?1, ?2, 'user', ?3)
             ON CONFLICT(key_kind, key) DO UPDATE SET
                source = 'user', enabled = excluded.enabled",
            params![key_kind.as_str(), key, enabled as i64],
        )?;
        tx.execute(
            "UPDATE sessions
             SET ml_category_id = NULL, ml_confidence = NULL, ml_model_version = NULL
             WHERE category_key_kind = ?1 AND category_key = ?2",
            params![key_kind.as_str(), key],
        )?;
        if enabled {
            tx.execute(
                "DELETE FROM category_suggestions
                 WHERE status = 'pending' AND key_kind = ?1 AND key = ?2",
                params![key_kind.as_str(), key],
            )?;
        }
        bump_classifier_input_revision(&tx)?;
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn accept_suggestions(&self, ids: &[i64], override_category: Option<i64>) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for id in ids {
            let suggestion: Option<(String, String, i64)> = tx
                .query_row(
                    "SELECT key_kind, key, category_id
                     FROM category_suggestions
                     WHERE id = ?1 AND status = 'pending'",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((key_kind, key, suggested_category)) = suggestion else {
                bail!("category suggestion {id} is no longer pending");
            };
            let category_id = override_category.unwrap_or(suggested_category);
            ensure_exists(&tx, category_id)?;
            // Accepting is idempotent: an exact rule for the same key is retargeted rather than
            // rejected, so a suggestion that reappears for an already-ruled key stays actionable.
            tx.execute(
                "INSERT INTO category_rules
                    (key_kind, pattern, match_kind, category_id)
                 VALUES (?1, ?2, 'exact', ?3)
                 ON CONFLICT(key_kind, match_kind, pattern) DO UPDATE SET
                    category_id = excluded.category_id",
                params![key_kind, key, category_id],
            )?;
            tx.execute(
                "DELETE FROM category_suggestions WHERE id = ?1",
                params![id],
            )?;
        }
        bump_classifier_input_revision(&tx)?;
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn dismiss_suggestions(&self, ids: &[i64]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE category_suggestions SET status = 'dismissed'
                 WHERE id = ?1 AND status = 'pending'",
                params![id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn seed_taxonomy_preview(seed_json: &str) -> Result<SeedTaxonomy> {
        serde_json::from_str(seed_json).context("parsing bundled seed taxonomy")
    }

    pub fn apply_seed_taxonomy(&self, seed_json: &str) -> Result<()> {
        let seed = Self::seed_taxonomy_preview(seed_json)?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut category_ids = std::collections::HashMap::new();
        for category in &seed.categories {
            insert_seed_category(&tx, category, None, &mut category_ids)?;
        }
        for rule in &seed.rules {
            let key_kind = KeyKind::parse(&rule.key_kind)
                .ok_or_else(|| anyhow!("invalid seed rule key kind: {}", rule.key_kind))?;
            let match_kind = MatchKind::parse(&rule.match_kind)
                .ok_or_else(|| anyhow!("invalid seed rule match kind: {}", rule.match_kind))?;
            validate_pattern(match_kind, &rule.pattern)?;
            let category_id = category_ids
                .get(rule.category.as_str())
                .copied()
                .ok_or_else(|| {
                    anyhow!("seed rule references unknown category {}", rule.category)
                })?;
            tx.execute(
                "INSERT OR IGNORE INTO category_rules
                    (key_kind, pattern, match_kind, category_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    key_kind.as_str(),
                    rule.pattern.trim(),
                    match_kind.as_str(),
                    category_id
                ],
            )?;
        }
        for multi_purpose in &seed.multi_purpose {
            let key_kind = KeyKind::parse(&multi_purpose.key_kind)
                .filter(|kind| matches!(kind, KeyKind::Exe | KeyKind::Domain))
                .ok_or_else(|| {
                    anyhow!("invalid multi-purpose key kind: {}", multi_purpose.key_kind)
                })?;
            let key = multi_purpose.key.trim().to_ascii_lowercase();
            if key.is_empty() {
                bail!("seed multi-purpose key must not be empty");
            }
            tx.execute(
                "INSERT OR IGNORE INTO multi_purpose_keys (key_kind, key, source, enabled)
                 VALUES (?1, ?2, 'seed', 1)",
                params![key_kind.as_str(), key],
            )?;
        }
        bump_classifier_input_revision(&tx)?;
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its rules and pins with it.
    pub fn delete_category(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        ensure_exists(&tx, id)?;
        // Deepest first: a parent still referenced by one of its children cannot be deleted.
        let doomed = load_category_tree(&tx)?.deletion_order(id);
        for category_id in &doomed {
            tx.execute(
                "DELETE FROM category_rules WHERE category_id = ?1",
                params![category_id],
            )?;
            tx.execute(
                "UPDATE sessions SET category_id = NULL WHERE category_id = ?1",
                params![category_id],
            )?;
            // Resolved labels reference categories just as hard as pins do — the delete
            // fails outright on the foreign key without this.
            tx.execute(
                "UPDATE sessions SET rule_category_id = NULL WHERE rule_category_id = ?1",
                params![category_id],
            )?;
            tx.execute(
                "UPDATE sessions SET ml_category_id = NULL, ml_confidence = NULL, ml_model_version = NULL WHERE ml_category_id = ?1",
                params![category_id],
            )?;
            tx.execute(
                "DELETE FROM work_session_categories WHERE category_id = ?1",
                params![category_id],
            )?;
            tx.execute("DELETE FROM categories WHERE id = ?1", params![category_id])?;
        }
        bump_classifier_input_revision(&tx)?;
        // The branch's rules went with it, so rows they used to claim may now match a
        // different rule or become uncategorized.
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_rules(&self) -> Result<Vec<CategoryRule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.key_kind, r.pattern, r.match_kind, r.category_id,
                    CASE WHEN r.match_kind = 'exact' AND r.key_kind IN ('exe', 'domain')
                              AND m.key IS NOT NULL THEN 1 ELSE 0 END
             FROM category_rules r
             LEFT JOIN multi_purpose_keys m
               ON m.key_kind = r.key_kind AND m.key = r.pattern AND m.enabled = 1
             ORDER BY r.key_kind, r.pattern",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CategoryRule {
                    id: row.get(0)?,
                    key_kind: row.get(1)?,
                    pattern: row.get(2)?,
                    match_kind: row.get(3)?,
                    category_id: row.get(4)?,
                    inactive: row.get::<_, i64>(5)? != 0,
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
        validate_pattern(kind, pattern)?;
        let mut conn = self.conn.lock().unwrap();
        // The insert and the replay are one unit: a rule that saved while its replay failed
        // would leave history disagreeing with the rule list until something else moved.
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id) VALUES (?1, ?2, ?3, ?4)",
            params![key_kind, pattern, kind.as_str(), category_id],
        )?;
        let id = tx.last_insert_rowid();
        bump_classifier_input_revision(&tx)?;
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(CategoryRule {
            id,
            key_kind: key_kind.to_string(),
            pattern: pattern.to_string(),
            match_kind: kind.as_str().to_string(),
            category_id,
            inactive: false,
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM category_rules WHERE id = ?1", params![id])?;
        bump_classifier_input_revision(&tx)?;
        resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_row_category(&self, session_id: i64, category_id: Option<i64>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions
             SET category_id = ?1,
                 ml_category_id = NULL,
                 ml_confidence = NULL,
                 ml_model_version = NULL
             WHERE id = ?2",
            params![category_id, session_id],
        )?;
        bump_classifier_input_revision(&self.conn.lock().unwrap())?;
        Ok(())
    }

    pub fn set_row_categories(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for session_id in session_ids {
            tx.execute(
                "UPDATE sessions
                 SET category_id = ?1,
                     ml_category_id = NULL,
                     ml_confidence = NULL,
                     ml_model_version = NULL
                 WHERE id = ?2",
                params![category_id, session_id],
            )?;
        }
        bump_classifier_input_revision(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Replays automatic categorization without disturbing categories chosen by hand.
    /// Derived AI values are cleared, then every rule label is recomputed; manual pins keep
    /// taking precedence over the refreshed automatic result.
    pub fn recategorize_all(&self) -> Result<RecategorizeOutcome> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let manual_categories_preserved: i64 = tx.query_row(
            "SELECT COUNT(*) FROM sessions WHERE category_id IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        let ai_predictions_cleared = tx.execute(
            "UPDATE sessions SET ml_category_id = NULL, ml_confidence = NULL, ml_model_version = NULL WHERE ml_category_id IS NOT NULL",
            [],
        )?;
        bump_classifier_input_revision(&tx)?;
        let rows_resolved = resolve::resolve_all(&tx)?;
        tx.commit()?;
        Ok(RecategorizeOutcome {
            manual_categories_preserved,
            ai_predictions_cleared: ai_predictions_cleared as i64,
            rows_resolved: rows_resolved as i64,
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
            "a fresh database starts with no taxonomy"
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

    /// Ten minutes, used where a test does not care about the row's exact duration.
    const LONG_ENOUGH_SECONDS: i64 = 600;

    fn ends_after(start_time: &str, seconds: i64) -> String {
        let start: chrono::DateTime<chrono::Utc> =
            start_time.parse().expect("valid RFC 3339 start");
        (start + chrono::Duration::seconds(seconds)).to_rfc3339()
    }

    fn insert_session(conn: &SharedConnection, exe: &str, start_time: &str) -> i64 {
        insert_session_lasting(conn, exe, start_time, LONG_ENOUGH_SECONDS)
    }

    /// Rows go in *closed and resolved*, the way the tracker writes them. Resolution is
    /// materialized now, so a raw insert would sit at NULL no matter what the rules say —
    /// and `create_rule` replays history at the moment it is called, which in most of these
    /// tests is before the sessions even exist.
    fn insert_session_lasting(
        conn: &SharedConnection,
        exe: &str,
        start_time: &str,
        seconds: i64,
    ) -> i64 {
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time, end_time)
             VALUES (?1, ?1, 'title', ?1, 'exe', ?2, ?3)",
            params![exe, start_time, ends_after(start_time, seconds)],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        resolve::resolve_session(&conn, id).unwrap();
        id
    }

    /// A row the tracker has opened but not yet closed. Rules apply to open and closed rows alike.
    fn insert_open_session(conn: &SharedConnection, exe: &str, start_time: &str) -> i64 {
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time)
             VALUES (?1, ?1, 'title', ?1, 'exe', ?2)",
            params![exe, start_time],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        resolve::resolve_session(&conn, id).unwrap();
        id
    }

    fn insert_browser_session(conn: &SharedConnection, url: &str, start_time: &str) -> i64 {
        let (key_kind, key) = crate::core::category::category_key("chrome.exe", Some(url));
        let url_key = crate::core::category::url_key(url);
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key, start_time, end_time)
             VALUES ('chrome.exe', 'chrome.exe', 'title', ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                url,
                key,
                key_kind.as_str(),
                url_key,
                start_time,
                ends_after(start_time, LONG_ENOUGH_SECONDS)
            ],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        resolve::resolve_session(&conn, id).unwrap();
        id
    }

    #[test]
    fn recategorize_all_preserves_pins_while_replaying_rules() {
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
        conn.lock()
            .unwrap()
            .execute("UPDATE sessions SET rule_category_id = NULL", [])
            .unwrap();

        let outcome = store.recategorize_all().unwrap();
        assert_eq!(outcome.manual_categories_preserved, 2);
        assert_eq!(outcome.rows_resolved, 3);

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(by_rule), Some(ruled.id));
        assert_eq!(category_of(by_pin), Some(manual.id));
        assert_eq!(
            category_of(pinned_over_rule),
            Some(manual.id),
            "a manual category must continue to override the refreshed rule label"
        );
        let refreshed_rule: Option<i64> = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE id = ?1",
                params![pinned_over_rule],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(refreshed_rule, Some(ruled.id));

        assert!(store
            .list_rules()
            .unwrap()
            .iter()
            .any(|r| r.pattern == "ruled.exe"));

        assert_eq!(
            store
                .recategorize_all()
                .unwrap()
                .manual_categories_preserved,
            2
        );

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
    fn changing_or_clearing_a_manual_label_removes_the_rows_old_ai_prediction() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let manual = store.create_category("Manual", "#111111", None).unwrap();
        let predicted = store.create_category("Predicted", "#222222", None).unwrap();
        let row = insert_session(&conn, "editor.exe", "2024-06-15T00:00:00Z");

        conn.lock()
            .unwrap()
            .execute(
                "UPDATE sessions
                 SET ml_category_id = ?1, ml_confidence = 0.9, ml_model_version = 7
                 WHERE id = ?2",
                params![predicted.id, row],
            )
            .unwrap();
        store.set_row_category(row, Some(manual.id)).unwrap();
        store.set_row_category(row, None).unwrap();

        let ai_values: (Option<i64>, Option<f32>, Option<i64>) = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT ml_category_id, ml_confidence, ml_model_version
                 FROM sessions WHERE id = ?1",
                params![row],
                |result| Ok((result.get(0)?, result.get(1)?, result.get(2)?)),
            )
            .unwrap();
        assert_eq!(ai_values, (None, None, None));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recategorize_all_preserves_bulk_pins_and_clears_ai_predictions() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let pin_cat = store.create_category("PinCat", "#333333", None).unwrap();
        let predicted = store.create_category("Predicted", "#444444", None).unwrap();
        let a = insert_session(&conn, "pinned.exe", "2024-06-15T00:00:00Z");
        let b = insert_browser_session(&conn, "https://chatgpt.com/c/AAA", "2024-06-15T01:00:00Z");
        let c = insert_session(&conn, "predicted.exe", "2024-06-15T02:00:00Z");
        store.set_row_categories(&[a, b], Some(pin_cat.id)).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE sessions
                 SET ml_category_id = ?1, ml_confidence = 0.9, ml_model_version = 7
                 WHERE id = ?2",
                params![predicted.id, c],
            )
            .unwrap();

        let outcome = store.recategorize_all().unwrap();
        assert_eq!(outcome.manual_categories_preserved, 2);
        assert_eq!(outcome.ai_predictions_cleared, 1);

        let still_pinned: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE category_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(still_pinned, 2);

        let remaining_predictions: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE ml_category_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining_predictions, 0);

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
    fn glob_rule_matches_a_wildcard_subdomain_but_stays_anchored() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let cat = store.create_category("Work", "#222222", None).unwrap();
        store
            .create_rule("domain", "*.corp.example.com", "glob", cat.id)
            .unwrap();

        let a = insert_browser_session(
            &conn,
            "https://wiki.corp.example.com/",
            "2024-06-15T00:00:00Z",
        );
        let b = insert_browser_session(
            &conn,
            "https://evil-corp.example.com.attacker.net/",
            "2024-06-15T01:00:00Z",
        );

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(a), Some(cat.id));
        assert_eq!(
            category_of(b),
            None,
            "GLOB is anchored at both ends, so a lookalike suffix should not match"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn glob_url_rule_matches_a_wildcard_path_segment() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let dev = store
            .create_category("Development", "#111111", None)
            .unwrap();
        let review = store
            .create_category("Code Review", "#222222", None)
            .unwrap();
        store
            .create_rule("domain", "github.com", "exact", dev.id)
            .unwrap();
        store
            .create_rule("url", "github.com/*/pulls", "glob", review.id)
            .unwrap();

        let pulls = insert_browser_session(
            &conn,
            "https://github.com/anthropics/claude-code/pulls",
            "2024-06-15T00:00:00Z",
        );
        let issues = insert_browser_session(
            &conn,
            "https://github.com/anthropics/claude-code/issues",
            "2024-06-15T01:00:00Z",
        );

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(
            category_of(pulls),
            Some(review.id),
            "a url rule outranks the domain rule for the same page"
        );
        assert_eq!(
            category_of(issues),
            Some(dev.id),
            "a page the url rule misses still falls back to the domain rule"
        );

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
    fn glob_rule_loses_to_any_non_glob_match() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let glob_cat = store.create_category("Glob", "#111111", None).unwrap();
        let substring_cat = store.create_category("Substring", "#222222", None).unwrap();
        store
            .create_rule("exe", "my*.exe", "glob", glob_cat.id)
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
        let english = store
            .create_category("English study", "#222222", None)
            .unwrap();
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
        // A row the doomed rule had already claimed — its resolved label references the
        // category just as hard as the pin does.
        insert_session(&conn, "papers.exe", "2024-06-15T01:00:00Z");

        store.delete_category(work.id).unwrap();

        let left: Vec<i64> = store
            .list_categories()
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(left, vec![personal.id]);
        assert!(store.list_rules().unwrap().is_empty());
        let still_referencing: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sessions
                 WHERE category_id IS NOT NULL OR rule_category_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            still_referencing, 0,
            "both the pin and the resolved label must let go of a deleted category"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_category_named_distraction_has_full_category_crud() {
        let (store, path) = temp_store();
        let work = store.create_category("Work", "#111111", None).unwrap();
        let distraction = store
            .create_category("Distraction", "#a1a1aa", None)
            .unwrap();

        store
            .update_category(distraction.id, "Noise", "#ff0000", Some(work.id))
            .unwrap();
        let child = store
            .create_category("Nested", "#222222", Some(distraction.id))
            .unwrap();
        assert_eq!(
            store
                .list_categories()
                .unwrap()
                .into_iter()
                .find(|category| category.id == distraction.id)
                .unwrap()
                .parent_id,
            Some(work.id)
        );
        store.delete_category(distraction.id).unwrap();
        let remaining = store.list_categories().unwrap();
        assert!(remaining
            .iter()
            .all(|category| category.id != distraction.id));
        assert!(remaining.iter().all(|category| category.id != child.id));
        assert!(remaining.iter().any(|category| category.id == work.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn activity_duration_never_changes_rule_resolution() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let work = store.create_category("Work", "#111111", None).unwrap();
        store
            .create_rule("exe", "code.exe", "exact", work.id)
            .unwrap();

        let quick_tool_switch =
            insert_session_lasting(&conn, "code.exe", "2024-06-15T00:00:00Z", 4);
        let short_unmatched =
            insert_session_lasting(&conn, "random.exe", "2024-06-15T01:00:00Z", 3);
        let long_unmatched = insert_session(&conn, "random.exe", "2024-06-15T02:00:00Z");
        let open_unmatched = insert_open_session(&conn, "blip.exe", "2024-06-15T03:00:00Z");

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(quick_tool_switch), Some(work.id));
        assert_eq!(category_of(short_unmatched), None);
        assert_eq!(category_of(long_unmatched), None);
        assert_eq!(category_of(open_unmatched), None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clearing_a_manual_pin_leaves_an_unmatched_activity_uncategorized() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let work = store.create_category("Work", "#111111", None).unwrap();
        let row = insert_session_lasting(&conn, "random.exe", "2024-06-15T00:00:00Z", 3);
        store.set_row_category(row, Some(work.id)).unwrap();

        let category_of = |rows: &[crate::core::store::queries::SessionRow]| {
            rows.iter().find(|r| r.id == row).unwrap().category_id
        };
        assert_eq!(
            category_of(&reader.list_sessions(10, 0, &filter).unwrap()),
            Some(work.id)
        );

        store.set_row_category(row, None).unwrap();
        assert_eq!(
            category_of(&reader.list_sessions(10, 0, &filter).unwrap()),
            None
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn adding_or_removing_a_rule_relabels_existing_history_immediately() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let gaming = store.create_category("Gaming", "#111111", None).unwrap();
        let played = insert_session(&conn, "steam.exe", "2024-06-15T00:00:00Z");

        let category_of = |rows: &[crate::core::store::queries::SessionRow]| {
            rows.iter().find(|r| r.id == played).unwrap().category_id
        };
        assert_eq!(
            category_of(&reader.list_sessions(10, 0, &filter).unwrap()),
            None
        );

        let rule = store
            .create_rule("exe", "steam.exe", "exact", gaming.id)
            .unwrap();
        assert_eq!(
            category_of(&reader.list_sessions(10, 0, &filter).unwrap()),
            Some(gaming.id),
            "a rule written today has to reach back over history it was never applied to"
        );

        store.delete_rule(rule.id).unwrap();
        assert_eq!(
            category_of(&reader.list_sessions(10, 0, &filter).unwrap()),
            None
        );

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

    #[test]
    fn create_rule_rejects_a_malformed_glob() {
        let (store, path) = temp_store();
        let category = store.create_category("Work", "#111111", None).unwrap();
        assert!(store
            .create_rule("exe", "unterminated[", "glob", category.id)
            .is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn malformed_legacy_glob_stays_visible_but_inactive() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let category = store.create_category("Work", "#111111", None).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                 VALUES ('exe', 'unterminated[', 'glob', ?1)",
                params![category.id],
            )
            .unwrap();

        let session = insert_session(&conn, "unterminated.exe", "2024-06-15T00:00:00Z");
        let row = crate::core::store::queries::HistoryReader::new(conn)
            .list_sessions(
                10,
                0,
                &crate::core::store::queries::SessionFilter::default(),
            )
            .unwrap()
            .into_iter()
            .find(|row| row.id == session)
            .unwrap();
        assert_eq!(row.category_id, None);
        assert_eq!(store.list_rules().unwrap().len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn accepting_a_suggestion_creates_one_rule_and_resolves_history_once() {
        let (store, path) = temp_store();
        let development = store
            .create_category("Development", "#111111", None)
            .unwrap();
        let conn = store.conn.clone();
        let session = insert_domain_session(
            &conn,
            "github.com",
            "2024-06-15T00:00:00Z",
            "2024-06-15T00:10:00Z",
        );
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO category_suggestions
                    (id, key_kind, key, category_id, confidence, seconds, model_version, created_at)
                 VALUES (1, 'domain', 'github.com', ?1, 0.91, 600.0, 1, '2024-06-15T00:00:00Z')",
                params![development.id],
            )
            .unwrap();

        store.accept_suggestions(&[1], None).unwrap();
        assert!(store.list_suggestions().unwrap().is_empty());
        let rule = store
            .list_rules()
            .unwrap()
            .into_iter()
            .find(|rule| rule.pattern == "github.com")
            .expect("accepted suggestion should become a rule");
        assert_eq!(rule.category_id, development.id);
        let resolved: Option<i64> = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE id = ?1",
                params![session],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved, Some(development.id));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn accepting_a_suggestion_retargets_an_existing_exact_rule() {
        let (store, path) = temp_store();
        let social = store.create_category("Social", "#111111", None).unwrap();
        let general = store.create_category("General", "#222222", None).unwrap();
        store
            .create_rule("domain", "facebook.com", "exact", social.id)
            .unwrap();
        let conn = store.conn.clone();
        let session = insert_domain_session(
            &conn,
            "facebook.com",
            "2024-06-15T00:00:00Z",
            "2024-06-15T00:10:00Z",
        );
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO category_suggestions
                    (id, key_kind, key, category_id, confidence, seconds, model_version, created_at)
                 VALUES (1, 'domain', 'facebook.com', ?1, 0.77, 600.0, 1, '2024-06-15T00:00:00Z')",
                params![general.id],
            )
            .unwrap();

        store.accept_suggestions(&[1], None).unwrap();
        let rules = store
            .list_rules()
            .unwrap()
            .into_iter()
            .filter(|rule| rule.pattern == "facebook.com")
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].category_id, general.id);
        let resolved: Option<i64> = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE id = ?1",
                params![session],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved, Some(general.id));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dismissing_and_reapplying_seed_taxonomy_are_idempotent() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        conn.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO categories (id, name, color) VALUES (1, 'Existing', '#111111');
                 INSERT INTO category_suggestions
                    (id, key_kind, key, category_id, confidence, seconds, model_version, created_at)
                VALUES (1, 'exe', 'unknown.exe', 1, 0.8, 10.0, 1, '2024-06-15T00:00:00Z')",
            )
            .unwrap();
        store.dismiss_suggestions(&[1]).unwrap();
        assert!(store.list_suggestions().unwrap().is_empty());
        let seed = r##"{
            "categories": [{"name":"Seed","color":"#222222","description":"A seed"}],
            "rules": [{"key_kind":"domain","match_kind":"exact","pattern":"example.com","category":"Seed"}]
        }"##;
        store.apply_seed_taxonomy(seed).unwrap();
        store.apply_seed_taxonomy(seed).unwrap();
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM categories WHERE name = 'Seed'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM category_rules WHERE pattern = 'example.com'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn multi_purpose_toggle_replays_rules_and_marks_exact_rules_inactive() {
        let (store, path) = temp_store();
        let category = store.create_category("Media", "#111111", None).unwrap();
        store
            .create_rule("domain", "youtube.com", "exact", category.id)
            .unwrap();
        let session = insert_browser_session(
            &store.conn,
            "https://youtube.com/watch",
            "2024-06-15T00:00:00Z",
        );
        let rule_category: Option<i64> = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE id = ?1",
                params![session],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rule_category, Some(category.id));

        store
            .set_multi_purpose_key("domain", "youtube.com", true)
            .unwrap();
        let rule_category: Option<i64> = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE id = ?1",
                params![session],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rule_category, None);
        assert!(
            store
                .list_rules()
                .unwrap()
                .into_iter()
                .find(|rule| rule.pattern == "youtube.com")
                .unwrap()
                .inactive
        );

        store
            .set_multi_purpose_key("domain", "youtube.com", false)
            .unwrap();
        let rule_category: Option<i64> = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE id = ?1",
                params![session],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rule_category, Some(category.id));
        assert!(
            !store
                .list_rules()
                .unwrap()
                .into_iter()
                .find(|rule| rule.pattern == "youtube.com")
                .unwrap()
                .inactive
        );
        let veto: (String, i64) = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT source, enabled FROM multi_purpose_keys WHERE key = 'youtube.com'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(veto, ("user".into(), 0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn nested_seed_taxonomy_is_idempotent_and_targets_leaf_rules() {
        let (store, path) = temp_store();
        let seed = r##"{
            "categories": [{
                "name": "Learning", "color": "#111111", "description": "Root",
                "children": [{"name":"Courses","color":"#222222","description":"Leaf"}]
            }],
            "rules": [{"key_kind":"domain","match_kind":"exact","pattern":"courses.example","category":"Courses"}],
            "multi_purpose": [{"key_kind":"domain","key":"mixed.example"}]
        }"##;
        store.apply_seed_taxonomy(seed).unwrap();
        store.apply_seed_taxonomy(seed).unwrap();
        let categories = store.list_categories().unwrap();
        let parent = categories
            .iter()
            .find(|category| category.name == "Learning")
            .unwrap();
        let child = categories
            .iter()
            .find(|category| category.name == "Courses")
            .unwrap();
        assert_eq!(child.parent_id, Some(parent.id));
        assert_eq!(
            categories
                .iter()
                .filter(|category| category.name == "Courses")
                .count(),
            1
        );
        assert_eq!(store.list_rules().unwrap().len(), 1);
        let multi = store.list_multi_purpose_keys().unwrap();
        assert_eq!(
            multi
                .iter()
                .filter(|key| key.key == "mixed.example")
                .count(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    fn insert_domain_session(conn: &SharedConnection, domain: &str, start: &str, end: &str) -> i64 {
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions
                (app_name, exe_path, window_title, category_key, category_key_kind,
                 start_time, end_time)
            VALUES ('Chrome', 'chrome', ?1, ?2, 'domain', ?3, ?4)",
            params![domain, domain, start, end],
        )
        .unwrap();
        conn.last_insert_rowid()
    }
}
