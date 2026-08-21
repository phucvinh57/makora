use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyDocument {
    pub key_kind: String,
    pub key: String,
    pub document: String,
    pub seconds: f64,
    pub doc_hash: String,
}

#[derive(Default)]
struct KeyParts {
    apps: BTreeSet<String>,
    titles: HashMap<String, f64>,
    paths: HashMap<String, f64>,
    seconds: f64,
}

/// Builds one semantic document per stable categorization key.
///
/// The SQL groups raw observations once; the small amount of ranking and formatting happens in
/// Rust so the query remains compatible with the SQLite versions bundled by Tauri.
pub fn build(conn: &Connection) -> Result<Vec<KeyDocument>> {
    let mut stmt = conn.prepare(
        "SELECT category_key_kind, category_key, app_name, window_title, url_key,
                SUM((julianday(end_time) - julianday(start_time)) * 86400.0)
         FROM sessions
         WHERE end_time IS NOT NULL
         GROUP BY category_key_kind, category_key, app_name, window_title, url_key",
    )?;
    let mut grouped: BTreeMap<(String, String), KeyParts> = BTreeMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<f64>>(5)?.unwrap_or_default().max(0.0),
        ))
    })?;
    for row in rows {
        let (kind, key, app, title, path, seconds) = row?;
        let parts = grouped.entry((kind.clone(), key)).or_default();
        parts.seconds += seconds;
        if !app.trim().is_empty() {
            parts.apps.insert(app);
        }
        if !title.trim().is_empty() {
            *parts.titles.entry(title).or_default() += seconds;
        }
        if kind == "domain" {
            if let Some(path) = path.filter(|path| !path.trim().is_empty()) {
                *parts.paths.entry(path).or_default() += seconds;
            }
        }
    }

    Ok(grouped
        .into_iter()
        .map(|((key_kind, key), parts)| {
            let mut document = humanize_key(&key);
            if !parts.apps.is_empty() {
                document.push_str(" apps ");
                document.push_str(&parts.apps.into_iter().collect::<Vec<_>>().join(" "));
            }
            append_ranked(&mut document, "titles", parts.titles, 20);
            if key_kind == "domain" {
                append_ranked(&mut document, "paths", parts.paths, 5);
            }
            let doc_hash = hash_document(&document);
            KeyDocument {
                key_kind,
                key,
                document,
                seconds: parts.seconds,
                doc_hash,
            }
        })
        .collect())
}

fn append_ranked(document: &mut String, label: &str, values: HashMap<String, f64>, limit: usize) {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (value, _) in values.into_iter().take(limit) {
        document.push(' ');
        document.push_str(label);
        document.push(' ');
        document.extend(value.chars().take(160));
    }
}

fn humanize_key(key: &str) -> String {
    key.chars()
        .map(|value| match value {
            '.' | '-' | '_' => ' ',
            value => value,
        })
        .collect::<String>()
}

fn hash_document(document: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    document.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    #[test]
    fn aggregates_titles_by_duration_and_humanizes_keys() {
        let path = std::env::temp_dir().join(format!(
            "makora-keydoc-test-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = open_connection(&path).unwrap();
        conn.lock().unwrap().execute_batch(
            "INSERT INTO sessions
                (app_name, exe_path, window_title, url, category_key, category_key_kind,
                 url_key, start_time, end_time)
             VALUES
                ('Chrome', 'chrome', 'Short title', 'https://github.com/a', 'github.com', 'domain',
                 'github.com/a', '2024-01-01T00:00:00Z', '2024-01-01T00:00:10Z'),
                ('Chrome', 'chrome', 'Important Rust repository', 'https://github.com/b', 'github.com', 'domain',
                 'github.com/b', '2024-01-01T00:01:00Z', '2024-01-01T00:03:00Z');",
        )
        .unwrap();

        let docs = build(&conn.lock().unwrap()).unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].document.contains("github com"));
        assert!(docs[0].document.contains("Important Rust repository"));
        assert!(docs[0].seconds > 129.0 && docs[0].seconds < 131.0);
        assert!(!docs[0].doc_hash.is_empty());
        let _ = std::fs::remove_file(path);
    }
}
