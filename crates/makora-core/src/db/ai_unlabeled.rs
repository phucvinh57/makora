use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::upsert::excluded;
use diesel::QueryResult;

use super::schema::ai_unlabeled;

pub use super::traits::AiUnlabeledRepo;

impl AiUnlabeledRepo for SqliteConnection {
    fn upsert_many(&mut self, activity_ids: &[i64], model: &str, answered_at: &str) -> QueryResult<()> {
        for &id in activity_ids {
            diesel::insert_into(ai_unlabeled::table)
                .values((
                    ai_unlabeled::activity_id.eq(id),
                    ai_unlabeled::model.eq(model),
                    ai_unlabeled::answered_at.eq(answered_at),
                ))
                .on_conflict(ai_unlabeled::activity_id)
                .do_update()
                .set((
                    ai_unlabeled::model.eq(excluded(ai_unlabeled::model)),
                    ai_unlabeled::answered_at.eq(excluded(ai_unlabeled::answered_at)),
                ))
                .execute(self)?;
        }
        Ok(())
    }

    fn delete_many(&mut self, activity_ids: &[i64]) -> QueryResult<()> {
        diesel::delete(ai_unlabeled::table.filter(ai_unlabeled::activity_id.eq_any(activity_ids))).execute(self)?;
        Ok(())
    }

    fn delete_all(&mut self) -> QueryResult<usize> {
        diesel::delete(ai_unlabeled::table).execute(self)
    }

    fn get_ids(&mut self) -> QueryResult<Vec<i64>> {
        ai_unlabeled::table
            .select(ai_unlabeled::activity_id)
            .order(ai_unlabeled::activity_id)
            .load(self)
    }
}

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;

    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn rows_upsert_and_go_with_their_activity() {
        let mut conn = open_in_memory();
        conn.batch_execute(
            "INSERT INTO sources (id, kind, key, name) VALUES (1, 'exe', 'code', 'Code');
             INSERT INTO activities (id, source_id, path, title) VALUES (1, 1, '', 'a'), (2, 1, '', 'b');",
        )
        .unwrap();
        conn.upsert_many(&[1, 2], "m1", "2026-03-01T10:00:00+00:00").unwrap();
        conn.upsert_many(&[2], "m2", "2026-03-02T10:00:00+00:00").unwrap();
        assert_eq!(conn.get_ids().unwrap(), [1, 2]);
        conn.batch_execute("DELETE FROM activities WHERE id = 1;").unwrap();
        assert_eq!(conn.get_ids().unwrap(), [2]);
        conn.delete_many(&[2]).unwrap();
        assert!(conn.get_ids().unwrap().is_empty());
    }
}
