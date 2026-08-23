use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use anyhow::Result;
use chrono::DateTime;
use rusqlite::Connection;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyDocument {
    pub key_kind: String,
    pub key: String,
    pub document: String,
    pub seconds: f64,
    pub doc_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityDocument {
    pub activity_id: Option<i64>,
    pub key_kind: String,
    pub key: String,
    pub url_key: Option<String>,
    pub window_title: String,
    pub document: String,
    pub seconds: f64,
    pub session_ids: Vec<i64>,
    pub observation_days: Vec<String>,
    pub distinct_days: usize,
    pub labeled_session_count: usize,
    pub doc_hash: String,
}

#[derive(Default)]
struct KeyParts {
    apps: BTreeSet<String>,
    titles: HashMap<String, f64>,
    paths: HashMap<String, f64>,
    seconds: f64,
}

#[derive(Default)]
struct ActivityParts {
    activity_id: Option<i64>,
    apps: BTreeSet<String>,
    session_ids: Vec<i64>,
    days: BTreeSet<String>,
    seconds: f64,
    labeled_session_count: usize,
}

/// Builds one semantic document per stable categorization key.
///
/// The SQL groups raw observations once; the small amount of ranking and formatting happens in
/// Rust so the query remains compatible with the SQLite versions bundled by Tauri.
pub fn build(conn: &Connection) -> Result<Vec<KeyDocument>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(a.key_kind, s.category_key_kind), COALESCE(a.key, s.category_key),
                COALESCE(NULLIF(a.app_name, ''), s.app_name),
                COALESCE(NULLIF(a.display_title, ''), s.window_title),
                COALESCE(a.url_key, s.url_key),
                SUM((julianday(s.end_time) - julianday(s.start_time)) * 86400.0)
         FROM sessions s LEFT JOIN activities a ON a.id = s.activity_id
         WHERE s.end_time IS NOT NULL
         GROUP BY COALESCE(a.id, s.id), COALESCE(a.key_kind, s.category_key_kind),
                  COALESCE(a.key, s.category_key), COALESCE(NULLIF(a.app_name, ''), s.app_name),
                  COALESCE(NULLIF(a.display_title, ''), s.window_title), COALESCE(a.url_key, s.url_key)",
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

/// Builds one document per title-bearing activity within a stable key.
///
/// Unlike [`build`], this deliberately includes the currently open session. Its duration is
/// zero until the tracker closes it, but its title is still useful evidence for the next AI run.
pub fn build_activities(conn: &Connection) -> Result<Vec<ActivityDocument>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(a.id, s.id), a.id, s.id, COALESCE(a.key_kind, s.category_key_kind),
                COALESCE(a.key, s.category_key), COALESCE(NULLIF(a.app_name, ''), s.app_name),
                COALESCE(NULLIF(a.display_title, ''), s.window_title), COALESCE(a.url_key, s.url_key),
                start_time, CASE WHEN end_time IS NULL THEN 0.0
                                 ELSE MAX(0.0, (julianday(end_time) - julianday(start_time)) * 86400.0)
                            END,
                (COALESCE(a.category_id, s.category_id) IS NOT NULL OR
                 COALESCE(a.rule_category_id, s.rule_category_id) IS NOT NULL)
         FROM sessions s LEFT JOIN activities a ON a.id = s.activity_id",
    )?;
    let mut grouped: BTreeMap<(i64, String, String, Option<String>, String), ActivityParts> =
        BTreeMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, f64>(9)?.max(0.0),
            row.get::<_, bool>(10)?,
        ))
    })?;
    for row in rows {
        let (
            group_id,
            activity_id,
            session_id,
            kind,
            key,
            app,
            title,
            url_key,
            start_time,
            seconds,
            labeled,
        ) = row?;
        let group_key = (group_id, kind, key, url_key, title);
        let parts = grouped.entry(group_key).or_insert_with(|| ActivityParts {
            activity_id,
            ..Default::default()
        });
        parts.seconds += seconds;
        if labeled {
            parts.labeled_session_count += 1;
        }
        parts.session_ids.push(session_id);
        if !app.trim().is_empty() {
            parts.apps.insert(app);
        }
        if let Ok(start) = DateTime::parse_from_rfc3339(&start_time) {
            parts
                .days
                .insert(start.with_timezone(&chrono::Utc).date_naive().to_string());
        }
    }

    Ok(grouped
        .into_iter()
        .map(
            |((_group_id, key_kind, key, url_key, window_title), mut parts)| {
                parts.session_ids.sort_unstable();
                let mut document = humanize_key(&key);
                if !parts.apps.is_empty() {
                    document.push_str(" app ");
                    document.push_str(&parts.apps.into_iter().collect::<Vec<_>>().join(" "));
                }
                if let Some(path) = url_key.as_deref().filter(|path| !path.trim().is_empty()) {
                    document.push_str(" path ");
                    document.push_str(path);
                }
                if !window_title.trim().is_empty() {
                    document.push_str(" title ");
                    document.extend(window_title.chars().take(160));
                }
                let doc_hash = hash_document(&document);
                ActivityDocument {
                    activity_id: parts.activity_id,
                    key_kind,
                    key,
                    url_key,
                    window_title,
                    document,
                    seconds: parts.seconds,
                    session_ids: parts.session_ids,
                    observation_days: parts.days.iter().cloned().collect(),
                    distinct_days: parts.days.len(),
                    labeled_session_count: parts.labeled_session_count,
                    doc_hash,
                }
            },
        )
        .collect())
}

impl ActivityDocument {
    pub fn cache_key(&self) -> String {
        activity_cache_key(
            &self.key_kind,
            &self.key,
            self.url_key.as_deref(),
            &self.window_title,
        )
    }
}

/// The single cache-key format shared by activity documents and title-level training labels.
pub fn activity_cache_key(
    key_kind: &str,
    key: &str,
    url_key: Option<&str>,
    window_title: &str,
) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{window_title}",
        key_kind,
        key,
        url_key.unwrap_or_default(),
    )
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

    #[test]
    fn activity_documents_keep_titles_and_open_rows() {
        let path = std::env::temp_dir().join(format!(
            "makora-keydoc-activities-{}-{}.sqlite",
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
                ('Chrome', 'chrome', 'Lecture', 'https://youtube.com/watch', 'youtube.com', 'domain',
                 'youtube.com/watch', '2024-01-01T00:00:00Z', '2024-01-01T00:10:00Z'),
                ('Chrome', 'chrome', 'Game stream', 'https://youtube.com/watch', 'youtube.com', 'domain',
                 'youtube.com/watch', '2024-01-02T00:00:00Z', NULL);",
        )
        .unwrap();

        let docs = build_activities(&conn.lock().unwrap()).unwrap();
        assert_eq!(docs.len(), 2);
        let open = docs
            .iter()
            .find(|doc| doc.window_title == "Game stream")
            .unwrap();
        assert_eq!(open.seconds, 0.0);
        assert_eq!(open.session_ids.len(), 1);
        assert_eq!(open.distinct_days, 1);
        assert!(open.document.contains("path youtube.com/watch"));
        assert!(open.document.contains("title Game stream"));
        let _ = std::fs::remove_file(path);
    }
}
