use anyhow::Result;
use serde::Serialize;

use crate::category::source_url;

use super::categories::load_category_tree;
use super::sqlite::SharedConnection;
use super::HistoryStore;

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: i64,
    pub activity_id: i64,
    pub app_name: String,
    pub source_kind: String,
    pub source_key: String,
    pub title: String,
    pub url: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub category_is_pinned: bool,
    pub category_source: Option<String>,
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

/// `sessions` joined out to its activity, source and resolved category. A manual pin
/// (`a.category_id`) always wins over a pending AI label (`l.category_id`, only when not
/// rejected).
const JOIN: &str = "FROM sessions s
     JOIN activities a ON a.id = s.activity_id
     JOIN sources src ON src.id = a.source_id
     LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
     LEFT JOIN categories cat ON cat.id = COALESCE(a.category_id, l.category_id)";

pub struct SqliteHistoryStore {
    conn: SharedConnection,
}

impl SqliteHistoryStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }
}

impl HistoryStore for SqliteHistoryStore {
    fn count_sessions(&self, filter: &SessionFilter) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let category_ids = expanded_category_ids(&conn, filter)?;
        let (where_sql, binds) = filter.to_sql(category_ids.as_deref());
        let sql = format!("SELECT COUNT(*) {JOIN} WHERE {where_sql}");
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            binds.iter().map(|b| b.as_ref()).collect();
        Ok(conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))?)
    }

    fn list_sessions(
        &self,
        limit: i64,
        offset: i64,
        filter: &SessionFilter,
    ) -> Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let category_ids = expanded_category_ids(&conn, filter)?;
        let (where_sql, mut binds) = filter.to_sql(category_ids.as_deref());
        let sql = format!(
            "SELECT s.id, s.activity_id, src.name, src.kind, src.key, a.path, a.title,
                    s.start_time, s.end_time,
                    cat.id, cat.name, cat.color, a.category_id IS NOT NULL,
                    CASE
                      WHEN a.category_id IS NOT NULL THEN 'manual'
                      WHEN l.category_id IS NOT NULL THEN 'ai'
                      ELSE NULL
                    END
             {JOIN}
             WHERE {where_sql}
             ORDER BY s.start_time DESC
             LIMIT ? OFFSET ?"
        );
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            binds.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let source_kind: String = row.get(3)?;
                let source_key: String = row.get(4)?;
                let path: String = row.get(5)?;
                let url = (source_kind == "domain").then(|| source_url(&source_key, &path));
                Ok(SessionRow {
                    id: row.get(0)?,
                    activity_id: row.get(1)?,
                    app_name: row.get(2)?,
                    source_kind,
                    source_key,
                    title: row.get(6)?,
                    url,
                    start_time: row.get(7)?,
                    end_time: row.get(8)?,
                    category_id: row.get(9)?,
                    category_name: row.get(10)?,
                    category_color: row.get(11)?,
                    category_is_pinned: row.get(12)?,
                    category_source: row.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn activity_stats(&self, start: &str, end: &str) -> Result<ActivityStats> {
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

        let mut stmt = conn.prepare(&format!(
            "SELECT cat.id, cat.name, cat.color, COALESCE(SUM({DURATION}), 0.0), COUNT(*)
             {JOIN}
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
            "SELECT src.kind, src.name, cat.color,
                    COALESCE(SUM({DURATION}), 0.0), COUNT(*)
             {JOIN}
             WHERE s.end_time IS NOT NULL AND s.start_time >= ?1 AND s.start_time < ?2
             GROUP BY src.id
             ORDER BY 4 DESC
             LIMIT ?3"
        ))?;
        let apps = stmt
            .query_map(rusqlite::params![start, end, TOP_APPS_LIMIT], |row| {
                Ok(AppStat {
                    kind: row.get(0)?,
                    label: row.get(1)?,
                    category_color: row.get(2)?,
                    total_seconds: row.get(3)?,
                    session_count: row.get(4)?,
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
pub enum CategorySourceFilter {
    #[default]
    All,
    Manual,
    Ai,
}

#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub search: Option<String>,
    pub category: CategoryFilter,
    pub source: CategorySourceFilter,
    pub start: Option<String>,
    pub end: Option<String>,
}

impl SessionFilter {
    fn to_sql(
        &self,
        expanded_category_ids: Option<&[i64]>,
    ) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
        let mut clauses = vec!["1=1".to_string()];
        let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(term) = self
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            clauses.push(
                "(src.name LIKE ? ESCAPE '\\' OR a.title LIKE ? ESCAPE '\\' OR (src.key || a.path) LIKE ? ESCAPE '\\')"
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
            // Picking a parent means "this category and everything filed under it".
            CategoryFilter::Category(_) => {
                let ids = expanded_category_ids.unwrap_or_default();
                if ids.is_empty() {
                    clauses.push("0=1".to_string());
                } else {
                    clauses.push(format!(
                        "cat.id IN ({})",
                        std::iter::repeat_n("?", ids.len())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    binds.extend(
                        ids.iter()
                            .copied()
                            .map(|id| Box::new(id) as Box<dyn rusqlite::types::ToSql>),
                    );
                }
            }
        }

        match self.source {
            CategorySourceFilter::All => {}
            CategorySourceFilter::Manual => {
                clauses.push("a.category_id IS NOT NULL".to_string());
            }
            CategorySourceFilter::Ai => {
                clauses.push("a.category_id IS NULL AND l.category_id IS NOT NULL".to_string())
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

fn expanded_category_ids(
    conn: &rusqlite::Connection,
    filter: &SessionFilter,
) -> Result<Option<Vec<i64>>> {
    match filter.category {
        CategoryFilter::Category(id) => Ok(Some(load_category_tree(conn)?.descendants(id))),
        _ => Ok(None),
    }
}
