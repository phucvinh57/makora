use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::schema::rule_matches;

pub use super::traits::RuleMatchesRepo;

/// SQLite caps the number of bound variables in one statement; two per row, with room to spare.
const CHUNK: usize = 400;

impl RuleMatchesRepo for SqliteConnection {
    fn replace_all(&mut self, matches: &[(i64, i64)]) -> QueryResult<()> {
        diesel::delete(rule_matches::table).execute(self)?;
        for chunk in matches.chunks(CHUNK) {
            let values: Vec<_> = chunk
                .iter()
                .map(|&(activity_id, rule_id)| {
                    (
                        rule_matches::activity_id.eq(activity_id),
                        rule_matches::rule_id.eq(rule_id),
                    )
                })
                .collect();
            diesel::insert_into(rule_matches::table).values(values).execute(self)?;
        }
        Ok(())
    }

    fn set(&mut self, activity_id: i64, rule_id: i64) -> QueryResult<()> {
        diesel::replace_into(rule_matches::table)
            .values((
                rule_matches::activity_id.eq(activity_id),
                rule_matches::rule_id.eq(rule_id),
            ))
            .execute(self)?;
        Ok(())
    }
}
