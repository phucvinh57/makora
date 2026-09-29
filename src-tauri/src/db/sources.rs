use diesel::dsl::{exists, not};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{NewSourceRow, SourceRow};
use super::schema::{activities, sources};

pub use super::traits::SourcesRepo;

impl SourcesRepo for SqliteConnection {
    fn get_by_key(&mut self, kind: &str, key: &str) -> QueryResult<Option<SourceRow>> {
        sources::table
            .filter(sources::kind.eq(kind))
            .filter(sources::key.eq(key))
            .select(SourceRow::as_select())
            .first(self)
            .optional()
    }

    fn insert(&mut self, new: &NewSourceRow<'_>) -> QueryResult<SourceRow> {
        diesel::insert_into(sources::table)
            .values(new)
            .returning(SourceRow::as_returning())
            .get_result(self)
    }

    fn set_name(&mut self, id: i64, name: &str) -> QueryResult<()> {
        diesel::update(sources::table.find(id))
            .set(sources::name.eq(name))
            .execute(self)?;
        Ok(())
    }

    fn delete_unused_many(&mut self, ids: &[i64]) -> QueryResult<()> {
        diesel::delete(
            sources::table.filter(sources::id.eq_any(ids)).filter(not(exists(
                activities::table.filter(activities::source_id.eq(sources::id)),
            ))),
        )
        .execute(self)?;
        Ok(())
    }
}
