use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{CategoryRuleRow, CategoryRuleValues};
use super::schema::category_rules;

pub use super::traits::CategoryRulesRepo;

impl CategoryRulesRepo for SqliteConnection {
    fn get_list(&mut self) -> QueryResult<Vec<CategoryRuleRow>> {
        category_rules::table
            .order(category_rules::id)
            .select(CategoryRuleRow::as_select())
            .load(self)
    }

    fn insert(&mut self, new: &CategoryRuleValues<'_>) -> QueryResult<CategoryRuleRow> {
        diesel::insert_into(category_rules::table)
            .values(new)
            .returning(CategoryRuleRow::as_returning())
            .get_result(self)
    }

    fn update(&mut self, id: i64, values: &CategoryRuleValues<'_>) -> QueryResult<Option<CategoryRuleRow>> {
        diesel::update(category_rules::table.find(id))
            .set(values)
            .returning(CategoryRuleRow::as_returning())
            .get_result(self)
            .optional()
    }

    fn delete(&mut self, id: i64) -> QueryResult<bool> {
        Ok(diesel::delete(category_rules::table.find(id)).execute(self)? > 0)
    }

    fn delete_for_categories(&mut self, category_ids: &[i64]) -> QueryResult<()> {
        diesel::delete(category_rules::table.filter(category_rules::category_id.eq_any(category_ids))).execute(self)?;
        Ok(())
    }
}
