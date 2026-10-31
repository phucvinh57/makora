//! Read models behind AI-label review: which activities still need a label, which ones the
//! user has already judged, and the paged/grouped review queue. Like [`super::history`], the
//! aggregate shape here stays hand-written SQL; [`crate::domain::labeling`] decides the filter
//! values.
//!
//! The review queue reads the `activity_category` view: an activity is pending exactly when its
//! resolved category is an AI suggestion (`ac.source = 'ai'`). The two "what has the user
//! judged" reads need the rejected labels the view leaves out, so they use `ai_labels` directly.

use diesel::sql_types::{BigInt, Double, Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

use crate::db::sql::{bind_all, count, in_clause, Param};

use super::window::{overlaps, sum_seconds, WITH_WINDOW};
use super::{CategoryScope, Window};

const ACTIVITY_URL: &str = "CASE WHEN src.kind = 'domain' THEN 'https://' || src.key || a.path END";

#[derive(QueryableByName)]
pub struct ActivityToLabelRow {
    #[diesel(sql_type = BigInt)]
    pub id: i64,
    #[diesel(sql_type = Text)]
    pub app_name: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub url: Option<String>,
    #[diesel(sql_type = Text)]
    pub title: String,
}

/// The `WHERE` clause for pending AI labels (over `activity_category ac`), and its binds. A scope
/// that matches nothing yields `0=1`, so the queries simply come back empty.
fn pending_fragment(category: &CategoryScope, activity_ids: Option<&[i64]>) -> (String, Vec<Param>) {
    let mut binds = Vec::new();

    let scope = match category.clause("ac.category_id") {
        Some((clause, scope_binds)) => {
            binds.extend(scope_binds);
            format!("AND {clause}")
        }
        None => String::new(),
    };

    let only = match activity_ids {
        Some(ids) => {
            let (clause, only_binds) = in_clause("a.id", ids, "0=1");
            binds.extend(only_binds);
            format!("AND {clause}")
        }
        None => String::new(),
    };

    (format!("ac.source = 'ai' {scope} {only}"), binds)
}

#[derive(QueryableByName)]
pub struct ReviewRow {
    #[diesel(sql_type = BigInt)]
    pub activity_id: i64,
    #[diesel(sql_type = Text)]
    pub app_name: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub url: Option<String>,
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = BigInt)]
    pub category_id: i64,
    #[diesel(sql_type = Text)]
    pub category_name: String,
    #[diesel(sql_type = Text)]
    pub category_color: String,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = BigInt)]
    pub session_count: i64,
}

#[derive(QueryableByName)]
pub struct ReviewGroupRow {
    #[diesel(sql_type = Text)]
    pub key_kind: String,
    #[diesel(sql_type = Text)]
    pub key: String,
    #[diesel(sql_type = Text)]
    pub app_name: String,
    #[diesel(sql_type = BigInt)]
    pub category_id: i64,
    #[diesel(sql_type = Text)]
    pub category_name: String,
    #[diesel(sql_type = Text)]
    pub category_color: String,
    #[diesel(sql_type = BigInt)]
    pub activity_count: i64,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
    #[diesel(sql_type = Text)]
    pub activity_ids: String,
}

#[derive(QueryableByName)]
pub struct FeedbackCountsRow {
    #[diesel(sql_type = BigInt)]
    pub corrected: i64,
    #[diesel(sql_type = BigInt)]
    pub confirmed: i64,
    #[diesel(sql_type = BigInt)]
    pub pinned: i64,
    #[diesel(sql_type = BigInt)]
    pub rejected: i64,
    #[diesel(sql_type = BigInt)]
    pub pending_review: i64,
}

#[derive(QueryableByName)]
pub struct FeedbackSecondsRow {
    #[diesel(sql_type = Double)]
    pub pending_seconds: f64,
    #[diesel(sql_type = Double)]
    pub reviewed_seconds: f64,
}

