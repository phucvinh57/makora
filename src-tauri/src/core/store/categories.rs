use anyhow::{anyhow, bail, Context, Result};
use rusqlite::params;
use serde::Serialize;

use super::sqlite::SharedConnection;
use crate::core::category::{KeyKind, MatchKind};

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryRule {
    pub id: i64,
    /// `"exe"`, `"domain"`, or `"url"` — see [`KeyKind`].
    pub key_kind: String,
    pub pattern: String,
    /// `"exact"`, `"prefix"`, `"substring"`, or `"regex"` — see [`MatchKind`]
    /// and `category::rule_join_sql`.
    pub match_kind: String,
    pub category_id: i64,
}

/// What picking a category for one session would actually affect, so the
/// frontend can show a confirmation ("this will recategorize N other
/// activities too") before committing — a single app can serve more than
/// one purpose, so applying a category retroactively to its whole history
/// deserves a heads-up rather than happening silently.
#[derive(Debug, Clone, Serialize)]
pub struct CategorizePreview {
    pub key_kind: String,
    pub key: String,
    /// Total sessions sharing this key, including the one that was clicked.
    pub affected_count: i64,
}

/// What a [`CategoryStore::recategorize_all`] pass erased, so the UI can say
/// how much manual categorization was actually thrown away rather than just
/// "done".
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecategorizeOutcome {
    /// Per-session pins cleared (`sessions.category_id`).
    pub pins_cleared: i64,
    /// Key-wide overrides deleted (`category_overrides`).
    pub overrides_cleared: i64,
    /// Rules deleted because they only existed to mirror one of those
    /// overrides on the Categories page (see [`CategoryStore::set_override`]).
    pub rules_cleared: i64,
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

    /// Ordered by name — categories no longer have a user-managed
    /// precedence, since rule conflicts resolve by specificity instead (see
    /// `category::rule_join_sql`).
    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, color FROM categories ORDER BY name COLLATE NOCASE, id")?;
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
        conn.execute(
            "INSERT INTO categories (name, color) VALUES (?1, ?2)",
            params![name, color],
        )?;
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
        tx.execute(
            "DELETE FROM category_rules WHERE category_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM category_overrides WHERE category_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM work_session_categories WHERE category_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
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

    /// `key_kind` is `"exe"`, `"domain"`, or `"url"` (see [`KeyKind`]);
    /// `match_kind` is `"exact"`, `"prefix"`, `"substring"`, or `"regex"`
    /// (see [`MatchKind`]); a `"regex"` pattern is compiled up front so an
    /// invalid one is rejected here rather than silently never matching at
    /// query time.
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

    /// Looks up how many sessions share `session_id`'s categorization key —
    /// i.e. how many rows [`Self::set_override`] would actually recategorize
    /// if called with this session. Read-only; doesn't write anything.
    pub fn preview_category_change(&self, session_id: i64) -> Result<CategorizePreview> {
        let conn = self.conn.lock().unwrap();
        let (key_kind, key): (String, String) = conn.query_row(
            "SELECT category_key_kind, category_key FROM sessions WHERE id = ?1",
            params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let affected_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE category_key_kind = ?1 AND category_key = ?2",
            params![key_kind, key],
            |row| row.get(0),
        )?;
        Ok(CategorizePreview {
            key_kind,
            key,
            affected_count,
        })
    }

    /// Assigns `category_id` to a single session only — a per-session pin
    /// that's resolved before any key-wide override or rule (see
    /// `HistoryReader::list_sessions`). `None` clears the pin, letting the
    /// key-wide override / rules / "Uncategorized" resolve it instead.
    pub fn set_row_category(&self, session_id: i64, category_id: Option<i64>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET category_id = ?1 WHERE id = ?2",
            params![category_id, session_id],
        )?;
        Ok(())
    }

    /// Manually assigns `category_id` to every past and future session that
    /// shares this session's categorization key (e.g. all `chrome.exe` rows,
    /// or all `github.com` tab rows) — not just this one row. That mirrors
    /// how the built-in rules work and is what "always put GitHub under
    /// Development" actually expects. Also clears any per-session pins on
    /// that key (see [`Self::set_row_category`]) so they don't shadow this
    /// wider choice, and upserts a matching `category_rules` entry so the
    /// app/site shows up in — and can be managed from — the category's rule
    /// list on the Categories page, not just as an invisible override.
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
        conn.execute(
            "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id) VALUES (?1, ?2, 'exact', ?3)
             ON CONFLICT(key_kind, match_kind, pattern) DO UPDATE SET category_id = excluded.category_id",
            params![key_kind, key, category_id],
        )?;
        conn.execute(
            "UPDATE sessions SET category_id = NULL WHERE category_key_kind = ?1 AND category_key = ?2",
            params![key_kind, key],
        )?;
        Ok(())
    }

    /// Pins `category_id` on exactly the given sessions and nothing else —
    /// the bulk form of [`Self::set_row_category`], and what the history
    /// page's multi-select bar does by default. Deliberately *not*
    /// [`Self::set_overrides`]: selecting a filtered set of rows (say, every
    /// `chatgpt.com` row inside one focus session) means "these rows", so
    /// widening the choice to the whole app/site is an explicit opt-in
    /// there. Written in one transaction so a large selection can't land
    /// half-applied.
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

    /// Same as [`Self::set_override`] but for many sessions at once — the
    /// opt-in "…and everything else from these apps/sites" form of
    /// [`Self::set_row_categories`], so it carries the same caveat: every
    /// past *and future* session sharing a selected row's key gets this
    /// category, and per-session pins on those keys are cleared. Sessions
    /// are looked up and written inside a single transaction so a large
    /// selection doesn't leave the database half-updated if one id is bad.
    /// Also upserts a `category_rules` entry per distinct key, same as
    /// [`Self::set_override`].
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
            tx.execute(
                "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id) VALUES (?1, ?2, 'exact', ?3)
                 ON CONFLICT(key_kind, match_kind, pattern) DO UPDATE SET category_id = excluded.category_id",
                params![key_kind, key, category_id],
            )?;
            tx.execute(
                "UPDATE sessions SET category_id = NULL WHERE category_key_kind = ?1 AND category_key = ?2",
                params![key_kind, key],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Throws away *every* stored categorization decision in the database —
    /// all per-session pins, all key-wide overrides, and the exact rules
    /// those overrides minted — so history is left resolving purely from the
    /// rules the user actually keeps on the Categories page (or
    /// "Uncategorized" where none match). This is the whole of what the
    /// history page's "Recategorize" button does: no date range, no
    /// candidate-key list, no "protect manual" escape hatch, because a
    /// from-scratch pass that skipped already-categorized activities would
    /// leave exactly the rows the user is trying to fix untouched.
    ///
    /// The mirror rules go with the overrides deliberately. `set_override`
    /// writes both halves at once, and the rule half is only there so a
    /// clicked-on app/site is visible and manageable on the Categories page
    /// — leaving it behind would have the pass silently re-derive the same
    /// manual answer it just erased. A rule with no matching override row is
    /// therefore treated as hand-authored and survives; that fingerprint is
    /// the only thing separating the two, so mirror rules whose override was
    /// already deleted by an earlier pass now count as hand-authored for
    /// good.
    ///
    /// Everything happens in one transaction: a pass either fully lands or
    /// leaves history exactly as it was, never half-erased.
    pub fn recategorize_all(&self) -> Result<RecategorizeOutcome> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // Before the overrides themselves, while the (key_kind, pattern)
        // pairs identifying them still exist.
        let rules_cleared = tx.execute(
            "DELETE FROM category_rules
             WHERE match_kind = 'exact'
               AND EXISTS (
                   SELECT 1 FROM category_overrides ov
                   WHERE ov.key_kind = category_rules.key_kind
                     AND ov.pattern = category_rules.pattern
               )",
            [],
        )?;
        let overrides_cleared = tx.execute("DELETE FROM category_overrides", [])?;
        let pins_cleared = tx.execute(
            "UPDATE sessions SET category_id = NULL WHERE category_id IS NOT NULL",
            [],
        )?;
        tx.commit()?;
        Ok(RecategorizeOutcome {
            pins_cleared: pins_cleared as i64,
            overrides_cleared: overrides_cleared as i64,
            rules_cleared: rules_cleared as i64,
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
        assert!(store
            .list_categories()
            .unwrap()
            .iter()
            .any(|c| c.id == created.id));

        store
            .update_category(created.id, "Deep Work", "#654321")
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

    /// Inserts a browser-tab session with `url` and its derived `url_key`
    /// (see `category::url_key`) filled in, alongside the usual `domain`
    /// `category_key`, so tests can exercise `url`-kind rules.
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
    fn preview_category_change_counts_every_session_sharing_the_key() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let first = insert_session(&conn, "shared.exe", "2024-06-15T00:00:00Z");
        insert_session(&conn, "shared.exe", "2024-06-16T00:00:00Z");
        insert_session(&conn, "other.exe", "2024-06-16T00:00:00Z");

        let preview = store.preview_category_change(first).unwrap();
        assert_eq!(preview.key_kind, "exe");
        assert_eq!(preview.key, "shared.exe");
        assert_eq!(preview.affected_count, 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recategorize_all_erases_manual_categorization_and_leaves_rules_deciding() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let ruled = store.create_category("Ruled", "#111111").unwrap();
        let manual = store.create_category("Manual", "#222222").unwrap();
        store
            .create_rule("exe", "ruled.exe", "exact", ruled.id)
            .unwrap();

        // One of each way a session can end up categorized: a hand-authored
        // rule, a key-wide override, a per-session pin — plus a pin sitting
        // on top of a rule, the case a pass that skipped already-categorized
        // rows would leave frozen.
        let by_rule = insert_session(&conn, "ruled.exe", "2024-06-15T00:00:00Z");
        let by_override = insert_session(&conn, "override.exe", "2024-06-15T01:00:00Z");
        let by_pin = insert_session(&conn, "pinned.exe", "2024-06-15T02:00:00Z");
        let pinned_over_rule = insert_session(&conn, "ruled.exe", "2024-06-15T03:00:00Z");
        store.set_override(by_override, manual.id).unwrap();
        store.set_row_category(by_pin, Some(manual.id)).unwrap();
        store
            .set_row_category(pinned_over_rule, Some(manual.id))
            .unwrap();

        let outcome = store.recategorize_all().unwrap();
        assert_eq!(outcome.pins_cleared, 2);
        assert_eq!(outcome.overrides_cleared, 1);
        assert_eq!(
            outcome.rules_cleared, 1,
            "the exact rule set_override minted for override.exe goes with it"
        );

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(by_rule), Some(ruled.id));
        assert_eq!(category_of(by_override), None);
        assert_eq!(category_of(by_pin), None);
        assert_eq!(
            category_of(pinned_over_rule),
            Some(ruled.id),
            "a pinned row falls back to what the rules say, not to nothing"
        );

        // The hand-authored rule is still there; the minted one isn't.
        let patterns = store
            .list_rules()
            .unwrap()
            .into_iter()
            .map(|r| r.pattern)
            .collect::<Vec<_>>();
        assert!(patterns.contains(&"ruled.exe".to_string()));
        assert!(!patterns.contains(&"override.exe".to_string()));

        // Nothing manual left, so a second pass is a no-op.
        let again = store.recategorize_all().unwrap();
        assert_eq!(again.pins_cleared, 0);
        assert_eq!(again.overrides_cleared, 0);
        assert_eq!(again.rules_cleared, 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn category_resolution_prefers_row_pin_over_key_override_over_rule() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let rule_cat = store.create_category("RuleCat", "#111111").unwrap();
        let override_cat = store.create_category("OverrideCat", "#222222").unwrap();
        let pin_cat = store.create_category("PinCat", "#333333").unwrap();
        store
            .create_rule("exe", "shared.exe", "exact", rule_cat.id)
            .unwrap();

        let a = insert_session(&conn, "shared.exe", "2024-06-15T00:00:00Z");
        let b = insert_session(&conn, "shared.exe", "2024-06-16T00:00:00Z");

        let category_of = |rows: &[crate::core::store::queries::SessionRow], id: i64| {
            rows.iter().find(|r| r.id == id).unwrap().category_id
        };

        // Nothing pinned/overridden yet: both fall back to the rule.
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, a), Some(rule_cat.id));
        assert_eq!(category_of(&rows, b), Some(rule_cat.id));

        // A key-wide override (from picking a category on `a`) beats the
        // rule for every session sharing the key, including `b`.
        store.set_override(a, override_cat.id).unwrap();
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, a), Some(override_cat.id));
        assert_eq!(category_of(&rows, b), Some(override_cat.id));

        // Pinning `b` alone beats the key-wide override for `b` only — `a`
        // is unaffected.
        store.set_row_category(b, Some(pin_cat.id)).unwrap();
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, a), Some(override_cat.id));
        assert_eq!(category_of(&rows, b), Some(pin_cat.id));

        // Clearing the pin falls back to the key-wide override again.
        store.set_row_category(b, None).unwrap();
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(category_of(&rows, b), Some(override_cat.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_override_clears_a_pre_existing_per_session_pin_on_the_same_key() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let pin_cat = store.create_category("PinCat", "#333333").unwrap();
        let override_cat = store.create_category("OverrideCat", "#222222").unwrap();
        let a = insert_session(&conn, "pinned.exe", "2024-06-15T00:00:00Z");
        store.set_row_category(a, Some(pin_cat.id)).unwrap();

        store.set_override(a, override_cat.id).unwrap();

        let pinned: Option<i64> = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT category_id FROM sessions WHERE id = ?1",
                params![a],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            pinned, None,
            "a key-wide override should clear the per-session pin it supersedes"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_override_also_upserts_a_matching_category_rule() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let first_cat = store.create_category("FirstCat", "#111111").unwrap();
        let a = insert_session(&conn, "lockapp.exe", "2024-06-15T00:00:00Z");

        store.set_override(a, first_cat.id).unwrap();
        let rules = store.list_rules().unwrap();
        assert!(
            rules.iter().any(|r| r.key_kind == "exe"
                && r.pattern == "lockapp.exe"
                && r.category_id == first_cat.id),
            "applying a category key-wide should surface it as a rule on the Categories page too"
        );

        // Re-applying to a different category updates the rule in place
        // rather than erroring on the unique (key_kind, pattern) index.
        let second_cat = store.create_category("SecondCat", "#222222").unwrap();
        store.set_override(a, second_cat.id).unwrap();
        let rules = store.list_rules().unwrap();
        assert_eq!(
            rules
                .iter()
                .filter(|r| r.key_kind == "exe" && r.pattern == "lockapp.exe")
                .count(),
            1
        );
        assert!(rules
            .iter()
            .any(|r| r.pattern == "lockapp.exe" && r.category_id == second_cat.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_row_categories_pins_only_the_given_sessions() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let english = store.create_category("English", "#111111").unwrap();
        // Three rows on the same key — the shape of "chatgpt.com used for
        // two different things", where only some of them were English.
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
    fn set_overrides_still_widens_to_the_whole_key_when_explicitly_asked() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();
        let reader = crate::core::store::queries::HistoryReader::new(conn.clone());
        let filter = crate::core::store::queries::SessionFilter::default();

        let english = store.create_category("English", "#111111").unwrap();
        let a = insert_browser_session(&conn, "https://chatgpt.com/c/AAA", "2024-06-15T00:00:00Z");
        let c = insert_browser_session(&conn, "https://chatgpt.com/c/CCC", "2024-06-15T02:00:00Z");

        store.set_overrides(&[a], english.id).unwrap();

        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        let category_of = |id: i64| rows.iter().find(|r| r.id == id).unwrap().category_id;
        assert_eq!(category_of(a), Some(english.id));
        assert_eq!(category_of(c), Some(english.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recategorize_all_clears_bulk_pins_across_every_app_and_site() {
        let (store, path) = temp_store();
        let conn = store.conn.clone();

        let pin_cat = store.create_category("PinCat", "#333333").unwrap();
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

        let cat = store.create_category("Games", "#111111").unwrap();
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

        let cat = store.create_category("Work", "#222222").unwrap();
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
        let cat = store.create_category("Bad", "#333333").unwrap();
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

        // "Broad" is created (and thus would have won under the old
        // category-priority scheme) before "Specific", yet its rule is less
        // specific — specificity alone must decide the winner.
        let broad = store.create_category("Broad", "#444444").unwrap();
        let specific = store.create_category("Specific", "#555555").unwrap();
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

        let substring_cat = store.create_category("Substring", "#111111").unwrap();
        let prefix_cat = store.create_category("Prefix", "#222222").unwrap();
        let exact_cat = store.create_category("Exact", "#333333").unwrap();
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

        store.delete_rule(
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

        // Neither pattern collides with any seeded default rule (see
        // `sqlite::seed_default_categories`), so the only thing deciding the
        // winner here is pattern length.
        let short_cat = store.create_category("Short", "#111111").unwrap();
        let long_cat = store.create_category("Long", "#222222").unwrap();
        store
            .create_rule("domain", "example.net", "substring", short_cat.id)
            .unwrap();
        store
            .create_rule("domain", "corp.example.net", "substring", long_cat.id)
            .unwrap();

        let a = insert_browser_session(&conn, "https://wiki.corp.example.net/page", "2024-06-15T00:00:00Z");
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

        let regex_cat = store.create_category("Regex", "#111111").unwrap();
        let substring_cat = store.create_category("Substring", "#222222").unwrap();
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

        // "docs.example.org" is not one of the seeded default domain
        // patterns (see `sqlite::seed_default_categories`), so the only
        // rules in play are the ones this test creates.
        let work = store.create_category("Work", "#111111").unwrap();
        let english = store.create_category("English study", "#222222").unwrap();
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

        let cat = store.create_category("Docs", "#111111").unwrap();
        store
            .create_rule("url", "myeditor.exe", "substring", cat.id)
            .unwrap();

        let a = insert_session(&conn, "myeditor.exe", "2024-06-15T00:00:00Z");
        let rows = reader.list_sessions(10, 0, &filter).unwrap();
        assert_eq!(rows.iter().find(|r| r.id == a).unwrap().category_id, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn create_rule_rejects_invalid_key_kind() {
        let (store, path) = temp_store();
        let cat = store.create_category("Cat", "#111111").unwrap();
        assert!(store.create_rule("bogus", "x", "exact", cat.id).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
