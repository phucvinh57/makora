use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{NewTaxonomySuggestionRow, TaxonomySuggestionRow};
use super::schema::taxonomy_suggestions;

pub use super::traits::TaxonomySuggestionsRepo;

impl TaxonomySuggestionsRepo for SqliteConnection {
    fn insert(&mut self, new: &NewTaxonomySuggestionRow<'_>) -> QueryResult<i64> {
        diesel::insert_into(taxonomy_suggestions::table)
            .values(new)
            .returning(taxonomy_suggestions::id)
            .get_result(self)
    }

    fn update_payload(&mut self, id: i64, payload: &str) -> QueryResult<()> {
        diesel::update(taxonomy_suggestions::table.find(id))
            .set(taxonomy_suggestions::payload.eq(payload))
            .execute(self)?;
        Ok(())
    }

    fn set_status(&mut self, id: i64, from: &str, to: &str, at: &str) -> QueryResult<bool> {
        let changed = diesel::update(
            taxonomy_suggestions::table
                .find(id)
                .filter(taxonomy_suggestions::status.eq(from)),
        )
        .set((taxonomy_suggestions::status.eq(to), taxonomy_suggestions::decided_at.eq(at)))
        .execute(self)?;
        Ok(changed > 0)
    }

    fn get(&mut self, id: i64) -> QueryResult<Option<TaxonomySuggestionRow>> {
        taxonomy_suggestions::table
            .find(id)
            .select(TaxonomySuggestionRow::as_select())
            .first(self)
            .optional()
    }

    fn get_list(&mut self, statuses: &[&str]) -> QueryResult<Vec<TaxonomySuggestionRow>> {
        taxonomy_suggestions::table
            .filter(taxonomy_suggestions::status.eq_any(statuses))
            .order(taxonomy_suggestions::id)
            .select(TaxonomySuggestionRow::as_select())
            .load(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn new<'a>(kind: &'a str, payload: &'a str) -> NewTaxonomySuggestionRow<'a> {
        NewTaxonomySuggestionRow {
            kind,
            payload,
            created_at: "2026-03-01T10:00:00+00:00",
            run_id: None,
        }
    }

    #[test]
    fn a_suggestion_round_trips_and_changes_status_once() {
        let mut conn = open_in_memory();
        let id = conn.insert(&new("new", "{}")).unwrap();
        conn.update_payload(id, r#"{"a":1}"#).unwrap();
        let row = conn.get(id).unwrap().unwrap();
        assert_eq!(
            (row.kind.as_str(), row.payload.as_str(), row.status.as_str()),
            ("new", r#"{"a":1}"#, "pending")
        );

        assert!(conn.set_status(id, "pending", "dismissed", "y").unwrap());
        assert!(!conn.set_status(id, "pending", "accepted", "z").unwrap());
        assert!(conn.get_list(&["pending"]).unwrap().is_empty());
        assert_eq!(conn.get_list(&["pending", "dismissed"]).unwrap().len(), 1);
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        let mut conn = open_in_memory();
        assert!(conn.insert(&new("rename", "{}")).is_err());
    }
}