pub trait ReviewRepo {
    fn get_activities_to_label(&mut self, recheck: bool) -> QueryResult<Vec<ActivityToLabelRow>>;
    fn get_review_count(&mut self, category: &CategoryScope, activity_ids: Option<&[i64]>) -> QueryResult<i64>;
    fn get_review_page(
        &mut self,
        limit: i64,
        offset: i64,
        category: &CategoryScope,
        activity_ids: Option<&[i64]>,
        window: &Window,
    ) -> QueryResult<Vec<ReviewRow>>;
    fn get_review_group_count(&mut self, category: &CategoryScope) -> QueryResult<i64>;
    fn get_review_groups(
        &mut self,
        limit: i64,
        offset: i64,
        category: &CategoryScope,
        window: &Window,
    ) -> QueryResult<Vec<ReviewGroupRow>>;
    fn get_feedback_counts(&mut self) -> QueryResult<FeedbackCountsRow>;
    /// Time, not rows: a few long activities matter more than many short ones.
    fn get_feedback_seconds(&mut self, window: &Window) -> QueryResult<FeedbackSecondsRow>;
}

impl ReviewRepo for SqliteConnection {
    fn get_activities_to_label(&mut self, recheck: bool) -> QueryResult<Vec<ActivityToLabelRow>> {
        // A rule has already decided an activity it matches, so the model has nothing to add.
        let wanted = if recheck {
            "EXISTS (SELECT 1 FROM ai_labels l WHERE l.activity_id = a.id AND l.rejected = 0)"
        } else {
            "NOT EXISTS (SELECT 1 FROM ai_labels l WHERE l.activity_id = a.id)"
        };
        sql_query(format!(
            "SELECT a.id, src.name AS app_name, {ACTIVITY_URL} AS url, a.title
             FROM activities a
             JOIN sources src ON src.id = a.source_id
             WHERE a.category_id IS NULL AND {wanted}
               AND NOT EXISTS (SELECT 1 FROM rule_matches m WHERE m.activity_id = a.id)
             ORDER BY (SELECT MAX(start_time) FROM sessions WHERE activity_id = a.id) DESC"
        ))
        .get_results(self)
    }

    fn get_review_count(&mut self, category: &CategoryScope, activity_ids: Option<&[i64]>) -> QueryResult<i64> {
        let (pending, binds) = pending_fragment(category, activity_ids);
        let sql = format!(
            "SELECT COUNT(*) AS count FROM activities a
             JOIN activity_category ac ON ac.activity_id = a.id
             WHERE {pending}"
        );
        count(self, sql, binds)
    }

    fn get_review_page(
        &mut self,
        limit: i64,
        offset: i64,
        category: &CategoryScope,
        activity_ids: Option<&[i64]>,
        window: &Window,
    ) -> QueryResult<Vec<ReviewRow>> {
        let (pending, pending_binds) = pending_fragment(category, activity_ids);
        let mut binds = window.binds();
        binds.extend(pending_binds);
        binds.push(Param::I64(limit));
        binds.push(Param::I64(offset));
        let rows_sql = format!(
            "{WITH_WINDOW}
             SELECT a.id AS activity_id, src.name AS app_name, {ACTIVITY_URL} AS url, a.title,
                    cat.id AS category_id, cat.name AS category_name, cat.color AS category_color,
                    {seconds} AS total_seconds, COUNT(s.id) AS session_count
             FROM w
             JOIN activities a
             JOIN sources src ON src.id = a.source_id
             JOIN activity_category ac ON ac.activity_id = a.id
             JOIN categories cat ON cat.id = ac.category_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             WHERE {pending}
             GROUP BY a.id
             ORDER BY 8 DESC, a.id
             LIMIT ? OFFSET ?",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(rows_sql).into_boxed(), binds).get_results(self)
    }

    fn get_review_group_count(&mut self, category: &CategoryScope) -> QueryResult<i64> {
        let (pending, binds) = pending_fragment(category, None);
        let sql = format!(
            "SELECT COUNT(*) AS count FROM (
               SELECT 1 FROM activities a
               JOIN sources src ON src.id = a.source_id
               JOIN activity_category ac ON ac.activity_id = a.id
               WHERE {pending}
               GROUP BY src.id, ac.category_id)"
        );
        count(self, sql, binds)
    }

