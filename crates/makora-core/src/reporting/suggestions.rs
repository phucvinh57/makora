//! Read models behind taxonomy suggestions: the activities the model found no category for and
//! that are still unfiled, with their embeddings and tracked time; how the user answered each
//! category's AI labels; and where they moved them.

use diesel::sql_types::{BigInt, Binary, Double, Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

use crate::db::sql::{bind_all, count, Param};
use crate::tracking::domain::source_key::KeyKind;

use super::window::{overlaps, sum_seconds, WITH_WINDOW};
use super::Window;

/// An `ai_unlabeled u` row whose activity `a` is still unfiled: no pin, no rule, no active AI
/// label.
const STILL_UNFILED: &str = "a.category_id IS NULL
    AND NOT EXISTS (SELECT 1 FROM rule_matches m WHERE m.activity_id = a.id)
    AND NOT EXISTS (SELECT 1 FROM ai_labels l WHERE l.activity_id = a.id AND l.rejected = 0)";

#[derive(QueryableByName)]
pub struct UnlabeledRow {
    #[diesel(sql_type = BigInt)]
    pub id: i64,
    #[diesel(sql_type = Text)]
    pub app_name: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub url: Option<String>,
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = Text)]
    pub source_kind: KeyKind,
    #[diesel(sql_type = Text)]
    pub source_key: String,
    #[diesel(sql_type = Text)]
    pub path: String,
    #[diesel(sql_type = Binary)]
    pub vector: Vec<u8>,
    #[diesel(sql_type = Double)]
    pub total_seconds: f64,
}

/// How the user answered the AI labels of one category.
#[derive(QueryableByName, Debug, PartialEq, Eq)]
pub struct CategoryFeedbackRow {
    #[diesel(sql_type = BigInt)]
    pub category_id: i64,
    #[diesel(sql_type = BigInt)]
    pub confirmed: i64,
    #[diesel(sql_type = BigInt)]
    pub corrected: i64,
    #[diesel(sql_type = BigInt)]
    pub rejected: i64,
}

/// How many AI labels in `from_id` the user pinned to `into_id`.
#[derive(QueryableByName, Debug, PartialEq, Eq)]
pub struct CorrectionPairRow {
    #[diesel(sql_type = BigInt)]
    pub from_id: i64,
    #[diesel(sql_type = BigInt)]
    pub into_id: i64,
    #[diesel(sql_type = BigInt)]
    pub count: i64,
}

#[derive(QueryableByName, Debug)]
pub struct CategorySecondsRow {
    #[diesel(sql_type = BigInt)]
    pub category_id: i64,
    #[diesel(sql_type = Double)]
    pub seconds: f64,
}

/// An activity as the model sees it, and, for one moved out of a category, the category the user
/// put it in (`None`: left uncategorized).
#[derive(QueryableByName)]
pub struct SampleRow {
    #[diesel(sql_type = Text)]
    pub app_name: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub url: Option<String>,
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = Text)]
    pub source_kind: KeyKind,
    #[diesel(sql_type = Text)]
    pub source_key: String,
    #[diesel(sql_type = Text)]
    pub path: String,
    #[diesel(sql_type = Nullable<BigInt>)]
    pub moved_to: Option<i64>,
}

const SAMPLE_COLUMNS: &str = "src.name AS app_name,
    CASE WHEN src.kind = 'domain' THEN 'https://' || src.key || a.path END AS url,
    a.title, src.kind AS source_kind, src.key AS source_key, a.path";

/// The AI label `l` of activity `a` was moved out of its category: the user pinned `a` elsewhere,
/// or rejected the label and left `a` to no rule.
const MOVED_OUT: &str = "((a.category_id IS NOT NULL AND a.category_id != l.category_id)
    OR (l.rejected = 1 AND a.category_id IS NULL
        AND NOT EXISTS (SELECT 1 FROM rule_matches m WHERE m.activity_id = a.id)))";

/// Activity `a` has a session that started at or after the bound `since`.
const SEEN_SINCE: &str = "EXISTS (SELECT 1 FROM sessions s WHERE s.activity_id = a.id AND s.start_time >= ?)";

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

