use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Text};
use diesel::sqlite::SqliteConnection;
use diesel::{QueryResult, QueryableByName, RunQueryDsl};

use super::models::{CategoryChanges, CategoryRow, NewCategoryRow};
use super::schema::categories;

/// Every category, flat. Siblings come out in the order they should be shown under their
/// parent; the caller assembles the tree from `parent_id`.
pub fn list_ordered(conn: &mut SqliteConnection) -> QueryResult<Vec<CategoryRow>> {
    categories::table
        .order((sql::<Text>("name COLLATE NOCASE"), categories::id))
        .select(CategoryRow::as_select())
        .load(conn)
}

pub fn find(conn: &mut SqliteConnection, id: i64) -> QueryResult<Option<CategoryRow>> {
    categories::table
        .find(id)
        .select(CategoryRow::as_select())
        .first(conn)
        .optional()
}

pub fn exists(conn: &mut SqliteConnection, id: i64) -> QueryResult<bool> {
    diesel::select(diesel::dsl::exists(categories::table.find(id))).get_result(conn)
}

/// `(id, parent_id)` for every category, to build a [`crate::category::CategoryTree`] from.
pub fn parent_pairs(conn: &mut SqliteConnection) -> QueryResult<Vec<(i64, Option<i64>)>> {
    categories::table
        .select((categories::id, categories::parent_id))
        .load(conn)
}

pub fn insert(conn: &mut SqliteConnection, new: &NewCategoryRow<'_>) -> QueryResult<CategoryRow> {
    diesel::insert_into(categories::table)
        .values(new)
        .returning(CategoryRow::as_returning())
        .get_result(conn)
}

/// Replaces every editable field; `Ok(None)` when `id` does not exist.
pub fn update(
    conn: &mut SqliteConnection,
    id: i64,
    changes: &CategoryChanges<'_>,
) -> QueryResult<Option<CategoryRow>> {
    diesel::update(categories::table.find(id))
        .set(changes)
        .returning(CategoryRow::as_returning())
        .get_result(conn)
        .optional()
}

/// Deletes every category in `ids`, in the order given — callers pass deepest-first so a
/// child never outlives the parent it was deleted alongside.
pub fn delete_many(conn: &mut SqliteConnection, ids: &[i64]) -> QueryResult<()> {
    diesel::delete(categories::table.filter(categories::id.eq_any(ids))).execute(conn)?;
    Ok(())
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

/// Sessions whose activity currently resolves to a category — a manual pin or an active AI
/// label — for [`reset_categorization`](crate::domain::categories::Categories::reset_categorization)'s
/// summary, counted before both are cleared. The `COALESCE` join is why this stays raw SQL.
pub fn count_sessions_with_category(conn: &mut SqliteConnection) -> QueryResult<i64> {
    sql_query(
        "SELECT COUNT(*) AS count FROM sessions s
         JOIN activities a ON a.id = s.activity_id
         LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
         WHERE COALESCE(a.category_id, l.category_id) IS NOT NULL",
    )
    .get_result::<CountRow>(conn)
    .map(|row| row.count)
}
