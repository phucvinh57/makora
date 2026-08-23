use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use super::sqlite::SharedConnection;

#[derive(Debug, Clone, Serialize)]
pub struct UrlQueryParam {
    pub id: i64,
    pub domain: String,
    pub param: String,
}

pub struct UrlQueryStore {
    conn: SharedConnection,
}

impl UrlQueryStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<UrlQueryParam>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, domain, param FROM url_query_whitelist ORDER BY domain, param")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(UrlQueryParam {
                    id: row.get(0)?,
                    domain: row.get(1)?,
                    param: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn create(&self, domain: &str, param: &str) -> Result<UrlQueryParam> {
        let domain = domain.trim().to_ascii_lowercase();
        let param = param.trim().to_string();
        if domain.is_empty() || param.is_empty() {
            anyhow::bail!("domain and parameter must not be empty");
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO url_query_whitelist (domain, param) VALUES (?1, ?2)",
            params![domain, param],
        )?;
        Ok(UrlQueryParam {
            id: conn.last_insert_rowid(),
            domain,
            param,
        })
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM url_query_whitelist WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn params_for(&self, domain: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT param FROM url_query_whitelist WHERE domain = ?1 ORDER BY param")?;
        let rows = stmt
            .query_map(params![domain.to_ascii_lowercase()], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
