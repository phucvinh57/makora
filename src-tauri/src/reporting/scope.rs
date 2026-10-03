use crate::db::sql::{in_clause, Param};

pub enum CategoryScope {
    Any,
    Uncategorized,
    /// Already expanded to include every descendant; an empty list matches nothing.
    Ids(Vec<i64>),
}

impl CategoryScope {
    /// The condition on `column` (the resolved category id) and its binds; `None` when the scope
    /// does not narrow anything.
    pub(super) fn clause(&self, column: &str) -> Option<(String, Vec<Param>)> {
        match self {
            Self::Any => None,
            Self::Uncategorized => Some((format!("{column} IS NULL"), Vec::new())),
            Self::Ids(ids) => Some(in_clause(column, ids, "0=1")),
        }
    }
}
