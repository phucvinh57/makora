//! Text columns read straight into the enums they hold, so a repository hands out parsed values.
//! Each column's `CHECK` constraint allows only these values; anything else is a corrupt row and
//! fails the read instead of being guessed at.

use diesel::deserialize::{self, FromSql, Queryable};
use diesel::sql_types::Text;
use diesel::sqlite::{Sqlite, SqliteValue};

use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::domain::usage::{RunStatus, RunTrigger};
use crate::domain::types::CategorySource;
use crate::tracking::domain::source_key::KeyKind;

macro_rules! text_enum {
    ($($ty:ty),*) => {$(
        impl FromSql<Text, Sqlite> for $ty {
            fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
                let text = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
                Ok(<$ty>::try_from(text.as_str())?)
            }
        }

        impl Queryable<Text, Sqlite> for $ty {
            type Row = Self;

            fn build(row: Self) -> deserialize::Result<Self> {
                Ok(row)
            }
        }
    )*};
}

text_enum!(KeyKind, RuleKind, CategorySource, RunTrigger, RunStatus);

/// `category_rules.field`: a pattern's field, `''` for the other kinds of rule.
pub struct PatternFieldColumn(Option<PatternField>);

impl FromSql<Text, Sqlite> for PatternFieldColumn {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let text = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
        if text.is_empty() {
            return Ok(Self(None));
        }
        Ok(Self(Some(PatternField::try_from(text.as_str())?)))
    }
}

impl Queryable<Text, Sqlite> for PatternFieldColumn {
    type Row = Self;

    fn build(row: Self) -> deserialize::Result<Self> {
        Ok(row)
    }
}

impl From<PatternFieldColumn> for Option<PatternField> {
    fn from(column: PatternFieldColumn) -> Self {
        column.0
    }
}

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;

    use crate::db::activities::ActivitiesRepo;
    use crate::db::category_rules::CategoryRulesRepo;
    use crate::db::open_in_memory;

    use super::*;

    #[test]
    fn rule_rows_come_out_parsed() {
        let mut conn = open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000');
             INSERT INTO category_rules (category_id, kind, field, value) VALUES
               (1, 'site', '', 'github.com'), (1, 'pattern', 'title', '*standup*');",
        )
        .unwrap();
        let rules: Vec<(RuleKind, Option<PatternField>)> = conn
            .get_list()
            .unwrap()
            .into_iter()
            .map(|row| (row.kind, row.field))
            .collect();
        assert_eq!(
            rules,
            [(RuleKind::Site, None), (RuleKind::Pattern, Some(PatternField::Title))]
        );
    }

    #[test]
    fn source_kinds_come_out_parsed() {
        let mut conn = open_in_memory();
        conn.batch_execute(
            "INSERT INTO sources (id, kind, key, name) VALUES (1, 'domain', 'github.com', 'github.com');
             INSERT INTO activities (id, source_id, path, title) VALUES (1, 1, '/acme', 'a');",
        )
        .unwrap();
        let identity = conn.get_identity(1).unwrap().unwrap();
        assert_eq!(identity.source_kind, KeyKind::Domain);
    }
}
