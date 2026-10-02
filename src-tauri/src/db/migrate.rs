use anyhow::{bail, Context, Result};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sqlite::SqliteConnection;

/// The SQL run to reach each schema version, in order. Index 0 takes a fresh database to
/// version 1, index 1 to version 2, and so on.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_ai_category.sql"),
    include_str!("migrations/0003_drop_work_sessions.sql"),
    include_str!("migrations/0004_ai_rejected_category.sql"),
    include_str!("migrations/0005_ai_confidence.sql"),
    include_str!("migrations/0006_normalize.sql"),
    include_str!("migrations/0007_activity_category_view.sql"),
    include_str!("migrations/0008_category_rules.sql"),
];

/// `user_version` the 28 migrations this schema replaced ended at. Such a database already has
/// the squashed schema, so it is stamped as migrated instead of being run again.
const LEGACY_SCHEMA_VERSION: i64 = 28;
const LATEST_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

#[derive(QueryableByName)]
struct UserVersion {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    user_version: i64,
}

fn user_version(conn: &mut SqliteConnection) -> Result<i64> {
    Ok(sql_query("PRAGMA user_version")
        .get_result::<UserVersion>(conn)?
        .user_version)
}

fn set_user_version(conn: &mut SqliteConnection, version: i64) -> Result<()> {
    sql_query(format!("PRAGMA user_version = {version}")).execute(conn)?;
    Ok(())
}

pub fn migrate(conn: &mut SqliteConnection) -> Result<()> {
    let mut version = user_version(conn)?;
    if version == LEGACY_SCHEMA_VERSION {
        conn.batch_execute("DROP TABLE IF EXISTS settings;")?;
        set_user_version(conn, 1)?;
        version = 1;
    } else if version > LATEST_SCHEMA_VERSION {
        bail!(
            "database schema version {version} is not supported; \
             open it with an older release first, or delete it"
        );
    }
    // `PRAGMA foreign_keys` is a no-op inside a transaction, so it has to be toggled out here.
    // Off because migration 6 drops a table still referenced by a table it just created (the
    // reference resolves once the replacement is renamed into its place); on again once every
    // migration has run.
    conn.batch_execute("PRAGMA foreign_keys = OFF;")?;
    let result = run_pending(conn, version);
    conn.batch_execute("PRAGMA foreign_keys = ON;")?;
    result
}

fn run_pending(conn: &mut SqliteConnection, from_version: i64) -> Result<()> {
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(from_version as usize) {
        let target = index as i64 + 1;
        conn.transaction(|conn| {
            conn.batch_execute(sql)?;
            set_user_version(conn, target)
        })
        .with_context(|| format!("running migration {target}"))?;
    }
    Ok(())
}
