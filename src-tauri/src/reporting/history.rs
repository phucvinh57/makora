//! Read models behind the history list and the stats page. The `SELECT`/`JOIN`/aggregate shape
//! here is what Diesel's typed query builder cannot express cleanly (a computed `COALESCE` join
//! key, duration sums inside `GROUP BY … HAVING`, a `CASE` column), so these stay hand-written
//! SQL — [`crate::domain::history`] is what decides *which* rows to ask for.

use diesel::sql_types::{BigInt, Bool, Double, Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

use crate::db::sql::{bind_all, Param};

use super::CategoryScope;

/// `sessions` joined out to its activity, source and resolved category. The category comes from
/// the `activity_category` view (`ac`): a manual pin wins over an AI suggestion the user has not
/// rejected, and `ac.source` says which of the two it was.
const JOIN: &str = "FROM sessions s
     JOIN activities a ON a.id = s.activity_id
     JOIN sources src ON src.id = a.source_id
     LEFT JOIN activity_category ac ON ac.activity_id = a.id
     LEFT JOIN categories cat ON cat.id = ac.category_id";

const TOP_APPS_LIMIT: i64 = 15;

pub enum SourceScope {
    Any,
    Manual,
    Ai,
}

pub struct SessionQuery {
    /// Trimmed, non-empty search term; matched against the source name, title and key/path.
    pub search: Option<String>,
    pub category: CategoryScope,
    pub source: SourceScope,
    /// Inclusive lower bound on `end_time` (a still-open session always passes).
    pub start: Option<String>,
    /// Exclusive upper bound on `start_time`.
    pub end: Option<String>,
}

/// Builds the `WHERE` clause and its bind values for [`SessionQuery`], in the order the `?`
/// placeholders appear.
fn where_clause(query: &SessionQuery) -> (String, Vec<Param>) {
    let mut clauses = vec!["1=1".to_string()];
    let mut binds = Vec::new();

    if let Some(term) = query.search.as_deref() {
        clauses.push(
            "(src.name LIKE ? ESCAPE '\\' OR a.title LIKE ? ESCAPE '\\' \
              OR (src.key || a.path) LIKE ? ESCAPE '\\')"
                .to_string(),
        );
        let pattern = format!("%{}%", term.replace('%', "\\%").replace('_', "\\_"));
        for _ in 0..3 {
            binds.push(Param::Text(pattern.clone()));
        }
    }

    if let Some((clause, category_binds)) = query.category.clause("cat.id") {
        clauses.push(clause);
        binds.extend(category_binds);
    }

    match query.source {
        SourceScope::Any => {}
        SourceScope::Manual => clauses.push("ac.source = 'manual'".to_string()),
        SourceScope::Ai => clauses.push("ac.source = 'ai'".to_string()),
    }

    if let Some(start) = &query.start {
        clauses.push("(s.end_time IS NULL OR s.end_time > ?)".to_string());
        binds.push(Param::Text(start.clone()));
    }
    if let Some(end) = &query.end {
        clauses.push("s.start_time < ?".to_string());
        binds.push(Param::Text(end.clone()));
    }

    (clauses.join(" AND "), binds)
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[derive(QueryableByName)]
pub struct SessionListRow {
    #[diesel(sql_type = BigInt)]
    pub id: i64,
    #[diesel(sql_type = BigInt)]
    pub activity_id: i64,
    #[diesel(sql_type = Text)]
    pub app_name: String,
    #[diesel(sql_type = Text)]
    pub source_kind: String,
    #[diesel(sql_type = Text)]
    pub source_key: String,
    #[diesel(sql_type = Text)]
    pub path: String,
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = Text)]
    pub start_time: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub end_time: Option<String>,
    #[diesel(sql_type = Nullable<BigInt>)]
    pub category_id: Option<i64>,
    #[diesel(sql_type = Nullable<Text>)]
    pub category_name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    pub category_color: Option<String>,
    #[diesel(sql_type = Bool)]
    pub category_is_pinned: bool,
    #[diesel(sql_type = Nullable<Text>)]
    pub category_source: Option<String>,
}

#[derive(QueryableByName)]
pub struct TotalsRow {
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
}

#[derive(QueryableByName)]
pub struct CategoryStatRow {
    #[diesel(sql_type = Nullable<BigInt>)]
    pub category_id: Option<i64>,
    #[diesel(sql_type = Nullable<Text>)]
    pub name: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    pub color: Option<String>,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
}

#[derive(QueryableByName)]
pub struct AppStatRow {
    #[diesel(sql_type = Text)]
    pub kind: String,
    #[diesel(sql_type = Text)]
    pub label: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub category_color: Option<String>,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
}

