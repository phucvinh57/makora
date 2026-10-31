use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{NewSessionRow, SessionRow};
use super::schema::sessions;

pub use super::traits::SessionsRepo;

impl SessionsRepo for SqliteConnection {
    fn get(&mut self, id: i64) -> QueryResult<Option<SessionRow>> {
        sessions::table
            .find(id)
            .select(SessionRow::as_select())
            .first(self)
            .optional()
    }

    fn get_last(&mut self) -> QueryResult<Option<SessionRow>> {
        sessions::table
            .order(sessions::id.desc())
            .select(SessionRow::as_select())
            .first(self)
            .optional()
    }

    fn insert(&mut self, new: &NewSessionRow<'_>) -> QueryResult<SessionRow> {
        diesel::insert_into(sessions::table)
            .values(new)
            .returning(SessionRow::as_returning())
            .get_result(self)
    }

    fn reopen(&mut self, id: i64, at: &str) -> QueryResult<()> {
        diesel::update(sessions::table.find(id))
            .set((sessions::end_time.eq(None::<String>), sessions::last_seen.eq(at)))
            .execute(self)?;
        Ok(())
    }

    fn touch(&mut self, id: i64, at: &str) -> QueryResult<()> {
        diesel::update(sessions::table.find(id).filter(sessions::end_time.is_null()))
            .set(sessions::last_seen.eq(at))
            .execute(self)?;
        Ok(())
    }

    fn set_end(&mut self, id: i64, end_time: &str) -> QueryResult<()> {
        diesel::update(sessions::table.find(id))
            .set(sessions::end_time.eq(end_time))
            .execute(self)?;
        Ok(())
    }

    fn reassign_activity(&mut self, id: i64, activity_id: i64) -> QueryResult<()> {
        diesel::update(sessions::table.find(id))
            .set(sessions::activity_id.eq(activity_id))
            .execute(self)?;
        Ok(())
    }

    fn delete(&mut self, id: i64) -> QueryResult<()> {
        diesel::delete(sessions::table.find(id)).execute(self)?;
        Ok(())
    }

    fn get_activity_ids_for(&mut self, session_ids: &[i64]) -> QueryResult<Vec<i64>> {
        sessions::table
            .filter(sessions::id.eq_any(session_ids))
            .select(sessions::activity_id)
            .distinct()
            .load(self)
    }

    fn get_open(&mut self) -> QueryResult<Vec<SessionRow>> {
        sessions::table
            .filter(sessions::end_time.is_null())
            .select(SessionRow::as_select())
            .load(self)
    }
}
