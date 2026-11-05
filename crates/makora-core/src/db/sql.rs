//! Binding a `?`-placeholder count that is only known at runtime is the one thing Diesel's
//! typed query builder cannot do, so the [`reporting`](crate::reporting) read models fall back
//! to boxed raw SQL for it. This is the shared plumbing for that.

use diesel::sql_types::{BigInt, Text};
use diesel::sqlite::{Sqlite, SqliteConnection};
use diesel::{sql_query, QueryResult, QueryableByName, RunQueryDsl};

pub(crate) type BoxedQuery<'a> = diesel::query_builder::BoxedSqlQuery<'a, Sqlite, diesel::query_builder::SqlQuery>;

#[derive(Clone)]
pub(crate) enum Param {
    Text(String),
    I64(i64),
}

pub(crate) fn bind_all(mut query: BoxedQuery<'static>, binds: Vec<Param>) -> BoxedQuery<'static> {
    for param in binds {
        query = match param {
            Param::Text(v) => query.bind::<Text, _>(v),
            Param::I64(v) => query.bind::<BigInt, _>(v),
        };
    }
    query
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

/// Runs `sql`, a `SELECT COUNT(*) AS count …`, with `binds`.
pub(crate) fn count(conn: &mut SqliteConnection, sql: String, binds: Vec<Param>) -> QueryResult<i64> {
    Ok(bind_all(sql_query(sql).into_boxed(), binds).get_result::<CountRow>(conn)?.count)
}

/// `col IN (id1, id2, …)` with its binds, or the given fallback clause when `ids` is empty
/// (`IN ()` is a syntax error in SQLite).
pub(crate) fn in_clause(column: &str, ids: &[i64], empty_clause: &str) -> (String, Vec<Param>) {
    if ids.is_empty() {
        return (empty_clause.to_string(), Vec::new());
    }
    let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
    (
        format!("{column} IN ({placeholders})"),
        ids.iter().copied().map(Param::I64).collect(),
    )
}
