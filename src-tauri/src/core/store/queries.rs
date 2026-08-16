use anyhow::Result;
use serde::Serialize;

use super::sqlite::SharedConnection;
use crate::core::category;

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: i64,
    pub app_name: String,
    pub exe_path: String,
    pub window_title: String,
    pub url: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub category_is_pinned: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ActivityStats {
    pub total_seconds: f64,
    pub session_count: i64,
    pub categories: Vec<CategoryStat>,
    pub apps: Vec<AppStat>,
    pub daily: Vec<DailyStat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryStat {
    pub category_id: Option<i64>,
    pub name: Option<String>,
    pub color: Option<String>,
    pub total_seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStat {
    pub label: String,
    pub kind: String,
    pub category_color: Option<String>,
    pub total_seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyStat {
    pub day: String,
    pub total_seconds: f64,
}

const TOP_APPS_LIMIT: i64 = 15;

pub struct HistoryReader {
    conn: SharedConnection,
}

impl HistoryReader {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    pub fn count_sessions(&self, filter: &SessionFilter) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let (where_sql, binds) = filter.to_sql();
        let sql = format!(
            "SELECT COUNT(*)
             FROM sessions s
             {rule_join}
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(s.category_id, r.category_id)
             WHERE {where_sql}",
            rule_join = category::rule_join_sql("r", "s"),
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            binds.iter().map(|b| b.as_ref()).collect();
        Ok(conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))?)
    }

    pub fn list_sessions(
        &self,
        limit: i64,
        offset: i64,
        filter: &SessionFilter,
    ) -> Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let (where_sql, mut binds) = filter.to_sql();
        let sql = format!(
            "SELECT s.id, s.app_name, s.exe_path, s.window_title, s.url, s.start_time, s.end_time,
                    cat.id, cat.name, cat.color, (s.category_id IS NOT NULL)
             FROM sessions s
             {rule_join}
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(s.category_id, r.category_id)
             WHERE {where_sql}
             ORDER BY s.start_time DESC
             LIMIT ? OFFSET ?",
            rule_join = category::rule_join_sql("r", "s"),
        );
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            binds.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(SessionRow {
                    id: row.get(0)?,
                    app_name: row.get(1)?,
                    exe_path: row.get(2)?,
                    window_title: row.get(3)?,
                    url: row.get(4)?,
                    start_time: row.get(5)?,
                    end_time: row.get(6)?,
                    category_id: row.get(7)?,
                    category_name: row.get(8)?,
                    category_color: row.get(9)?,
                    category_is_pinned: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn activity_stats(&self, start: &str, end: &str) -> Result<ActivityStats> {
        const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
        let conn = self.conn.lock().unwrap();

        let (total_seconds, session_count): (f64, i64) = conn.query_row(
            &format!(
                "SELECT COALESCE(SUM({DURATION}), 0.0), COUNT(*)
                 FROM sessions s
                 WHERE s.end_time IS NOT NULL AND s.start_time >= ?1 AND s.start_time < ?2"
            ),
            rusqlite::params![start, end],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let category_join = format!(
            "             {rule_join}
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(s.category_id, r.category_id)",
            rule_join = category::rule_join_sql("r", "s"),
        );

        let mut stmt = conn.prepare(&format!(
            "SELECT cat.id, cat.name, cat.color, COALESCE(SUM({DURATION}), 0.0), COUNT(*)
             FROM sessions s
             {category_join}
             WHERE s.end_time IS NOT NULL AND s.start_time >= ?1 AND s.start_time < ?2
             GROUP BY cat.id
             ORDER BY 4 DESC"
        ))?;
        let categories = stmt
            .query_map(rusqlite::params![start, end], |row| {
                Ok(CategoryStat {
                    category_id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    total_seconds: row.get(3)?,
                    session_count: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT s.category_key_kind, s.category_key, MIN(s.app_name), cat.color,
                    COALESCE(SUM({DURATION}), 0.0), COUNT(*)
             FROM sessions s
             {category_join}
             WHERE s.end_time IS NOT NULL AND s.start_time >= ?1 AND s.start_time < ?2
             GROUP BY s.category_key_kind, s.category_key
             ORDER BY 5 DESC
             LIMIT ?3"
        ))?;
        let apps = stmt
            .query_map(rusqlite::params![start, end, TOP_APPS_LIMIT], |row| {
                let kind: String = row.get(0)?;
                let key: String = row.get(1)?;
                let app_name: String = row.get(2)?;
                let label = if kind == "domain" { key } else { app_name };
                Ok(AppStat {
                    label,
                    kind,
                    category_color: row.get(3)?,
                    total_seconds: row.get(4)?,
                    session_count: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stmt = conn.prepare(&format!(
            "SELECT date(s.start_time, 'localtime') AS day, COALESCE(SUM({DURATION}), 0.0)
             FROM sessions s
             WHERE s.end_time IS NOT NULL AND s.start_time >= ?1 AND s.start_time < ?2
             GROUP BY day
             ORDER BY day ASC"
        ))?;
        let daily = stmt
            .query_map(rusqlite::params![start, end], |row| {
                Ok(DailyStat {
                    day: row.get(0)?,
                    total_seconds: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(ActivityStats {
            total_seconds,
            session_count,
            categories,
            apps,
            daily,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub enum CategoryFilter {
    #[default]
    All,
    Uncategorized,
    Category(i64),
}

#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub search: Option<String>,
    pub category: CategoryFilter,
    pub start: Option<String>,
    pub end: Option<String>,
}

impl SessionFilter {
    fn to_sql(&self) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
        let mut clauses = vec!["1=1".to_string()];
        let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(term) = self
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            clauses.push(
                "(s.app_name LIKE ? ESCAPE '\\' OR s.window_title LIKE ? ESCAPE '\\' OR s.url LIKE ? ESCAPE '\\')"
                    .to_string(),
            );
            let pattern = format!("%{}%", term.replace('%', "\\%").replace('_', "\\_"));
            for _ in 0..3 {
                binds.push(Box::new(pattern.clone()));
            }
        }

        match &self.category {
            CategoryFilter::All => {}
            CategoryFilter::Uncategorized => clauses.push("cat.id IS NULL".to_string()),
            CategoryFilter::Category(id) => {
                clauses.push("cat.id = ?".to_string());
                binds.push(Box::new(*id));
            }
        }

        if let Some(start) = &self.start {
            clauses.push("(s.end_time IS NULL OR s.end_time > ?)".to_string());
            binds.push(Box::new(start.clone()));
        }
        if let Some(end) = &self.end {
            clauses.push("s.start_time < ?".to_string());
            binds.push(Box::new(end.clone()));
        }

        (clauses.join(" AND "), binds)
    }
}

#[cfg(test)]
mod stats_tests {
    use rusqlite::params;

    use super::*;
    use crate::core::store::categories::CategoryStore;
    use crate::core::store::sqlite::open_connection;

    fn temp_reader() -> (
        HistoryReader,
        CategoryStore,
        SharedConnection,
        std::path::PathBuf,
    ) {
        let path = std::env::temp_dir().join(format!("makora-stats-test-{}.sqlite", uuid_ish()));
        let conn = open_connection(&path).expect("opening test database");
        (
            HistoryReader::new(conn.clone()),
            CategoryStore::new(conn.clone()),
            conn,
            path,
        )
    }

    fn uuid_ish() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_session(
        conn: &SharedConnection,
        app_name: &str,
        url: Option<&str>,
        category_key: &str,
        category_key_kind: &str,
        start_time: &str,
        end_time: Option<&str>,
    ) {
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, start_time, end_time)
                 VALUES (?1, ?1, 'title', ?2, ?3, ?4, ?5, ?6)",
                params![app_name, url, category_key, category_key_kind, start_time, end_time],
            )
            .unwrap();
    }

    #[test]
    fn totals_sum_closed_sessions_and_skip_still_open_ones() {
        let (reader, _categories, conn, path) = temp_reader();
        insert_session(
            &conn,
            "code.exe",
            None,
            "code.exe",
            "exe",
            "2024-06-15T10:00:00Z",
            Some("2024-06-15T10:10:00Z"),
        );
        insert_session(
            &conn,
            "chrome.exe",
            None,
            "chrome.exe",
            "exe",
            "2024-06-15T11:00:00Z",
            None,
        );

        let stats = reader
            .activity_stats("2024-06-15T00:00:00Z", "2024-06-16T00:00:00Z")
            .unwrap();
        assert_eq!(
            stats.session_count, 1,
            "an open session has no known duration yet and should be skipped"
        );
        assert!((stats.total_seconds - 600.0).abs() < 0.01);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn range_is_half_open_on_start_time() {
        let (reader, _categories, conn, path) = temp_reader();
        insert_session(
            &conn,
            "code.exe",
            None,
            "code.exe",
            "exe",
            "2024-06-14T23:59:00Z",
            Some("2024-06-15T00:00:00Z"),
        );
        insert_session(
            &conn,
            "code.exe",
            None,
            "code.exe",
            "exe",
            "2024-06-15T00:00:00Z",
            Some("2024-06-15T00:10:00Z"),
        );
        insert_session(
            &conn,
            "code.exe",
            None,
            "code.exe",
            "exe",
            "2024-06-16T00:00:00Z",
            Some("2024-06-16T00:10:00Z"),
        );

        let stats = reader
            .activity_stats("2024-06-15T00:00:00Z", "2024-06-16T00:00:00Z")
            .unwrap();
        assert_eq!(stats.session_count, 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn category_breakdown_resolves_via_rules_and_groups_uncategorized_separately() {
        let (reader, categories, conn, path) = temp_reader();
        let cat = categories.create_category("Dev", "#111111").unwrap();
        categories
            .create_rule("exe", "myeditor.exe", "exact", cat.id)
            .unwrap();

        insert_session(
            &conn,
            "myeditor.exe",
            None,
            "myeditor.exe",
            "exe",
            "2024-06-15T10:00:00Z",
            Some("2024-06-15T10:10:00Z"),
        );
        insert_session(
            &conn,
            "plainapp.exe",
            None,
            "plainapp.exe",
            "exe",
            "2024-06-15T11:00:00Z",
            Some("2024-06-15T11:05:00Z"),
        );

        let stats = reader
            .activity_stats("2024-06-15T00:00:00Z", "2024-06-16T00:00:00Z")
            .unwrap();
        assert_eq!(stats.categories.len(), 2);
        let dev = stats
            .categories
            .iter()
            .find(|c| c.category_id == Some(cat.id))
            .unwrap();
        assert!((dev.total_seconds - 600.0).abs() < 0.01);
        let uncategorized = stats
            .categories
            .iter()
            .find(|c| c.category_id.is_none())
            .unwrap();
        assert!((uncategorized.total_seconds - 300.0).abs() < 0.01);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn app_breakdown_labels_browser_tab_rows_by_domain() {
        let (reader, _categories, conn, path) = temp_reader();
        insert_session(
            &conn,
            "chrome.exe",
            Some("https://github.com/foo"),
            "github.com",
            "domain",
            "2024-06-15T10:00:00Z",
            Some("2024-06-15T10:05:00Z"),
        );

        let stats = reader
            .activity_stats("2024-06-15T00:00:00Z", "2024-06-16T00:00:00Z")
            .unwrap();
        assert_eq!(stats.apps.len(), 1);
        assert_eq!(stats.apps[0].label, "github.com");
        assert_eq!(stats.apps[0].kind, "domain");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn time_range_filter_keeps_rows_overlapping_the_range() {
        let (reader, _categories, conn, path) = temp_reader();
        insert_session(
            &conn,
            "before.exe",
            None,
            "before.exe",
            "exe",
            "2024-06-15T08:00:00Z",
            Some("2024-06-15T09:00:00Z"),
        );
        insert_session(
            &conn,
            "straddles.exe",
            None,
            "straddles.exe",
            "exe",
            "2024-06-15T09:30:00Z",
            Some("2024-06-15T10:30:00Z"),
        );
        insert_session(
            &conn,
            "inside.exe",
            None,
            "inside.exe",
            "exe",
            "2024-06-15T11:00:00Z",
            Some("2024-06-15T11:10:00Z"),
        );
        insert_session(
            &conn,
            "open.exe",
            None,
            "open.exe",
            "exe",
            "2024-06-15T11:30:00Z",
            None,
        );
        insert_session(
            &conn,
            "after.exe",
            None,
            "after.exe",
            "exe",
            "2024-06-15T13:00:00Z",
            Some("2024-06-15T13:10:00Z"),
        );

        let filter = SessionFilter {
            start: Some("2024-06-15T10:00:00Z".to_string()),
            end: Some("2024-06-15T12:00:00Z".to_string()),
            ..Default::default()
        };
        let rows = reader.list_sessions(50, 0, &filter).unwrap();
        let names: Vec<&str> = rows.iter().map(|r| r.app_name.as_str()).collect();
        assert_eq!(names, ["open.exe", "inside.exe", "straddles.exe"]);
        assert_eq!(reader.count_sessions(&filter).unwrap(), 3);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn daily_breakdown_has_one_bucket_per_day_and_sums_to_the_total() {
        let (reader, _categories, conn, path) = temp_reader();
        insert_session(
            &conn,
            "code.exe",
            None,
            "code.exe",
            "exe",
            "2024-06-15T10:00:00Z",
            Some("2024-06-15T10:10:00Z"),
        );
        insert_session(
            &conn,
            "code.exe",
            None,
            "code.exe",
            "exe",
            "2024-06-16T10:00:00Z",
            Some("2024-06-16T10:20:00Z"),
        );

        let stats = reader
            .activity_stats("2024-06-15T00:00:00Z", "2024-06-17T00:00:00Z")
            .unwrap();
        assert_eq!(
            stats.daily.len(),
            2,
            "each session lands in its own day bucket"
        );
        let daily_total: f64 = stats.daily.iter().map(|d| d.total_seconds).sum();
        assert!((daily_total - stats.total_seconds).abs() < 0.01);

        let _ = std::fs::remove_file(&path);
    }
}
