//! Read models behind the history list and the stats page. The `SELECT`/`JOIN`/aggregate shape
//! here is what Diesel's typed query builder cannot express cleanly (a computed `COALESCE` join
//! key, duration sums inside `GROUP BY … HAVING`, a `CASE` column), so these stay hand-written
//! SQL — [`crate::domain::history`] is what decides *which* rows to ask for.

use diesel::sql_types::{BigInt, Bool, Double, Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

use crate::db::sql::{bind_all, count, Param};
use crate::domain::types::CategorySource;
use crate::tracking::domain::source_key::KeyKind;

use super::window::{clipped_end, clipped_start, overlaps, sum_seconds, WITH_WINDOW};
use super::{CategoryScope, Window};

/// `sessions s` joined out to its activity, source and resolved category. The category comes
/// from the `activity_category` view (`ac`), which decides the precedence (pin, then rule, then
/// an AI suggestion the user has not rejected); `ac.source` says which one it was.
const JOIN: &str = "JOIN activities a ON a.id = s.activity_id
     JOIN sources src ON src.id = a.source_id
     LEFT JOIN activity_category ac ON ac.activity_id = a.id
     LEFT JOIN categories cat ON cat.id = ac.category_id";

const TOP_APPS_LIMIT: i64 = 15;

pub enum SourceScope {
    Any,
    Manual,
    Rule,
    Ai,
}

pub struct SessionQuery {
    pub search: Option<String>,
    pub category: CategoryScope,
    pub source: SourceScope,
    /// Inclusive lower bound on `end_time` (a still-open session always passes).
    pub start: Option<String>,
    pub end: Option<String>,
}

fn where_clause(query: &SessionQuery) -> (String, Vec<Param>) {
    let mut clauses = vec!["1=1".to_string()];
    let mut binds = Vec::new();

    if let Some(term) = query.search.as_deref() {
        clauses.push(
            "(src.name LIKE ? ESCAPE '\\' OR a.title LIKE ? ESCAPE '\\' \
              OR (src.key || a.path) LIKE ? ESCAPE '\\')"
                .to_string(),
        );
        let pattern = like_pattern(term);
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
        SourceScope::Rule => clauses.push("ac.source = 'rule'".to_string()),
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
/// `%term%` for `LIKE … ESCAPE '\'`; the escape is doubled first so the ones added after stay intact.
/// `%term%` for `LIKE … ESCAPE '\\'`; the escape goes first so the ones added after stay intact.
fn like_pattern(term: &str) -> String {
    let escaped = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
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
    pub source_kind: KeyKind,
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
    pub category_source: Option<CategorySource>,
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
    pub kind: KeyKind,
    #[diesel(sql_type = Text)]
    pub label: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub category_color: Option<String>,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
}

/// The part of one session inside a window.
#[derive(QueryableByName)]
pub struct IntervalRow {
    #[diesel(sql_type = Text)]
    pub start_time: String,
    #[diesel(sql_type = Text)]
    pub end_time: String,
}

pub trait HistoryRepo {
    fn get_session_count(&mut self, query: &SessionQuery) -> QueryResult<i64>;
    fn get_sessions(&mut self, limit: i64, offset: i64, query: &SessionQuery) -> QueryResult<Vec<SessionListRow>>;
    fn get_totals(&mut self, window: &Window) -> QueryResult<TotalsRow>;
    fn get_category_stats(&mut self, window: &Window) -> QueryResult<Vec<CategoryStatRow>>;
    fn get_app_stats(&mut self, window: &Window) -> QueryResult<Vec<AppStatRow>>;
    fn get_intervals(&mut self, window: &Window) -> QueryResult<Vec<IntervalRow>>;
}

impl HistoryRepo for SqliteConnection {
    fn get_session_count(&mut self, query: &SessionQuery) -> QueryResult<i64> {
        let (where_sql, binds) = where_clause(query);
        let sql = format!("SELECT COUNT(*) AS count FROM sessions s {JOIN} WHERE {where_sql}");
        count(self, sql, binds)
    }

    fn get_sessions(&mut self, limit: i64, offset: i64, query: &SessionQuery) -> QueryResult<Vec<SessionListRow>> {
        let (where_sql, mut binds) = where_clause(query);
        let sql = format!(
            "SELECT s.id, s.activity_id, src.name AS app_name, src.kind AS source_kind,
                    src.key AS source_key, a.path, a.title, s.start_time, s.end_time,
                    cat.id AS category_id, cat.name AS category_name, cat.color AS category_color,
                    COALESCE(ac.source = 'manual', 0) AS category_is_pinned,
                    ac.source AS category_source
             FROM sessions s {JOIN}
             WHERE {where_sql}
             ORDER BY s.start_time DESC
             LIMIT ? OFFSET ?"
        );
        binds.push(Param::I64(limit));
        binds.push(Param::I64(offset));
        bind_all(sql_query(sql).into_boxed(), binds).get_results(self)
    }

    fn get_totals(&mut self, window: &Window) -> QueryResult<TotalsRow> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT {seconds} AS total_seconds, COUNT(*) AS session_count
             FROM w JOIN sessions s
             WHERE {overlaps}",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(sql).into_boxed(), window.binds()).get_result(self)
    }

    fn get_category_stats(&mut self, window: &Window) -> QueryResult<Vec<CategoryStatRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT cat.id AS category_id, cat.name, cat.color,
                    {seconds} AS total_seconds, COUNT(*) AS session_count
             FROM w JOIN sessions s {JOIN}
             WHERE {overlaps}
             GROUP BY cat.id
             ORDER BY total_seconds DESC",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(sql).into_boxed(), window.binds()).get_results(self)
    }

    fn get_app_stats(&mut self, window: &Window) -> QueryResult<Vec<AppStatRow>> {
        // A source can span several categories; it takes the colour of the one it spent the most
        // time in (ties broken by category id) rather than whichever row SQLite happens to keep.
        let sql = format!(
            "{WITH_WINDOW}
             SELECT src.kind, src.name AS label,
                    (SELECT c2.color
                     FROM sessions s2
                     JOIN activities a2 ON a2.id = s2.activity_id
                     JOIN activity_category ac2 ON ac2.activity_id = a2.id
                     JOIN categories c2 ON c2.id = ac2.category_id
                     WHERE a2.source_id = src.id AND {overlaps2}
                     GROUP BY c2.id
                     ORDER BY {seconds2} DESC, c2.id
                     LIMIT 1) AS category_color,
                    {seconds} AS total_seconds, COUNT(*) AS session_count
             FROM w JOIN sessions s {JOIN}
             WHERE {overlaps}
             GROUP BY src.id
             ORDER BY total_seconds DESC
             LIMIT ?",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
            seconds2 = sum_seconds("s2"),
            overlaps2 = overlaps("s2"),
        );
        let mut binds = window.binds();
        binds.push(Param::I64(TOP_APPS_LIMIT));
        bind_all(sql_query(sql).into_boxed(), binds).get_results(self)
    }

    fn get_intervals(&mut self, window: &Window) -> QueryResult<Vec<IntervalRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT {start} AS start_time, {end} AS end_time
             FROM w JOIN sessions s
             WHERE {overlaps}",
            start = clipped_start("s"),
            end = clipped_end("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(sql).into_boxed(), window.binds()).get_results(self)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use diesel::connection::SimpleConnection;

    use super::*;
    use crate::reporting::window::assert_seconds;
    use crate::shared::time::TimeRange;

    fn utc(at: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(at).unwrap().with_timezone(&Utc)
    }

    fn db_time(at: &str) -> String {
        crate::shared::time::to_db(utc(at))
    }

    /// One app with these sessions (`None` = still open).
    fn conn_with(sessions: &[(&str, Option<&str>)]) -> SqliteConnection {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO sources (id, kind, key, name) VALUES (1, 'exe', 'code', 'Code');
             INSERT INTO activities (id, source_id, path, title) VALUES (1, 1, '', 'main.rs');",
        )
        .unwrap();
        for (start, end) in sessions {
            let end = end.map_or("NULL".to_string(), |end| format!("'{}'", db_time(end)));
            conn.batch_execute(&format!(
                "INSERT INTO sessions (activity_id, start_time, end_time) VALUES (1, '{}', {end});",
                db_time(start)
            ))
            .unwrap();
        }
        conn
    }

    fn window(from: &str, to: &str, now: &str) -> Window {
        Window::new(
            &TimeRange {
                start: utc(from),
                end: utc(to),
            },
            utc(now),
        )
    }

    const FROM: &str = "2026-03-01T00:00:00Z";
    const TO: &str = "2026-03-02T00:00:00Z";
    const LATER: &str = "2026-03-05T00:00:00Z";

    #[test]
    fn a_search_term_matches_literally() {
        assert_eq!(like_pattern(r"50%_a\b"), r"%50\%\_a\\b%");
    }

    #[test]
    fn the_session_list_filters_by_category_source() {
        let mut conn = conn_with(&[("2026-03-01T10:00:00Z", None)]);
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000');
             INSERT INTO category_rules (id, category_id, kind, value) VALUES (1, 1, 'app', 'code');
             INSERT INTO rule_matches (activity_id, rule_id) VALUES (1, 1);",
        )
        .unwrap();
        let query = |source| SessionQuery {
            search: None,
            category: CategoryScope::Any,
            source,
            start: None,
            end: None,
        };
        let rows = conn.get_sessions(10, 0, &query(SourceScope::Rule)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_kind, KeyKind::Exe);
        assert_eq!(rows[0].category_source, Some(CategorySource::Rule));
        assert_eq!(conn.get_session_count(&query(SourceScope::Ai)).unwrap(), 0);
    }

    #[test]
    fn sessions_are_clipped_to_the_window() {
        let mut conn = conn_with(&[
            ("2026-02-28T23:00:00Z", Some("2026-03-01T01:00:00Z")),
            ("2026-03-01T10:00:00Z", Some("2026-03-01T10:30:00Z")),
            ("2026-03-01T23:30:00Z", Some("2026-03-02T02:00:00Z")),
            ("2026-02-28T10:00:00Z", Some("2026-02-28T11:00:00Z")),
            ("2026-03-02T00:00:00Z", Some("2026-03-02T01:00:00Z")),
        ]);
        let totals = conn.get_totals(&window(FROM, TO, LATER)).unwrap();
        assert_eq!(totals.session_count, 3);
        assert_seconds(totals.total_seconds, 7200.0);
    }

    #[test]
    fn an_open_session_runs_until_now() {
        let mut conn = conn_with(&[("2026-03-01T10:00:00Z", None)]);
        let totals = conn.get_totals(&window(FROM, TO, "2026-03-01T10:20:00Z")).unwrap();
        assert_eq!(totals.session_count, 1);
        assert_seconds(totals.total_seconds, 1200.0);

        let past = conn.get_totals(&window(FROM, TO, LATER)).unwrap();
        assert_seconds(past.total_seconds, 50400.0);
    }

    #[test]
    fn an_open_session_that_starts_after_the_window_is_left_out() {
        let mut conn = conn_with(&[("2026-03-03T10:00:00Z", None)]);
        let totals = conn.get_totals(&window(FROM, TO, LATER)).unwrap();
        assert_eq!(totals.session_count, 0);
        assert_eq!(totals.total_seconds, 0.0);
    }

    #[test]
    fn category_app_and_interval_reads_agree_with_the_totals() {
        let mut conn = conn_with(&[
            ("2026-02-28T23:00:00Z", Some("2026-03-01T01:00:00Z")),
            ("2026-03-01T23:30:00Z", None),
        ]);
        let window = window(FROM, TO, LATER);
        let total = conn.get_totals(&window).unwrap().total_seconds;
        let by_category: f64 = conn.get_category_stats(&window).unwrap().iter().map(|r| r.total_seconds).sum();
        let by_app: f64 = conn.get_app_stats(&window).unwrap().iter().map(|r| r.total_seconds).sum();
        assert_seconds(total, 5400.0);
        assert_seconds(by_category, total);
        assert_seconds(by_app, total);

        let intervals: Vec<(String, String)> = conn
            .get_intervals(&window)
            .unwrap()
            .into_iter()
            .map(|r| (r.start_time, r.end_time))
            .collect();
        assert_eq!(
            intervals,
            [
                (db_time(FROM), db_time("2026-03-01T01:00:00Z")),
                (db_time("2026-03-01T23:30:00Z"), db_time(TO)),
            ]
        );
    }
}
