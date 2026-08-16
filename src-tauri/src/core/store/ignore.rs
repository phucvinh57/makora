use anyhow::{anyhow, bail, Context, Result};
use rusqlite::params;
use serde::Serialize;

use super::sqlite::SharedConnection;
use crate::core::category::{category_key, normalized_domain, MatchKind};
use crate::core::ignore::IgnorePolicy;

#[derive(Debug, Clone, Serialize)]
pub struct IgnoreRule {
    pub id: i64,
    pub kind: String,
    pub pattern: String,
    pub match_kind: String,
}

pub struct IgnoreStore {
    conn: SharedConnection,
}

impl IgnoreStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    pub fn list_rules(&self) -> Result<Vec<IgnoreRule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, kind, pattern, match_kind FROM ignore_rules ORDER BY kind, pattern",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(IgnoreRule {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    pattern: row.get(2)?,
                    match_kind: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create_rule(&self, kind: &str, pattern: &str, match_kind: &str) -> Result<IgnoreRule> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            bail!("ignore pattern must not be empty");
        }
        let parsed_match_kind = MatchKind::parse(match_kind)
            .ok_or_else(|| anyhow!("invalid match kind: {match_kind}"))?;
        if parsed_match_kind == MatchKind::Regex {
            regex::Regex::new(pattern)
                .with_context(|| format!("invalid regex pattern: {pattern}"))?;
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ignore_rules (kind, pattern, match_kind) VALUES (?1, ?2, ?3)",
            params![kind, pattern, parsed_match_kind.as_str()],
        )?;
        Ok(IgnoreRule {
            id: conn.last_insert_rowid(),
            kind: kind.to_string(),
            pattern: pattern.to_string(),
            match_kind: parsed_match_kind.as_str().to_string(),
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM ignore_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn matches(&self, kind: &str, candidate: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM ignore_rules
                WHERE kind = ?1 AND (
                    (match_kind = 'exact' AND lower(pattern) = lower(?2))
                    OR (match_kind = 'substring' AND instr(lower(?2), lower(pattern)) > 0)
                    OR (match_kind = 'regex' AND ?2 REGEXP pattern)
                )
            )",
            params![kind, candidate],
            |row| row.get(0),
        )
        .unwrap_or(false)
    }
}

impl IgnorePolicy for IgnoreStore {
    fn is_app_ignored(&self, exe_path: &str) -> bool {
        let (_, file_name) = category_key(exe_path, None);
        self.matches("app", &file_name)
    }

    fn is_site_ignored(&self, url: &str) -> bool {
        match normalized_domain(url) {
            Some(domain) => self.matches("site", &domain),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    fn temp_store() -> (IgnoreStore, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("makora-ignore-store-test-{}.sqlite", uuid_ish()));
        let conn = open_connection(&path).expect("opening test database");
        (IgnoreStore::new(conn), path)
    }

    fn uuid_ish() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64
    }

    #[test]
    fn rule_crud_round_trips_through_sqlite() {
        let (store, path) = temp_store();

        let rule = store.create_rule("app", "steam.exe", "substring").unwrap();
        assert!(store.list_rules().unwrap().iter().any(|r| r.id == rule.id));

        store.delete_rule(rule.id).unwrap();
        assert!(!store.list_rules().unwrap().iter().any(|r| r.id == rule.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_pattern_is_rejected() {
        let (store, path) = temp_store();
        assert!(store.create_rule("app", "   ", "substring").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_match_kind_is_rejected() {
        let (store, path) = temp_store();
        assert!(store.create_rule("app", "steam", "bogus").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_regex_pattern_is_rejected_up_front() {
        let (store, path) = temp_store();
        assert!(store.create_rule("app", "(unclosed", "regex").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn substring_app_pattern_matches_as_a_case_insensitive_substring_of_the_exe_file_name() {
        let (store, path) = temp_store();
        store.create_rule("app", "Steam", "substring").unwrap();

        assert!(store.is_app_ignored(r"C:\Program Files\Steam\steamwebhelper.exe"));
        assert!(!store.is_app_ignored(r"C:\Windows\notepad.exe"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn exact_app_pattern_does_not_match_a_broader_substring() {
        let (store, path) = temp_store();
        store.create_rule("app", "steam.exe", "exact").unwrap();

        assert!(store.is_app_ignored(r"C:\Program Files\Steam\steam.exe"));
        assert!(!store.is_app_ignored(r"C:\Program Files\Steam\steamwebhelper.exe"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn regex_app_pattern_matches_against_the_exe_file_name() {
        let (store, path) = temp_store();
        store
            .create_rule("app", r"^game\d+\.exe$", "regex")
            .unwrap();

        assert!(store.is_app_ignored(r"C:\Games\game1.exe"));
        assert!(!store.is_app_ignored(r"C:\Games\game.exe"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn site_pattern_matches_against_the_normalized_url_domain_not_the_full_url() {
        let (store, path) = temp_store();
        store
            .create_rule("site", "youtube.com", "substring")
            .unwrap();

        assert!(store.is_site_ignored("https://www.youtube.com/watch?v=abc"));
        assert!(!store.is_site_ignored("https://example.com/youtube.com"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn no_rules_ignores_nothing() {
        let (store, path) = temp_store();
        assert!(!store.is_app_ignored("anything.exe"));
        assert!(!store.is_site_ignored("https://example.com"));
        let _ = std::fs::remove_file(&path);
    }
}
