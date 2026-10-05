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

use crate::db::sql::{bind_all, in_clause, Param};

use super::CategoryScope;

const ACTIVITY_URL: &str = "CASE WHEN src.kind = 'domain' THEN 'https://' || src.key || a.path END";

const SECONDS: &str = "COALESCE(SUM((julianday(s.end_time) - julianday(s.start_time)) * 86400.0), 0.0)";

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
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
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
    #[diesel(sql_type = Nullable<Double>)]
    pub min_confidence: Option<f64>,
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
    fn get_review_page(
        &mut self,
        page: i64,
        page_size: i64,
        category: &CategoryScope,
        activity_ids: Option<&[i64]>,
    ) -> QueryResult<(Vec<ReviewRow>, i64)>;
    fn get_review_groups(
        &mut self,
        page: i64,
        page_size: i64,
        category: &CategoryScope,
    ) -> QueryResult<(Vec<ReviewGroupRow>, i64)>;
    fn get_feedback_counts(&mut self) -> QueryResult<FeedbackCountsRow>;
    /// Time, not rows: a few long activities matter more than many short ones.
    fn get_feedback_seconds(&mut self) -> QueryResult<FeedbackSecondsRow>;
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

    fn get_review_page(
        &mut self,
        page: i64,
        page_size: i64,
        category: &CategoryScope,
        activity_ids: Option<&[i64]>,
    ) -> QueryResult<(Vec<ReviewRow>, i64)> {
        let (pending, pending_binds) = pending_fragment(category, activity_ids);

        let total_sql = format!(
            "SELECT COUNT(*) AS count FROM activities a
             JOIN activity_category ac ON ac.activity_id = a.id
             WHERE {pending}"
        );
        let total = bind_all(sql_query(total_sql).into_boxed(), pending_binds.clone())
            .get_result::<CountRow>(self)?
            .count;

        let mut binds = pending_binds;
        binds.push(Param::I64(page_size));
        binds.push(Param::I64(page * page_size));
        let rows_sql = format!(
            "SELECT a.id AS activity_id, src.name AS app_name, {ACTIVITY_URL} AS url, a.title,
                    cat.id AS category_id, cat.name AS category_name, cat.color AS category_color,
                    {SECONDS} AS total_seconds, COUNT(s.id) AS session_count
             FROM activities a
             JOIN sources src ON src.id = a.source_id
             JOIN activity_category ac ON ac.activity_id = a.id
             JOIN categories cat ON cat.id = ac.category_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND s.end_time IS NOT NULL
             WHERE {pending}
             GROUP BY a.id
             ORDER BY 8 DESC, a.id
             LIMIT ? OFFSET ?"
        );
        let rows = bind_all(sql_query(rows_sql).into_boxed(), binds).get_results(self)?;
        Ok((rows, total))
    }

    fn get_review_groups(
        &mut self,
        page: i64,
        page_size: i64,
        category: &CategoryScope,
    ) -> QueryResult<(Vec<ReviewGroupRow>, i64)> {
        // A group picks its own members, so unlike `review_page` this ignores any activity filter.
        let (pending, pending_binds) = pending_fragment(category, None);
        let grouped = format!(
            "SELECT src.kind AS key_kind, src.key, MIN(src.name) AS app_name, cat.id AS category_id,
                    cat.name AS category_name, cat.color AS category_color,
                    COUNT(DISTINCT a.id) AS activity_count, {SECONDS} AS total_seconds,
                    MIN(ac.confidence) AS min_confidence, GROUP_CONCAT(DISTINCT a.id) AS activity_ids
             FROM activities a
             JOIN sources src ON src.id = a.source_id
             JOIN activity_category ac ON ac.activity_id = a.id
             JOIN categories cat ON cat.id = ac.category_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND s.end_time IS NOT NULL
             WHERE {pending}
             GROUP BY src.id, cat.id"
        );

        let total = bind_all(
            sql_query(format!("SELECT COUNT(*) AS count FROM ({grouped})")).into_boxed(),
            pending_binds.clone(),
        )
        .get_result::<CountRow>(self)?
        .count;

        let mut binds = pending_binds;
        binds.push(Param::I64(page_size));
        binds.push(Param::I64(page * page_size));
        let rows_sql = format!("{grouped} ORDER BY 8 DESC, key_kind, key LIMIT ? OFFSET ?");
        let groups = bind_all(sql_query(rows_sql).into_boxed(), binds).get_results(self)?;
        Ok((groups, total))
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
               COALESCE(SUM(a.category_id IS NULL AND rej.activity_id IS NOT NULL), 0) AS rejected,
               COALESCE(SUM(a.category_id IS NULL AND l.activity_id IS NOT NULL
                            AND m.activity_id IS NULL), 0) AS pending_review
             FROM activities a
             LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
             LEFT JOIN ai_labels rej ON rej.activity_id = a.id AND rej.rejected = 1
             LEFT JOIN rule_matches m ON m.activity_id = a.id",
        )
        .get_result(self)
    }

    fn get_feedback_seconds(&mut self) -> QueryResult<FeedbackSecondsRow> {
        sql_query(
            "SELECT COALESCE(SUM(CASE WHEN pending THEN secs END), 0.0) AS pending_seconds,
                    COALESCE(SUM(CASE WHEN reviewed THEN secs END), 0.0) AS reviewed_seconds
             FROM (
               SELECT (a.category_id IS NULL AND l.activity_id IS NOT NULL
                       AND m.activity_id IS NULL) AS pending,
                      ((l.activity_id IS NOT NULL AND a.category_id IS NOT NULL)
                       OR rej.activity_id IS NOT NULL) AS reviewed,
                      (SELECT COALESCE(SUM((julianday(s.end_time) - julianday(s.start_time))
                                           * 86400.0), 0.0)
                       FROM sessions s
                       WHERE s.activity_id = a.id AND s.end_time IS NOT NULL) AS secs
               FROM activities a
               LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
               LEFT JOIN ai_labels rej ON rej.activity_id = a.id AND rej.rejected = 1
               LEFT JOIN rule_matches m ON m.activity_id = a.id)",
        )
        .get_result(self)
    }
}
