use diesel::sql_types::{BigInt, Bool, Double, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

use crate::db::sql::{bind_all, Param};

use super::window::{overlaps, sum_seconds, WITH_WINDOW};
use super::Window;

const SOURCE_SUGGESTION_LIMIT: i64 = 1000;

#[derive(QueryableByName)]
pub struct RuleStatRow {
    #[diesel(sql_type = BigInt)]
    pub rule_id: i64,
    #[diesel(sql_type = BigInt)]
    pub activity_count: i64,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

#[derive(QueryableByName)]
pub struct SourceSuggestionRow {
    #[diesel(sql_type = Text)]
    pub kind: String,
    #[diesel(sql_type = Text)]
    pub key: String,
    #[diesel(sql_type = Text)]
    pub name: String,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

#[derive(QueryableByName)]
pub struct PathSecondsRow {
    #[diesel(sql_type = Text)]
    pub path: String,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

#[derive(QueryableByName)]
pub struct ActivityPreviewRow {
    #[diesel(sql_type = Text)]
    pub source_kind: String,
    #[diesel(sql_type = Text)]
    pub source_key: String,
    #[diesel(sql_type = Text)]
    pub source_name: String,
    #[diesel(sql_type = Text)]
    pub path: String,
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = Bool)]
    pub pinned: bool,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

pub trait RulesReportRepo {
    /// For each rule that currently decides some activity: how many, and their tracked time.
    /// Activities with a pin are not counted, since the rule does not decide them.
    fn get_rule_stats(&mut self, window: &Window) -> QueryResult<Vec<RuleStatRow>>;
    fn get_source_suggestions(&mut self, window: &Window) -> QueryResult<Vec<SourceSuggestionRow>>;
    fn get_paths_of(&mut self, domain: &str, window: &Window) -> QueryResult<Vec<PathSecondsRow>>;
    fn get_activities_for_preview(&mut self, window: &Window) -> QueryResult<Vec<ActivityPreviewRow>>;
}

impl RulesReportRepo for SqliteConnection {
    fn get_rule_stats(&mut self, window: &Window) -> QueryResult<Vec<RuleStatRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT m.rule_id, COUNT(DISTINCT m.activity_id) AS activity_count, {seconds} AS total_seconds
             FROM w
             JOIN rule_matches m
             JOIN activities a ON a.id = m.activity_id AND a.category_id IS NULL
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             GROUP BY m.rule_id",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(sql).into_boxed(), window.binds()).get_results(self)
    }

    fn get_source_suggestions(&mut self, window: &Window) -> QueryResult<Vec<SourceSuggestionRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT src.kind, src.key, src.name, {seconds} AS total_seconds
             FROM w
             JOIN sources src
             LEFT JOIN activities a ON a.source_id = src.id
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             GROUP BY src.id
             ORDER BY total_seconds DESC, src.key
             LIMIT {SOURCE_SUGGESTION_LIMIT}",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(sql).into_boxed(), window.binds()).get_results(self)
    }

    fn get_paths_of(&mut self, domain: &str, window: &Window) -> QueryResult<Vec<PathSecondsRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT a.path, {seconds} AS total_seconds
             FROM w
             JOIN activities a
             JOIN sources src ON src.id = a.source_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             WHERE src.kind = 'domain' AND src.key = ? AND a.path != ''
             GROUP BY a.id",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        let mut binds = window.binds();
        binds.push(Param::Text(domain.to_string()));
        bind_all(sql_query(sql).into_boxed(), binds).get_results(self)
    }

    fn get_activities_for_preview(&mut self, window: &Window) -> QueryResult<Vec<ActivityPreviewRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT src.kind AS source_kind, src.key AS source_key, src.name AS source_name,
                    a.path, a.title, a.category_id IS NOT NULL AS pinned, {seconds} AS total_seconds
             FROM w
             JOIN activities a
             JOIN sources src ON src.id = a.source_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             GROUP BY a.id",
            seconds = sum_seconds("s"),
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

    #[test]
    fn rule_and_source_totals_count_open_sessions_up_to_now() {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000');
             INSERT INTO sources (id, kind, key, name) VALUES (1, 'domain', 'github.com', 'github.com');
             INSERT INTO activities (id, source_id, path, title) VALUES (1, 1, '/acme', 'a');
             INSERT INTO category_rules (id, category_id, kind, value) VALUES (1, 1, 'site', 'github.com');
             INSERT INTO rule_matches (activity_id, rule_id) VALUES (1, 1);
             INSERT INTO sessions (activity_id, start_time, end_time) VALUES
               (1, '2026-03-01T10:00:00+00:00', '2026-03-01T10:30:00+00:00'),
               (1, '2026-03-01T11:00:00+00:00', NULL);",
        )
        .unwrap();
        let now = DateTime::parse_from_rfc3339("2026-03-01T11:10:00Z").unwrap().with_timezone(&Utc);
        let window = Window::until(now);

        let rules = conn.get_rule_stats(&window).unwrap();
        assert_eq!((rules[0].rule_id, rules[0].activity_count), (1, 1));
        assert_eq!(rules[0].total_seconds.round() as i64, 2400);
        assert_eq!(conn.get_source_suggestions(&window).unwrap()[0].total_seconds.round() as i64, 2400);
        assert_eq!(conn.get_paths_of("github.com", &window).unwrap()[0].total_seconds.round() as i64, 2400);
        assert_eq!(conn.get_activities_for_preview(&window).unwrap()[0].total_seconds.round() as i64, 2400);
    }
}
