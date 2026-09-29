//! Read models behind the history list and the stats page. The `SELECT`/`JOIN`/aggregate shape
//! here is what Diesel's typed query builder cannot express cleanly (a computed `COALESCE` join
//! key, duration sums inside `GROUP BY … HAVING`, a `CASE` column), so these stay hand-written
//! SQL — [`crate::domain::history`] is what decides *which* rows to ask for.

use diesel::sql_types::{BigInt, Bool, Double, Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

use super::sql::{bind_all, in_clause, Param};

/// `sessions` joined out to its activity, source and resolved category. A manual pin
/// (`a.category_id`) always wins over a pending AI label (`l.category_id`, only when not
/// rejected).
const JOIN: &str = "FROM sessions s
     JOIN activities a ON a.id = s.activity_id
     JOIN sources src ON src.id = a.source_id
     LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
     LEFT JOIN categories cat ON cat.id = COALESCE(a.category_id, l.category_id)";

const TOP_APPS_LIMIT: i64 = 15;

/// Which category ids a session's resolved category must fall under.
pub enum CategoryScope {
    Any,
    Uncategorized,
    /// Already expanded to include every descendant; an empty list matches nothing.
    Ids(Vec<i64>),
}

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

    match &query.category {
        CategoryScope::Any => {}
        CategoryScope::Uncategorized => clauses.push("cat.id IS NULL".to_string()),
        CategoryScope::Ids(ids) => {
            let (clause, ids_binds) = in_clause("cat.id", ids, "0=1");
            clauses.push(clause);
            binds.extend(ids_binds);
        }
    }

    match query.source {
        SourceScope::Any => {}
        SourceScope::Manual => clauses.push("a.category_id IS NOT NULL".to_string()),
        SourceScope::Ai => clauses.push("a.category_id IS NULL AND l.category_id IS NOT NULL".to_string()),
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

pub fn count_sessions(conn: &mut SqliteConnection, query: &SessionQuery) -> QueryResult<i64> {
    let (where_sql, binds) = where_clause(query);
    let sql = format!("SELECT COUNT(*) AS count {JOIN} WHERE {where_sql}");
    let row = bind_all(sql_query(sql).into_boxed(), binds).get_result::<CountRow>(conn)?;
    Ok(row.count)
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

pub fn list_sessions(
    conn: &mut SqliteConnection,
    limit: i64,
    offset: i64,
    query: &SessionQuery,
) -> QueryResult<Vec<SessionListRow>> {
    let (where_sql, mut binds) = where_clause(query);
    let sql = format!(
        "SELECT s.id, s.activity_id, src.name AS app_name, src.kind AS source_kind,
                src.key AS source_key, a.path, a.title, s.start_time, s.end_time,
                cat.id AS category_id, cat.name AS category_name, cat.color AS category_color,
                a.category_id IS NOT NULL AS category_is_pinned,
                CASE
                  WHEN a.category_id IS NOT NULL THEN 'manual'
                  WHEN l.category_id IS NOT NULL THEN 'ai'
                  ELSE NULL
                END AS category_source
         {JOIN}
         WHERE {where_sql}
         ORDER BY s.start_time DESC
         LIMIT ? OFFSET ?"
    );
    binds.push(Param::I64(limit));
    binds.push(Param::I64(offset));
    bind_all(sql_query(sql).into_boxed(), binds).get_results(conn)
}

#[derive(QueryableByName)]
pub struct TotalsRow {
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
}

pub fn totals(conn: &mut SqliteConnection, start: &str, end: &str) -> QueryResult<TotalsRow> {
    const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
    sql_query(format!(
        "SELECT COALESCE(SUM({DURATION}), 0.0) AS total_seconds, COUNT(*) AS session_count
         FROM sessions s
         WHERE s.end_time IS NOT NULL AND s.start_time >= ? AND s.start_time < ?"
    ))
    .bind::<Text, _>(start)
    .bind::<Text, _>(end)
    .get_result(conn)
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

pub fn category_stats(conn: &mut SqliteConnection, start: &str, end: &str) -> QueryResult<Vec<CategoryStatRow>> {
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
    .get_results(conn)
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

pub fn app_stats(conn: &mut SqliteConnection, start: &str, end: &str) -> QueryResult<Vec<AppStatRow>> {
    const DURATION: &str = "(julianday(s.end_time) - julianday(s.start_time)) * 86400.0";
    sql_query(format!(
        "SELECT src.kind, src.name AS label, cat.color AS category_color,
                COALESCE(SUM({DURATION}), 0.0) AS total_seconds, COUNT(*) AS session_count
         {JOIN}
         WHERE s.end_time IS NOT NULL AND s.start_time >= ? AND s.start_time < ?
         GROUP BY src.id
         ORDER BY total_seconds DESC
         LIMIT ?"
    ))
    .bind::<Text, _>(start)
    .bind::<Text, _>(end)
    .bind::<BigInt, _>(TOP_APPS_LIMIT)
    .get_results(conn)
}

#[derive(QueryableByName)]
pub struct DailyStatRow {
    #[diesel(sql_type = Text)]
    pub day: String,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

pub fn daily_stats(conn: &mut SqliteConnection, start: &str, end: &str) -> QueryResult<Vec<DailyStatRow>> {
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
    .get_results(conn)
}