#[derive(QueryableByName)]
pub struct DailyStatRow {
    #[diesel(sql_type = Text)]
    pub day: String,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

pub trait HistoryRepo {
    fn get_session_count(&mut self, query: &SessionQuery) -> QueryResult<i64>;
    fn get_sessions(&mut self, limit: i64, offset: i64, query: &SessionQuery) -> QueryResult<Vec<SessionListRow>>;
    fn get_totals(&mut self, start: &str, end: &str) -> QueryResult<TotalsRow>;
    fn get_category_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<CategoryStatRow>>;
    fn get_app_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<AppStatRow>>;
    fn get_daily_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<DailyStatRow>>;
}

impl HistoryRepo for SqliteConnection {
    fn get_session_count(&mut self, query: &SessionQuery) -> QueryResult<i64> {
        let (where_sql, binds) = where_clause(query);
        let sql = format!("SELECT COUNT(*) AS count {JOIN} WHERE {where_sql}");
        let row = bind_all(sql_query(sql).into_boxed(), binds).get_result::<CountRow>(self)?;
        Ok(row.count)
    }

    fn get_sessions(&mut self, limit: i64, offset: i64, query: &SessionQuery) -> QueryResult<Vec<SessionListRow>> {
        let (where_sql, mut binds) = where_clause(query);
        let sql = format!(
            "SELECT s.id, s.activity_id, src.name AS app_name, src.kind AS source_kind,
                    src.key AS source_key, a.path, a.title, s.start_time, s.end_time,
                    cat.id AS category_id, cat.name AS category_name, cat.color AS category_color,
                    COALESCE(ac.source = 'manual', 0) AS category_is_pinned,
                    ac.source AS category_source
             {JOIN}
             WHERE {where_sql}
             ORDER BY s.start_time DESC
             LIMIT ? OFFSET ?"
        );
        binds.push(Param::I64(limit));
        binds.push(Param::I64(offset));
        bind_all(sql_query(sql).into_boxed(), binds).get_results(self)
    }

    fn get_totals(&mut self, start: &str, end: &str) -> QueryResult<TotalsRow> {
        const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
        sql_query(format!(
            "SELECT COALESCE(SUM({DURATION}), 0.0) AS total_seconds, COUNT(*) AS session_count
             FROM sessions s
             WHERE s.end_time IS NOT NULL AND s.start_time >= ? AND s.start_time < ?"
        ))
        .bind::<Text, _>(start)
        .bind::<Text, _>(end)
        .get_result(self)
    }

    fn get_category_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<CategoryStatRow>> {
        const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
        sql_query(format!(
            "SELECT cat.id AS category_id, cat.name, cat.color,
                    COALESCE(SUM({DURATION}), 0.0) AS total_seconds, COUNT(*) AS session_count
             {JOIN}
             WHERE s.end_time IS NOT NULL AND s.start_time >= ? AND s.start_time < ?
             GROUP BY cat.id
             ORDER BY total_seconds DESC"
        ))
        .bind::<Text, _>(start)
        .bind::<Text, _>(end)
        .get_results(self)
    }

    fn get_app_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<AppStatRow>> {
        const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
        // A source can span several categories; it takes the colour of the one it spent the most
        // time in (ties broken by category id) rather than whichever row SQLite happens to keep.
        sql_query(format!(
            "SELECT src.kind, src.name AS label,
                    (SELECT c2.color
                     FROM sessions s2
                     JOIN activities a2 ON a2.id = s2.activity_id
                     JOIN activity_category ac2 ON ac2.activity_id = a2.id
                     JOIN categories c2 ON c2.id = ac2.category_id
                     WHERE a2.source_id = src.id AND s2.end_time IS NOT NULL
                       AND s2.start_time >= ? AND s2.start_time < ?
                     GROUP BY c2.id
                     ORDER BY SUM((julianday(s2.end_time) - julianday(s2.start_time)) * 86400.0) DESC, c2.id
                     LIMIT 1) AS category_color,
                    COALESCE(SUM({DURATION}), 0.0) AS total_seconds, COUNT(*) AS session_count
             {JOIN}
             WHERE s.end_time IS NOT NULL AND s.start_time >= ? AND s.start_time < ?
             GROUP BY src.id
             ORDER BY total_seconds DESC
             LIMIT ?"
        ))
        .bind::<Text, _>(start)
        .bind::<Text, _>(end)
        .bind::<Text, _>(start)
        .bind::<Text, _>(end)
        .bind::<BigInt, _>(TOP_APPS_LIMIT)
        .get_results(self)
    }

    fn get_daily_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<DailyStatRow>> {
        const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
        sql_query(format!(
            "SELECT date(s.start_time, 'localtime') AS day, COALESCE(SUM({DURATION}), 0.0) AS total_seconds
             FROM sessions s
             WHERE s.end_time IS NOT NULL AND s.start_time >= ? AND s.start_time < ?
             GROUP BY day
             ORDER BY day ASC"
        ))
        .bind::<Text, _>(start)
        .bind::<Text, _>(end)
        .get_results(self)
    }
}