    fn get_review_groups(
        &mut self,
        limit: i64,
        offset: i64,
        category: &CategoryScope,
        window: &Window,
    ) -> QueryResult<Vec<ReviewGroupRow>> {
        // A group picks its own members, so unlike `review_page` this ignores any activity filter.
        let (pending, pending_binds) = pending_fragment(category, None);
        let grouped = format!(
            "SELECT src.kind AS key_kind, src.key, MIN(src.name) AS app_name, cat.id AS category_id,
                    cat.name AS category_name, cat.color AS category_color,
                    COUNT(DISTINCT a.id) AS activity_count, {seconds} AS total_seconds,
                    GROUP_CONCAT(DISTINCT a.id) AS activity_ids
             FROM w
             JOIN activities a
             JOIN sources src ON src.id = a.source_id
             JOIN activity_category ac ON ac.activity_id = a.id
             JOIN categories cat ON cat.id = ac.category_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             WHERE {pending}
             GROUP BY src.id, cat.id",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        let mut binds = window.binds();
        binds.extend(pending_binds);
        binds.push(Param::I64(limit));
        binds.push(Param::I64(offset));
        let rows_sql = format!("{WITH_WINDOW} {grouped} ORDER BY 8 DESC, key_kind, key LIMIT ? OFFSET ?");
        bind_all(sql_query(rows_sql).into_boxed(), binds).get_results(self)
    }

    fn get_feedback_counts(&mut self) -> QueryResult<FeedbackCountsRow> {
        sql_query(
            "SELECT
               COALESCE(SUM(a.category_id IS NOT NULL AND (
                   (l.rejected = 0 AND l.category_id != a.category_id)
                   OR (l.activity_id IS NULL AND rej.activity_id IS NOT NULL))), 0) AS corrected,
               COALESCE(SUM(a.category_id IS NOT NULL AND l.rejected = 0
                            AND l.category_id = a.category_id), 0) AS confirmed,
               COALESCE(SUM(a.category_id IS NOT NULL AND l.activity_id IS NULL
                            AND rej.activity_id IS NULL), 0) AS pinned,
               COALESCE(SUM(a.category_id IS NULL AND rej.activity_id IS NOT NULL
                            AND m.activity_id IS NULL), 0) AS rejected,
               COALESCE(SUM(a.category_id IS NULL AND l.activity_id IS NOT NULL
                            AND m.activity_id IS NULL), 0) AS pending_review
             FROM activities a
             LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
             LEFT JOIN ai_labels rej ON rej.activity_id = a.id AND rej.rejected = 1
             LEFT JOIN rule_matches m ON m.activity_id = a.id",
        )
        .get_result(self)
    }

    fn get_feedback_seconds(&mut self, window: &Window) -> QueryResult<FeedbackSecondsRow> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT COALESCE(SUM(CASE WHEN pending THEN secs END), 0.0) AS pending_seconds,
                    COALESCE(SUM(CASE WHEN reviewed THEN secs END), 0.0) AS reviewed_seconds
             FROM (
               SELECT (a.category_id IS NULL AND l.activity_id IS NOT NULL
                       AND m.activity_id IS NULL) AS pending,
                      ((l.activity_id IS NOT NULL AND a.category_id IS NOT NULL)
                       OR (rej.activity_id IS NOT NULL
                           AND (a.category_id IS NOT NULL OR m.activity_id IS NULL))) AS reviewed,
                      (SELECT {seconds} FROM sessions s
                       WHERE s.activity_id = a.id AND {overlaps}) AS secs
               FROM w
               JOIN activities a
               LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
               LEFT JOIN ai_labels rej ON rej.activity_id = a.id AND rej.rejected = 1
               LEFT JOIN rule_matches m ON m.activity_id = a.id)",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        bind_all(sql_query(sql).into_boxed(), window.binds()).get_result(self)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use diesel::connection::SimpleConnection;

    use super::*;

    fn utc(at: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(at).unwrap().with_timezone(&Utc)
    }

    /// Activities 1 and 2 carry a pending AI label; 1 has a closed half hour and a session open
    /// since 11:00, 2 has ten minutes.
    fn conn() -> SqliteConnection {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000');
             INSERT INTO sources (id, kind, key, name) VALUES (1, 'exe', 'code', 'Code');
             INSERT INTO activities (id, source_id, path, title) VALUES (1, 1, '', 'a'), (2, 1, '', 'b');
             INSERT INTO ai_labels (activity_id, category_id) VALUES (1, 1), (2, 1);
             INSERT INTO sessions (activity_id, start_time, end_time) VALUES
               (1, '2026-03-01T10:00:00+00:00', '2026-03-01T10:30:00+00:00'),
               (1, '2026-03-01T11:00:00+00:00', NULL),
               (2, '2026-03-01T09:00:00+00:00', '2026-03-01T09:10:00+00:00');",
        )
        .unwrap();
        conn
    }

