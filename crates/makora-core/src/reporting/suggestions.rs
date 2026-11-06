//! Read models behind taxonomy suggestions: the activities the model found no category for and
//! that are still unfiled, with their embeddings and tracked time.

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
