//! Binding a `?`-placeholder count that is only known at runtime is the one thing Diesel's
//! typed query builder cannot do, so [`history`](super::history) and [`review`](super::review)
//! fall back to boxed raw SQL for it. This is the shared plumbing for that.

use diesel::sql_types::{BigInt, Double, Text};
use diesel::sqlite::Sqlite;

pub(super) type BoxedQuery<'a> = diesel::query_builder::BoxedSqlQuery<'a, Sqlite, diesel::query_builder::SqlQuery>;

#[derive(Clone)]
pub(super) enum Param {
    Text(String),
    I64(i64),
    F64(f64),
}

pub(super) fn bind_all(mut query: BoxedQuery<'static>, binds: Vec<Param>) -> BoxedQuery<'static> {
    for param in binds {
        query = match param {
            Param::Text(v) => query.bind::<Text, _>(v),
            Param::I64(v) => query.bind::<BigInt, _>(v),
            Param::F64(v) => query.bind::<Double, _>(v),
        };
    }
    query
}

/// `col IN (id1, id2, …)` with its binds, or the given fallback clause when `ids` is empty
/// (`IN ()` is a syntax error in SQLite).
pub(super) fn in_clause(column: &str, ids: &[i64], empty_clause: &str) -> (String, Vec<Param>) {
    if ids.is_empty() {
        return (empty_clause.to_string(), Vec::new());
    }
    let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
    (
        format!("{column} IN ({placeholders})"),
        ids.iter().copied().map(Param::I64).collect(),
    )
}