pub trait SuggestionsReportRepo {
    /// The still-unfiled activities the model found no category for that have an embedding from
    /// `model`, by id.
    fn get_unlabeled(&mut self, model: &str, window: &Window) -> QueryResult<Vec<UnlabeledRow>>;
    /// The ids of every still-unfiled activity the model found no category for.
    fn get_unlabeled_ids(&mut self) -> QueryResult<Vec<i64>>;
    fn count_unlabeled(&mut self) -> QueryResult<i64>;
    /// Per category, how the user answered the AI labels of activities seen since `since`. Only
    /// categories with a label moved out are listed.
    fn get_feedback_by_category(&mut self, since: &str) -> QueryResult<Vec<CategoryFeedbackRow>>;
    /// Activities pinned to `category_id`, those whose AI label the user confirmed first.
    fn get_kept_samples(&mut self, category_id: i64, limit: i64) -> QueryResult<Vec<SampleRow>>;
    /// Activities seen since `since` whose AI label in `category_id` the user moved out.
    fn get_moved_out_samples(&mut self, category_id: i64, since: &str, limit: i64) -> QueryResult<Vec<SampleRow>>;
    /// Per pair of categories, how many AI labels of activities seen since `since` the user
    /// pinned from the first to the second.
    fn get_correction_pairs(&mut self, since: &str) -> QueryResult<Vec<CorrectionPairRow>>;
    /// Per category, the time tracked in `window` on the activities filed there by a pin or a rule.
    fn get_filed_seconds(&mut self, window: &Window) -> QueryResult<Vec<CategorySecondsRow>>;
}

