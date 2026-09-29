use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::upsert::excluded;
use diesel::QueryResult;

use super::models::AiLabelRow;
use super::schema::ai_labels;

pub use super::traits::AiLabelsRepo;

impl AiLabelsRepo for SqliteConnection {
    fn get_active_many(&mut self, activity_ids: &[i64]) -> QueryResult<Vec<AiLabelRow>> {
        ai_labels::table
            .filter(ai_labels::activity_id.eq_any(activity_ids))
            .filter(ai_labels::rejected.eq(false))
            .select(AiLabelRow::as_select())
            .load(self)
    }

    fn upsert_many(&mut self, labels: &[(i64, i64, f64)]) -> QueryResult<()> {
        if labels.is_empty() {
            return Ok(());
        }
        let values: Vec<_> = labels
            .iter()
            .map(|&(activity_id, category_id, confidence)| {
                (
                    ai_labels::activity_id.eq(activity_id),
                    ai_labels::category_id.eq(category_id),
                    ai_labels::confidence.eq(confidence),
                    ai_labels::rejected.eq(false),
                )
            })
            .collect();
        diesel::insert_into(ai_labels::table)
            .values(values)
            .on_conflict(ai_labels::activity_id)
            .do_update()
            .set((
                ai_labels::category_id.eq(excluded(ai_labels::category_id)),
                ai_labels::confidence.eq(excluded(ai_labels::confidence)),
                ai_labels::rejected.eq(false),
            ))
            .execute(self)?;
        Ok(())
    }

    fn clear_active_many(&mut self, activity_ids: &[i64]) -> QueryResult<usize> {
        diesel::delete(
            ai_labels::table
                .filter(ai_labels::activity_id.eq_any(activity_ids))
                .filter(ai_labels::rejected.eq(false)),
        )
        .execute(self)
    }

    fn reject_active_many(&mut self, activity_ids: &[i64]) -> QueryResult<()> {
        diesel::update(
            ai_labels::table
                .filter(ai_labels::activity_id.eq_any(activity_ids))
                .filter(ai_labels::rejected.eq(false)),
        )
        .set((ai_labels::rejected.eq(true), ai_labels::confidence.eq(None::<f64>)))
        .execute(self)?;
        Ok(())
    }

    fn delete_for_activities(&mut self, activity_ids: &[i64]) -> QueryResult<()> {
        diesel::delete(ai_labels::table.filter(ai_labels::activity_id.eq_any(activity_ids))).execute(self)?;
        Ok(())
    }

    fn delete_for_categories(&mut self, category_ids: &[i64]) -> QueryResult<()> {
        diesel::delete(ai_labels::table.filter(ai_labels::category_id.eq_any(category_ids))).execute(self)?;
        Ok(())
    }

    fn delete_all(&mut self) -> QueryResult<()> {
        diesel::delete(ai_labels::table).execute(self)?;
        Ok(())
    }
}