    fn now() -> Window {
        Window::until(utc("2026-03-01T11:10:00Z"))
    }

    #[test]
    fn the_review_queue_counts_open_sessions_up_to_now() {
        let mut conn = conn();
        let rows = conn.get_review_page(10, 0, &CategoryScope::Any, None, &now()).unwrap();
        let totals: Vec<(i64, i64, i64)> = rows
            .iter()
            .map(|r| (r.activity_id, r.total_seconds.round() as i64, r.session_count))
            .collect();
        assert_eq!(totals, [(1, 2400, 2), (2, 600, 1)]);
        assert_eq!(conn.get_review_count(&CategoryScope::Any, None).unwrap(), 2);

        let page = conn.get_review_page(1, 1, &CategoryScope::Any, None, &now()).unwrap();
        assert_eq!(page.iter().map(|r| r.activity_id).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn review_groups_fold_a_source_and_category_into_one_row() {
        let mut conn = conn();
        let groups = conn.get_review_groups(10, 0, &CategoryScope::Any, &now()).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].activity_count, 2);
        assert_eq!(groups[0].total_seconds.round() as i64, 3000);
        assert_eq!(conn.get_review_group_count(&CategoryScope::Any).unwrap(), 1);
    }

    #[test]
    fn a_rejected_label_on_a_rule_matched_activity_is_not_counted_as_rejected() {
        let mut conn = conn();
        conn.batch_execute(
            "INSERT INTO activities (id, source_id, path, title) VALUES (3, 1, '', 'c'), (4, 1, '', 'd');
             INSERT INTO ai_labels (activity_id, category_id, rejected) VALUES (3, 1, 1), (4, 1, 1);
             INSERT INTO category_rules (id, category_id, kind, value) VALUES (1, 1, 'app', 'code');
             INSERT INTO rule_matches (activity_id, rule_id) VALUES (4, 1);
             INSERT INTO sessions (activity_id, start_time, end_time) VALUES
               (3, '2026-03-01T08:00:00+00:00', '2026-03-01T08:01:00+00:00'),
               (4, '2026-03-01T08:00:00+00:00', '2026-03-01T08:02:00+00:00');",
        )
        .unwrap();
        assert_eq!(conn.get_feedback_counts().unwrap().rejected, 1);
        assert_eq!(conn.get_feedback_seconds(&now()).unwrap().reviewed_seconds.round() as i64, 60);
    }

    #[test]
    fn feedback_seconds_count_open_sessions_up_to_now() {
        let mut conn = conn();
        let seconds = conn.get_feedback_seconds(&now()).unwrap();
        assert_eq!(seconds.pending_seconds.round() as i64, 3000);
        assert_eq!(seconds.reviewed_seconds, 0.0);
    }
}
