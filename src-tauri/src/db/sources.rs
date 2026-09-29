use diesel::dsl::{exists, not};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{NewSourceRow, SourceRow};
use super::schema::{activities, sources};

pub fn find_by_key(conn: &mut SqliteConnection, kind: &str, key: &str) -> QueryResult<Option<SourceRow>> {
    sources::table
        .filter(sources::kind.eq(kind))
        .filter(sources::key.eq(key))
        .select(SourceRow::as_select())
        .first(conn)
        .optional()
}

pub fn insert(conn: &mut SqliteConnection, new: &NewSourceRow<'_>) -> QueryResult<SourceRow> {
    diesel::insert_into(sources::table)
        .values(new)
        .returning(SourceRow::as_returning())
        .get_result(conn)
}

pub fn set_name(conn: &mut SqliteConnection, id: i64, name: &str) -> QueryResult<()> {
    diesel::update(sources::table.find(id))
        .set(sources::name.eq(name))
        .execute(conn)?;
    Ok(())
}

/// Deletes the source if nothing in `activities` points to it any more.
pub fn delete_if_unused(conn: &mut SqliteConnection, id: i64) -> QueryResult<()> {
    diesel::delete(
        sources::table
            .find(id)
            .filter(not(exists(activities::table.filter(activities::source_id.eq(id))))),
    )
    .execute(conn)?;
    Ok(())
}
