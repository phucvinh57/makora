use anyhow::{bail, Result};
use rusqlite::params;
use serde::Serialize;

use super::sqlite::SharedConnection;
use crate::core::category::normalized_domain;

#[derive(Debug, Clone, Serialize)]
pub struct QueryWhitelistEntry {
    pub id: i64,
    pub domain: String,
    pub param: String,
}

/// Read/write access to the `url_query_whitelist` table — the privacy
/// boundary for browser-tab query strings (see `ingest::normalize_url`).
/// Everything not explicitly whitelisted for a URL's domain is stripped
/// before the URL is ever stored, so a lookup here decides what survives
/// into `sessions.url`/`url_key`. Shares a connection with the other stores
/// for the same reason they do — SQLite only allows one writer regardless of
/// connection count.
pub struct QueryWhitelistStore {
    conn: SharedConnection,
}

impl QueryWhitelistStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    pub fn list_entries(&self) -> Result<Vec<QueryWhitelistEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, domain, param FROM url_query_whitelist ORDER BY domain, param")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(QueryWhitelistEntry {
                    id: row.get(0)?,
                    domain: row.get(1)?,
                    param: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// `domain` is normalized the same way a session's browser-tab key is
    /// (lowercased, `www.`-stripped — see [`normalized_domain`]) so an entry
    /// always matches the domain a session actually resolves to. `param` is
    /// trimmed and has a leading `?`/`&` stripped, so pasting a copied query
    /// fragment like `?v` or `&v` still saves as `v`.
    pub fn create_entry(&self, domain: &str, param: &str) -> Result<QueryWhitelistEntry> {
        let domain = normalized_domain(domain.trim()).ok_or_else(|| {
            anyhow::anyhow!("invalid domain: {domain}")
        })?;
        let param = param.trim().trim_start_matches(['?', '&']).to_string();
        if param.is_empty() {
            bail!("query param must not be empty");
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO url_query_whitelist (domain, param) VALUES (?1, ?2)",
            params![domain, param],
        )?;
        Ok(QueryWhitelistEntry {
            id: conn.last_insert_rowid(),
            domain,
            param,
        })
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM url_query_whitelist WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Param names whitelisted for `domain` (already normalized, e.g. from
    /// [`normalized_domain`]) — an exact match, not a substring, so
    /// `m.youtube.com` needs its own entry separate from `youtube.com`.
    /// Infallible (empty on any error) since this only ever feeds a
    /// best-effort privacy filter, never a decision worth failing a request
    /// over.
    pub fn allowed_params(&self, domain: &str) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn
            .prepare("SELECT param FROM url_query_whitelist WHERE domain = ?1")
        {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![domain], |row| row.get(0))
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<String>>>())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    fn temp_store() -> (QueryWhitelistStore, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "makora-query-whitelist-store-test-{}.sqlite",
            uuid_ish()
        ));
        let conn = open_connection(&path).expect("opening test database");
        (QueryWhitelistStore::new(conn), path)
    }

    fn uuid_ish() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64
    }

    #[test]
    fn entry_crud_round_trips_through_sqlite() {
        let (store, path) = temp_store();

        let seeded = store.list_entries().unwrap();
        assert!(
            seeded.iter().any(|e| e.domain == "youtube.com" && e.param == "v"),
            "youtube.com/v should be seeded by migration"
        );

        let entry = store.create_entry("example.com", "id").unwrap();
        assert!(store.list_entries().unwrap().iter().any(|e| e.id == entry.id));

        store.delete_entry(entry.id).unwrap();
        assert!(!store.list_entries().unwrap().iter().any(|e| e.id == entry.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn create_entry_normalizes_domain_and_trims_param() {
        let (store, path) = temp_store();

        let entry = store.create_entry("WWW.Example.com", "?id").unwrap();
        assert_eq!(entry.domain, "example.com");
        assert_eq!(entry.param, "id");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn duplicate_domain_param_pair_is_rejected() {
        let (store, path) = temp_store();
        assert!(store.create_entry("youtube.com", "v").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_param_is_rejected() {
        let (store, path) = temp_store();
        assert!(store.create_entry("example.com", "  ").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn allowed_params_matches_domain_exactly_not_as_a_substring() {
        let (store, path) = temp_store();
        let allowed = store.allowed_params("youtube.com");
        assert_eq!(allowed, vec!["v".to_string()]);
        assert!(store.allowed_params("m.youtube.com").is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
