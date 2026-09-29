use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{NewSessionRow, SessionRow};
use super::schema::sessions;

pub fn find(conn: &mut SqliteConnection, id: i64) -> QueryResult<Option<SessionRow>> {
    sessions::table
        .find(id)
        .select(SessionRow::as_select())
        .first(conn)
        .optional()
}

/// The most recently opened session, whether or not it has closed yet.
pub fn last(conn: &mut SqliteConnection) -> QueryResult<Option<SessionRow>> {
    sessions::table
        .order(sessions::id.desc())
        .select(SessionRow::as_select())
        .first(conn)
        .optional()
}

pub fn insert(conn: &mut SqliteConnection, new: &NewSessionRow<'_>) -> QueryResult<SessionRow> {
    diesel::insert_into(sessions::table)
        .values(new)
        .returning(SessionRow::as_returning())
        .get_result(conn)
}

/// Reopens a session that was closed within the merge gap.
pub fn reopen(conn: &mut SqliteConnection, id: i64) -> QueryResult<()> {
    diesel::update(sessions::table.find(id))
        .set(sessions::end_time.eq(None::<String>))
        .execute(conn)?;
    Ok(())
}

pub fn set_end(conn: &mut SqliteConnection, id: i64, end_time: &str) -> QueryResult<()> {
    diesel::update(sessions::table.find(id))
        .set(sessions::end_time.eq(end_time))
        .execute(conn)?;
    Ok(())
}

/// Moves a session onto a different activity — used when a refined title resolves to a
/// different activity row than the one the session opened against.
pub fn reassign_activity(conn: &mut SqliteConnection, id: i64, activity_id: i64) -> QueryResult<()> {
    diesel::update(sessions::table.find(id))
        .set(sessions::activity_id.eq(activity_id))
        .execute(conn)?;
    Ok(())
}

pub fn delete(conn: &mut SqliteConnection, id: i64) -> QueryResult<()> {
    diesel::delete(sessions::table.find(id)).execute(conn)?;
    Ok(())
}

/// The activities behind `session_ids`. Ids with no matching session are silently skipped.
pub fn activity_ids_for(conn: &mut SqliteConnection, session_ids: &[i64]) -> QueryResult<Vec<i64>> {
    sessions::table
        .filter(sessions::id.eq_any(session_ids))
        .select(sessions::activity_id)
        .distinct()
        .load(conn)
}

/// Activities behind a session that never closed — left dangling by a crash or a forced quit.
pub fn open_activity_ids(conn: &mut SqliteConnection) -> QueryResult<Vec<i64>> {
    sessions::table
        .filter(sessions::end_time.is_null())
        .select(sessions::activity_id)
        .distinct()
        .load(conn)
}

pub fn delete_open(conn: &mut SqliteConnection) -> QueryResult<()> {
    diesel::delete(sessions::table.filter(sessions::end_time.is_null())).execute(conn)?;
    Ok(())
}
