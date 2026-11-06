use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{DismissalRow, DismissalValues};
use super::schema::rule_suggestion_dismissals;

pub use super::traits::RuleSuggestionDismissalsRepo;

impl RuleSuggestionDismissalsRepo for SqliteConnection {
    fn get_list(&mut self) -> QueryResult<Vec<DismissalRow>> {
        rule_suggestion_dismissals::table
            .select(DismissalRow::as_select())
            .load(self)
    }

    fn insert(&mut self, new: &DismissalValues<'_>) -> QueryResult<()> {
        diesel::insert_into(rule_suggestion_dismissals::table)
            .values(new)
            .on_conflict_do_nothing()
            .execute(self)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;

    use super::*;
    use crate::categorization::domain::rules::{PatternField, RuleKind};
    use crate::db::open_in_memory;

    fn dismiss(conn: &mut SqliteConnection, kind: &str, field: &str, value: &str, category_id: i64) {
        conn.insert(&DismissalValues {
            kind,
            field,
            value,
            category_id,
            dismissed_at: "2026-03-01T10:00:00+00:00",
        })
        .unwrap();
    }

    #[test]
    fn dismissals_round_trip_once_and_go_with_their_category() {
        let mut conn = open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000'), (2, 'Fun', '#ffffff');",
        )
        .unwrap();
        dismiss(&mut conn, "site", "", "github.com", 1);
        dismiss(&mut conn, "site", "", "github.com", 1);
        dismiss(&mut conn, "pattern", "title", "*jira*", 2);

        let rows = conn.get_list().unwrap();
        assert_eq!(rows.len(), 2);
        let site = rows.iter().find(|row| row.kind == RuleKind::Site).unwrap();
        assert_eq!((site.field, site.value.as_str(), site.category_id), (None, "github.com", 1));
        let pattern = rows.iter().find(|row| row.kind == RuleKind::Pattern).unwrap();
        assert_eq!(pattern.field, Some(PatternField::Title));

        conn.batch_execute("DELETE FROM categories WHERE id = 1;").unwrap();
        let rows = conn.get_list().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].category_id, 2);
    }
}
