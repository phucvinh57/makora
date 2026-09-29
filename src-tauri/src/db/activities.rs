use diesel::dsl::{exists, not};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{ActivityRow, NewActivityRow};
use super::schema::{activities, sessions};

pub use super::traits::ActivitiesRepo;

impl ActivitiesRepo for SqliteConnection {
    fn get(&mut self, id: i64) -> QueryResult<Option<ActivityRow>> {
        activities::table
            .find(id)
            .select(ActivityRow::as_select())
            .first(self)
            .optional()
    }

    fn get_by_identity(&mut self, source_id: i64, path: &str, title: &str) -> QueryResult<Option<ActivityRow>> {
        activities::table
            .filter(activities::source_id.eq(source_id))
            .filter(activities::path.eq(path))
            .filter(activities::title.eq(title))
            .select(ActivityRow::as_select())
            .first(self)
            .optional()
    }

    fn insert(&mut self, new: &NewActivityRow<'_>) -> QueryResult<ActivityRow> {
        diesel::insert_into(activities::table)
            .values(new)
            .returning(ActivityRow::as_returning())
            .get_result(self)
    }

    fn filter_unpinned(&mut self, ids: &[i64]) -> QueryResult<Vec<i64>> {
        activities::table
            .filter(activities::id.eq_any(ids))
            .filter(activities::category_id.is_null())
            .select(activities::id)
            .load(self)
    }

    fn set_category_many(&mut self, ids: &[i64], category_id: Option<i64>) -> QueryResult<()> {
        diesel::update(activities::table.filter(activities::id.eq_any(ids)))
            .set(activities::category_id.eq(category_id))
            .execute(self)?;
        Ok(())
    }

    fn set_category_if_unpinned(&mut self, id: i64, category_id: i64) -> QueryResult<bool> {
        let updated = diesel::update(
            activities::table
                .find(id)
                .filter(activities::category_id.is_null()),
        )
        .set(activities::category_id.eq(category_id))
        .execute(self)?;
        Ok(updated > 0)
    }

    fn set_category_many_if_unpinned(&mut self, ids: &[i64], category_id: i64) -> QueryResult<()> {
        diesel::update(
            activities::table
                .filter(activities::id.eq_any(ids))
                .filter(activities::category_id.is_null()),
        )
        .set(activities::category_id.eq(category_id))
        .execute(self)?;
        Ok(())
    }

    fn unpin_categories(&mut self, category_ids: &[i64]) -> QueryResult<()> {
        diesel::update(activities::table.filter(activities::category_id.eq_any(category_ids)))
            .set(activities::category_id.eq(None::<i64>))
            .execute(self)?;
        Ok(())
    }

    fn clear_all_pins(&mut self) -> QueryResult<()> {
        diesel::update(activities::table.filter(activities::category_id.is_not_null()))
            .set(activities::category_id.eq(None::<i64>))
            .execute(self)?;
        Ok(())
    }

    fn get_prunable_many(&mut self, ids: &[i64]) -> QueryResult<Vec<ActivityRow>> {
        activities::table
            .filter(activities::id.eq_any(ids))
            .filter(activities::category_id.is_null())
            .filter(not(exists(
                sessions::table.filter(sessions::activity_id.eq(activities::id)),
            )))
            .select(ActivityRow::as_select())
            .load(self)
    }

    fn delete_many(&mut self, ids: &[i64]) -> QueryResult<()> {
        diesel::delete(activities::table.filter(activities::id.eq_any(ids))).execute(self)?;
        Ok(())
    }
}