impl SuggestionsReportRepo for SqliteConnection {
    fn get_unlabeled(&mut self, model: &str, window: &Window) -> QueryResult<Vec<UnlabeledRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT a.id, src.name AS app_name,
                    CASE WHEN src.kind = 'domain' THEN 'https://' || src.key || a.path END AS url,
                    a.title, src.kind AS source_kind, src.key AS source_key, a.path,
                    e.vector, {seconds} AS total_seconds
             FROM w
             JOIN ai_unlabeled u
             JOIN activities a ON a.id = u.activity_id
             JOIN sources src ON src.id = a.source_id
             JOIN activity_embeddings e ON e.activity_id = a.id AND e.model = ?
             LEFT JOIN sessions s ON s.activity_id = a.id AND {overlaps}
             WHERE {STILL_UNFILED}
             GROUP BY a.id
             ORDER BY a.id",
            seconds = sum_seconds("s"),
            overlaps = overlaps("s"),
        );
        let mut binds = window.binds();
        binds.push(Param::Text(model.to_string()));
        bind_all(sql_query(sql).into_boxed(), binds).get_results(self)
    }

    fn get_unlabeled_ids(&mut self) -> QueryResult<Vec<i64>> {
        let sql = format!(
            "SELECT a.id FROM ai_unlabeled u JOIN activities a ON a.id = u.activity_id
             WHERE {STILL_UNFILED} ORDER BY a.id"
        );
        Ok(sql_query(sql).get_results::<IdRow>(self)?.into_iter().map(|row| row.id).collect())
    }

    fn count_unlabeled(&mut self) -> QueryResult<i64> {
        let sql = format!(
            "SELECT COUNT(*) AS count FROM ai_unlabeled u JOIN activities a ON a.id = u.activity_id
             WHERE {STILL_UNFILED}"
        );
        count(self, sql, Vec::new())
    }

    fn get_feedback_by_category(&mut self, since: &str) -> QueryResult<Vec<CategoryFeedbackRow>> {
        let sql = format!(
            "SELECT category_id, confirmed, corrected, rejected FROM (
               SELECT l.category_id,
                      SUM(l.rejected = 0 AND a.category_id = l.category_id) AS confirmed,
                      SUM(l.rejected = 0 AND {MOVED_OUT}) AS corrected,
                      SUM(l.rejected = 1 AND {MOVED_OUT}) AS rejected
               FROM ai_labels l
               JOIN activities a ON a.id = l.activity_id
               WHERE {SEEN_SINCE}
               GROUP BY l.category_id)
             WHERE corrected + rejected > 0
             ORDER BY category_id"
        );
        bind_all(sql_query(sql).into_boxed(), vec![Param::Text(since.to_string())]).get_results(self)
    }

    fn get_kept_samples(&mut self, category_id: i64, limit: i64) -> QueryResult<Vec<SampleRow>> {
        let sql = format!(
            "SELECT {SAMPLE_COLUMNS}, NULL AS moved_to
             FROM activities a
             JOIN sources src ON src.id = a.source_id
             LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0 AND l.category_id = a.category_id
             WHERE a.category_id = ?
             ORDER BY l.activity_id IS NULL, a.id DESC
             LIMIT ?"
        );
        bind_all(sql_query(sql).into_boxed(), vec![Param::I64(category_id), Param::I64(limit)]).get_results(self)
    }

    fn get_moved_out_samples(&mut self, category_id: i64, since: &str, limit: i64) -> QueryResult<Vec<SampleRow>> {
        let sql = format!(
            "SELECT {SAMPLE_COLUMNS}, a.category_id AS moved_to
             FROM ai_labels l
             JOIN activities a ON a.id = l.activity_id
             JOIN sources src ON src.id = a.source_id
             WHERE l.category_id = ? AND {MOVED_OUT} AND {SEEN_SINCE}
             ORDER BY a.id DESC
             LIMIT ?"
        );
        let binds = vec![Param::I64(category_id), Param::Text(since.to_string()), Param::I64(limit)];
        bind_all(sql_query(sql).into_boxed(), binds).get_results(self)
    }

    fn get_correction_pairs(&mut self, since: &str) -> QueryResult<Vec<CorrectionPairRow>> {
        let sql = format!(
            "SELECT l.category_id AS from_id, a.category_id AS into_id, COUNT(*) AS count
             FROM ai_labels l
             JOIN activities a ON a.id = l.activity_id
             WHERE a.category_id IS NOT NULL AND a.category_id != l.category_id AND {SEEN_SINCE}
             GROUP BY l.category_id, a.category_id
             ORDER BY from_id, into_id"
        );
        bind_all(sql_query(sql).into_boxed(), vec![Param::Text(since.to_string())]).get_results(self)
    }

    fn get_filed_seconds(&mut self, window: &Window) -> QueryResult<Vec<CategorySecondsRow>> {
        let sql = format!(
            "{WITH_WINDOW}
             SELECT ac.category_id, {seconds} AS seconds
             FROM w
             JOIN activity_category ac
             JOIN sessions s ON s.activity_id = ac.activity_id AND {overlaps}
             WHERE ac.source IN ('manual', 'rule')
             GROUP BY ac.category_id
             ORDER BY ac.category_id",
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
    use crate::reporting::window::assert_seconds;

    /// Five activities the model found no category for: 1 is unfiled with an embedding and an
    /// hour tracked, 2 has no embedding, 3 was pinned since, 4 got an AI label, 5 a rule.
    fn conn() -> SqliteConnection {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000');
             INSERT INTO sources (id, kind, key, name) VALUES (1, 'domain', 'example.com', 'Firefox');
             INSERT INTO activities (id, source_id, path, title, category_id) VALUES
               (1, 1, '/a', 'a', NULL), (2, 1, '', 'b', NULL), (3, 1, '', 'c', 1), (4, 1, '', 'd', NULL),
               (5, 1, '', 'e', NULL);
             INSERT INTO ai_labels (activity_id, category_id) VALUES (4, 1);
             INSERT INTO category_rules (id, category_id, kind, value) VALUES (1, 1, 'site', 'example.com');
             INSERT INTO rule_matches (activity_id, rule_id) VALUES (5, 1);
             INSERT INTO ai_unlabeled (activity_id, model, answered_at) VALUES
               (1, 'm', 'x'), (2, 'm', 'x'), (3, 'm', 'x'), (4, 'm', 'x'), (5, 'm', 'x');
             INSERT INTO activity_embeddings (activity_id, model, vector) VALUES (1, 'e', x'0000803f');
             INSERT INTO sessions (activity_id, start_time, end_time) VALUES
               (1, '2026-03-01T09:00:00+00:00', '2026-03-01T10:00:00+00:00');",
        )
        .unwrap();
        conn
    }

    fn now() -> Window {
        Window::until(DateTime::parse_from_rfc3339("2026-03-01T11:00:00Z").unwrap().with_timezone(&Utc))
    }

    #[test]
    fn only_still_unfiled_activities_count() {
        let mut conn = conn();
        assert_eq!(conn.get_unlabeled_ids().unwrap(), [1, 2]);
        assert_eq!(conn.count_unlabeled().unwrap(), 2);
    }

    /// Category 1's AI labels: 10 confirmed, 11 moved to 2, 12 rejected and left alone, 13
    /// rejected but matched by a rule, 14 moved to 2 but not seen since March; 15 is pending.
    fn feedback_conn() -> SqliteConnection {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000'), (2, 'Fun', '#000000');
             INSERT INTO sources (id, kind, key, name) VALUES (1, 'exe', 'app', 'App');
             INSERT INTO activities (id, source_id, path, title, category_id) VALUES
               (10, 1, '', 'kept', 1), (11, 1, '', 'moved', 2), (12, 1, '', 'rejected', NULL),
               (13, 1, '', 'ruled', NULL), (14, 1, '', 'old', 2), (15, 1, '', 'pending', NULL);
             INSERT INTO ai_labels (activity_id, category_id, rejected) VALUES
               (10, 1, 0), (11, 1, 0), (12, 1, 1), (13, 1, 1), (14, 1, 0), (15, 1, 0);
             INSERT INTO category_rules (id, category_id, kind, value) VALUES (1, 2, 'app', 'app');
             INSERT INTO rule_matches (activity_id, rule_id) VALUES (13, 1);
             INSERT INTO sessions (activity_id, start_time, end_time) VALUES
               (10, '2026-03-02T09:00:00+00:00', '2026-03-02T10:00:00+00:00'),
               (11, '2026-03-02T09:00:00+00:00', '2026-03-02T10:00:00+00:00'),
               (12, '2026-03-02T09:00:00+00:00', '2026-03-02T10:00:00+00:00'),
               (13, '2026-03-02T09:00:00+00:00', '2026-03-02T10:00:00+00:00'),
               (14, '2026-02-02T09:00:00+00:00', '2026-02-02T10:00:00+00:00'),
               (15, '2026-03-02T09:00:00+00:00', '2026-03-02T10:00:00+00:00');",
        )
        .unwrap();
        conn
    }

    const SINCE: &str = "2026-03-01T00:00:00+00:00";

    #[test]
    fn feedback_counts_what_the_user_moved_out_since_the_bound() {
        let mut conn = feedback_conn();
        assert_eq!(
            conn.get_feedback_by_category(SINCE).unwrap(),
            [CategoryFeedbackRow {
                category_id: 1,
                confirmed: 1,
                corrected: 1,
                rejected: 1,
            }]
        );
        assert!(conn.get_feedback_by_category("2026-04-01T00:00:00+00:00").unwrap().is_empty());
    }

    #[test]
    fn corrections_are_counted_per_pair_and_filed_time_per_category() {
        let mut conn = feedback_conn();
        assert_eq!(
            conn.get_correction_pairs(SINCE).unwrap(),
            [CorrectionPairRow {
                from_id: 1,
                into_id: 2,
                count: 1,
            }]
        );
        let window = Window::until(DateTime::parse_from_rfc3339("2026-04-01T00:00:00Z").unwrap().with_timezone(&Utc));
        let seconds: Vec<(i64, f64)> = conn
            .get_filed_seconds(&window)
            .unwrap()
            .into_iter()
            .map(|row| (row.category_id, row.seconds))
            .collect();
        assert_eq!(seconds.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [1, 2]);
        assert_seconds(seconds[0].1, 3600.);
        assert_seconds(seconds[1].1, 3.0 * 3600.);
    }

    #[test]
    fn samples_show_what_stayed_and_where_the_rest_went() {
        let mut conn = feedback_conn();
        let kept: Vec<String> = conn.get_kept_samples(1, 5).unwrap().into_iter().map(|row| row.title).collect();
        assert_eq!(kept, ["kept"]);
        let moved: Vec<(String, Option<i64>)> = conn
            .get_moved_out_samples(1, SINCE, 5)
            .unwrap()
            .into_iter()
            .map(|row| (row.title, row.moved_to))
            .collect();
        assert_eq!(moved, [("rejected".to_string(), None), ("moved".to_string(), Some(2))]);
    }

    #[test]
    fn points_need_an_embedding_from_the_model() {
        let mut conn = conn();
        let rows = conn.get_unlabeled("e", &now()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url.as_deref(), Some("https://example.com/a"));
        assert_eq!(rows[0].vector, [0, 0, 0x80, 0x3f]);
        assert_seconds(rows[0].total_seconds, 3600.0);
        assert!(conn.get_unlabeled("other", &now()).unwrap().is_empty());
    }
}
