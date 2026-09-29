use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::AiLabelRow;
use super::schema::ai_labels;

/// The label still standing for an activity — `None` once it has been rejected.
pub fn find_active(conn: &mut SqliteConnection, activity_id: i64) -> QueryResult<Option<AiLabelRow>> {
    ai_labels::table
        .find(activity_id)
        .filter(ai_labels::rejected.eq(false))
        .select(AiLabelRow::as_select())
        .first(conn)
        .optional()
}

/// Sets the AI label for `activity_id`, replacing whatever was there (rejected or not).
/// `activity_id` is the table's primary key, so this is insert-or-overwrite.
pub fn upsert(
    conn: &mut SqliteConnection,
    activity_id: i64,
    category_id: i64,
    confidence: f64,
) -> QueryResult<()> {
    let updated = diesel::update(ai_labels::table.find(activity_id))
        .set((
            ai_labels::category_id.eq(category_id),
            ai_labels::confidence.eq(confidence),
            ai_labels::rejected.eq(false),
        ))
        .execute(conn)?;
    if updated == 0 {
        diesel::insert_into(ai_labels::table)
            .values((
                ai_labels::activity_id.eq(activity_id),
                ai_labels::category_id.eq(category_id),
                ai_labels::confidence.eq(confidence),
                ai_labels::rejected.eq(false),
            ))
            .execute(conn)?;
    }
    Ok(())
}

/// Removes the active (non-rejected) label, leaving the activity with none.
pub fn clear_active(conn: &mut SqliteConnection, activity_id: i64) -> QueryResult<usize> {
    diesel::delete(
        ai_labels::table
            .find(activity_id)
            .filter(ai_labels::rejected.eq(false)),
    )
    .execute(conn)
}

/// Marks the active label rejected instead of deleting it, so the model can be told "not
/// this" next time.
pub fn reject_active(conn: &mut SqliteConnection, activity_id: i64) -> QueryResult<()> {
    diesel::update(
        ai_labels::table
            .find(activity_id)
            .filter(ai_labels::rejected.eq(false)),
    )
    .set((ai_labels::rejected.eq(true), ai_labels::confidence.eq(None::<f64>)))
    .execute(conn)?;
    Ok(())
}

pub fn delete_for_activity(conn: &mut SqliteConnection, activity_id: i64) -> QueryResult<()> {
    diesel::delete(ai_labels::table.find(activity_id)).execute(conn)?;
    Ok(())
}

pub fn delete_for_categories(conn: &mut SqliteConnection, category_ids: &[i64]) -> QueryResult<()> {
    diesel::delete(ai_labels::table.filter(ai_labels::category_id.eq_any(category_ids))).execute(conn)?;
    Ok(())
}

pub fn delete_all(conn: &mut SqliteConnection) -> QueryResult<()> {
    diesel::delete(ai_labels::table).execute(conn)?;
    Ok(())
}
