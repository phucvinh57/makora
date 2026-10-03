use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Binary, Text};
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{ActivityIdentityRow, CategorizedEmbeddingRow};
use super::schema::{activities, activity_embeddings, sources};

pub use super::traits::ActivityEmbeddingsRepo;

impl ActivityEmbeddingsRepo for SqliteConnection {
    fn get_missing(&mut self, model: &str, limit: i64) -> QueryResult<Vec<ActivityIdentityRow>> {
        activities::table
            .inner_join(sources::table)
            .left_join(
                activity_embeddings::table.on(activity_embeddings::activity_id
                    .eq(activities::id)
                    .and(activity_embeddings::model.eq(model))),
            )
            .filter(activity_embeddings::activity_id.is_null())
            .order(activities::id.desc())
            .limit(limit)
            .select((
                activities::id,
                sources::kind,
                sources::key,
                sources::name,
                activities::path,
                activities::title,
            ))
            .load(self)
    }

    fn upsert_many(&mut self, model: &str, vectors: &[(i64, Vec<u8>)]) -> QueryResult<()> {
        for (activity_id, vector) in vectors {
            sql_query(
                "INSERT INTO activity_embeddings (activity_id, model, vector)
                 SELECT ?, ?, ? WHERE EXISTS (SELECT 1 FROM activities WHERE id = ?)
                 ON CONFLICT(activity_id) DO UPDATE SET model = excluded.model, vector = excluded.vector",
            )
            .bind::<BigInt, _>(activity_id)
            .bind::<Text, _>(model)
            .bind::<Binary, _>(vector)
            .bind::<BigInt, _>(activity_id)
            .execute(self)?;
        }
        Ok(())
    }

    fn get_many(&mut self, model: &str, activity_ids: &[i64]) -> QueryResult<Vec<(i64, Vec<u8>)>> {
        activity_embeddings::table
            .filter(activity_embeddings::model.eq(model))
            .filter(activity_embeddings::activity_id.eq_any(activity_ids))
            .select((activity_embeddings::activity_id, activity_embeddings::vector))
            .load(self)
    }

    fn get_categorized(&mut self, model: &str) -> QueryResult<Vec<CategorizedEmbeddingRow>> {
        sql_query(
            "SELECT a.id AS activity_id, src.kind AS source_kind, src.key AS source_key,
                    src.name AS source_name, a.path, a.title, ac.category_id, e.vector
             FROM activity_embeddings e
             JOIN activities a ON a.id = e.activity_id
             JOIN sources src ON src.id = a.source_id
             JOIN activity_category ac ON ac.activity_id = a.id
             WHERE e.model = ? AND ac.source IN ('manual', 'rule')",
        )
        .bind::<Text, _>(model)
        .get_results(self)
    }
}
