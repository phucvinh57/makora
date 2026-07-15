use anyhow::Result;
use serde::Serialize;

use super::sqlite::SharedConnection;

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
    pub category_is_override: bool,
}
pub struct HistoryReader {
    conn: SharedConnection,
}

impl HistoryReader {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    /// Total number of activity rows matching `filter`, for pagination.
    pub fn count_sessions(&self, filter: &SessionFilter) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let (where_sql, binds) = filter.to_sql();
        let sql = format!(
            "SELECT COUNT(*)
             FROM sessions s
             LEFT JOIN category_overrides ov
                    ON ov.key_kind = s.category_key_kind AND ov.pattern = s.category_key
             LEFT JOIN category_rules r
                    ON r.key_kind = s.category_key_kind AND r.pattern = s.category_key
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(ov.category_id, r.category_id)
             WHERE {where_sql}"
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        Ok(conn.query_row(&sql, param_refs.as_slice(), |row| row.get(0))?)
    }

    /// A page of activity rows (app focus and browser-tab rows alike)
    /// matching `filter`, newest first. Category is resolved at read time —
    /// an override wins over a rule via `COALESCE` — rather than stored on
    /// the row, so editing a rule or override re-categorizes historical
    /// sessions immediately instead of only affecting future ones.
    pub fn list_sessions(&self, limit: i64, offset: i64, filter: &SessionFilter) -> Result<Vec<SessionRow>> {
        let conn = self.conn.lock().unwrap();
        let (where_sql, mut binds) = filter.to_sql();
        let sql = format!(
            "SELECT s.id, s.app_name, s.exe_path, s.window_title, s.url, s.start_time, s.end_time,
                    cat.id, cat.name, cat.color, ov.category_id IS NOT NULL
             FROM sessions s
             LEFT JOIN category_overrides ov
                    ON ov.key_kind = s.category_key_kind AND ov.pattern = s.category_key
             LEFT JOIN category_rules r
                    ON r.key_kind = s.category_key_kind AND r.pattern = s.category_key
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(ov.category_id, r.category_id)
             WHERE {where_sql}
             ORDER BY s.start_time DESC
             LIMIT ? OFFSET ?"
        );
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = binds.iter().map(|b| b.as_ref()).collect();

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
                    category_is_override: row.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

/// How the category filter narrows `list_sessions`/`count_sessions`: show
/// everything, only rows with no resolved category, or only rows resolving
/// to one specific category.
#[derive(Debug, Clone, Default)]
pub enum CategoryFilter {
    #[default]
    All,
    Uncategorized,
    Category(i64),
}

/// The search box and category picker on the history page, combined into a
/// single filter so `list_sessions`/`count_sessions` stay in lockstep (a
/// page of rows and its total must be counted under the same conditions).
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    /// Substring matched (case-insensitively) against app name, window
    /// title, and URL.
    pub search: Option<String>,
    pub category: CategoryFilter,
}

impl SessionFilter {
    /// Builds a `WHERE`-clause body (without the `WHERE` keyword, always
    /// non-empty thanks to the `1=1` filler) and its bound parameters, in
    /// the order they appear in the SQL.
    fn to_sql(&self) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
        let mut clauses = vec!["1=1".to_string()];
        let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(term) = self.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
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

        (clauses.join(" AND "), binds)
    }
}
