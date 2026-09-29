use diesel::dsl::{exists, not};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{ActivityRow, NewActivityRow};
use super::schema::{activities, sessions};

pub fn find(conn: &mut SqliteConnection, id: i64) -> QueryResult<Option<ActivityRow>> {
    activities::table
        .find(id)
        .select(ActivityRow::as_select())
        .first(conn)
        .optional()
}

pub fn find_by_identity(
    conn: &mut SqliteConnection,
    source_id: i64,
    path: &str,
    title: &str,
) -> QueryResult<Option<ActivityRow>> {
    activities::table
        .filter(activities::source_id.eq(source_id))
        .filter(activities::path.eq(path))
        .filter(activities::title.eq(title))
        .select(ActivityRow::as_select())
        .first(conn)
        .optional()
}

pub fn insert(conn: &mut SqliteConnection, new: &NewActivityRow<'_>) -> QueryResult<ActivityRow> {
    diesel::insert_into(activities::table)
        .values(new)
        .returning(ActivityRow::as_returning())
        .get_result(conn)
}

pub fn is_unpinned(conn: &mut SqliteConnection, id: i64) -> QueryResult<bool> {
    diesel::select(diesel::dsl::exists(
        activities::table
            .find(id)
            .filter(activities::category_id.is_null()),
    ))
    .get_result(conn)
}

/// Pins `id` to `category_id`, or unpins it with `None`.
pub fn set_category(conn: &mut SqliteConnection, id: i64, category_id: Option<i64>) -> QueryResult<()> {
    diesel::update(activities::table.find(id))
        .set(activities::category_id.eq(category_id))
        .execute(conn)?;
    Ok(())
}

/// Pins `id` to `category_id` only if it is not already pinned. Returns whether it changed.
pub fn set_category_if_unpinned(
    conn: &mut SqliteConnection,
    id: i64,
    category_id: i64,
) -> QueryResult<bool> {
    let updated = diesel::update(
        activities::table
            .find(id)
            .filter(activities::category_id.is_null()),
    )
    .set(activities::category_id.eq(category_id))
    .execute(conn)?;
    Ok(updated > 0)
}

/// Unpins every activity currently pinned to one of `category_ids` — used when those
/// categories are about to be deleted.
pub fn unpin_categories(conn: &mut SqliteConnection, category_ids: &[i64]) -> QueryResult<()> {
    diesel::update(activities::table.filter(activities::category_id.eq_any(category_ids)))
        .set(activities::category_id.eq(None::<i64>))
        .execute(conn)?;
    Ok(())
}

/// Clears every manual pin.
pub fn clear_all_pins(conn: &mut SqliteConnection) -> QueryResult<()> {
    diesel::update(activities::table.filter(activities::category_id.is_not_null()))
        .set(activities::category_id.eq(None::<i64>))
        .execute(conn)?;
    Ok(())
}

/// The activity if it is unpinned and no session references it any more — a candidate for
/// deletion once its last session goes away.
pub fn find_prunable(conn: &mut SqliteConnection, id: i64) -> QueryResult<Option<ActivityRow>> {
    activities::table
        .find(id)
        .filter(activities::category_id.is_null())
        .filter(not(exists(sessions::table.filter(sessions::activity_id.eq(id)))))
        .select(ActivityRow::as_select())
        .first(conn)
        .optional()
}

pub fn delete(conn: &mut SqliteConnection, id: i64) -> QueryResult<()> {
    diesel::delete(activities::table.find(id)).execute(conn)?;
    Ok(())
}
