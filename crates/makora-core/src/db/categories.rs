use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::{Text};
use diesel::sqlite::SqliteConnection;
use diesel::{QueryResult, RunQueryDsl};

use super::models::{CategoryChanges, CategoryRow, NewCategoryRow};
use super::schema::categories;

pub use super::traits::CategoriesRepo;

impl CategoriesRepo for SqliteConnection {
    fn get_list(&mut self) -> QueryResult<Vec<CategoryRow>> {
        categories::table
            .order((sql::<Text>("name COLLATE NOCASE"), categories::id))
            .select(CategoryRow::as_select())
            .load(self)
    }

    fn filter_existing(&mut self, ids: &[i64]) -> QueryResult<Vec<i64>> {
        categories::table
            .filter(categories::id.eq_any(ids))
            .select(categories::id)
            .load(self)
    }

    fn get_parent_pairs(&mut self) -> QueryResult<Vec<(i64, Option<i64>)>> {
        categories::table
            .select((categories::id, categories::parent_id))
            .load(self)
    }

    fn insert(&mut self, new: &NewCategoryRow<'_>) -> QueryResult<CategoryRow> {
        diesel::insert_into(categories::table)
            .values(new)
            .returning(CategoryRow::as_returning())
            .get_result(self)
    }

    fn update(&mut self, id: i64, changes: &CategoryChanges<'_>) -> QueryResult<Option<CategoryRow>> {
        diesel::update(categories::table.find(id))
            .set(changes)
            .returning(CategoryRow::as_returning())
            .get_result(self)
            .optional()
    }

    fn delete_many(&mut self, ids: &[i64]) -> QueryResult<()> {
        diesel::delete(categories::table.filter(categories::id.eq_any(ids))).execute(self)?;
        Ok(())
    }
}
