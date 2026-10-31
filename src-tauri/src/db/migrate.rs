use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sqlite::SqliteConnection;
use tracing::{info, warn};

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_ai_category.sql"),
    include_str!("migrations/0003_drop_work_sessions.sql"),
    include_str!("migrations/0004_ai_rejected_category.sql"),
    include_str!("migrations/0005_ai_confidence.sql"),
    include_str!("migrations/0006_normalize.sql"),
    include_str!("migrations/0007_activity_category_view.sql"),
    include_str!("migrations/0008_category_rules.sql"),
    include_str!("migrations/0009_activity_embeddings.sql"),
    include_str!("migrations/0010_drop_ai_confidence.sql"),
    include_str!("migrations/0011_session_last_seen.sql"),
    include_str!("migrations/0012_ai_runs.sql"),
];

/// `user_version` the 28 migrations this schema replaced ended at. Such a database already has
/// the squashed schema, so it is stamped as migrated instead of being run again.
const LEGACY_SCHEMA_VERSION: i64 = 28;
const LATEST_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// `PRAGMA application_id` stamped on every database this schema has migrated ("MKRA"). It is
/// what tells a legacy database from one whose own migrations have reached the legacy version.
const MAKORA_APPLICATION_ID: i64 = 0x4D4B_5241;

const BACKUPS_KEPT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbState {
    Legacy,
    Pending { from: i64 },
    Current,
    Unsupported { version: i64 },
}

fn classify_db(version: i64, application_id: i64) -> DbState {
    if version == LEGACY_SCHEMA_VERSION && application_id != MAKORA_APPLICATION_ID {
        return DbState::Legacy;
    }
    match version {
        LATEST_SCHEMA_VERSION => DbState::Current,
        0..LATEST_SCHEMA_VERSION => DbState::Pending { from: version },
        _ => DbState::Unsupported { version },
    }
}

#[derive(QueryableByName)]
struct CountOrValue {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    value: i64,
}

fn read_pragma(conn: &mut SqliteConnection, name: &str) -> Result<i64> {
    Ok(sql_query(format!("SELECT {name} AS value FROM pragma_{name}"))
        .get_result::<CountOrValue>(conn)?
        .value)
}

fn has_tables(conn: &mut SqliteConnection) -> Result<bool> {
    let count = sql_query("SELECT COUNT(*) AS value FROM sqlite_master WHERE type = 'table'")
        .get_result::<CountOrValue>(conn)?
        .value;
    Ok(count > 0)
}

fn set_user_version(conn: &mut SqliteConnection, version: i64) -> Result<()> {
    sql_query(format!("PRAGMA user_version = {version}")).execute(conn)?;
    Ok(())
}

pub fn migrate(conn: &mut SqliteConnection, db_path: &Path) -> Result<()> {
    let version = read_pragma(conn, "user_version")?;
    let from = match classify_db(version, read_pragma(conn, "application_id")?) {
        DbState::Current => return stamp_application_id(conn),
        DbState::Unsupported { version } => bail!(
            "database schema version {version} is not supported; \
             open it with an older release first, or delete it"
        ),
        DbState::Legacy => {
            back_up(conn, db_path, version)?;
            conn.batch_execute("DROP TABLE IF EXISTS settings;")?;
            set_user_version(conn, 1)?;
            1
        }
        DbState::Pending { from } => {
            if has_tables(conn)? {
                back_up(conn, db_path, from)?;
            }
            from
        }
    };
    // `PRAGMA foreign_keys` is a no-op inside a transaction, so it has to be toggled out here.
    // Off because migration 6 drops a table still referenced by a table it just created (the
    // reference resolves once the replacement is renamed into its place); on again once every
    // migration has run.
    conn.batch_execute("PRAGMA foreign_keys = OFF;")?;
    let result = run_pending(conn, from);
    conn.batch_execute("PRAGMA foreign_keys = ON;")?;
    result?;
    check_foreign_keys(conn)?;
    stamp_application_id(conn)
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

#[derive(QueryableByName)]
struct ForeignKeyViolation {
    #[diesel(sql_type = diesel::sql_types::Text)]
    child: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    parent: String,
}

fn check_foreign_keys(conn: &mut SqliteConnection) -> Result<()> {
    let violation = sql_query(r#"SELECT "table" AS child, parent FROM pragma_foreign_key_check LIMIT 1"#)
        .get_result::<ForeignKeyViolation>(conn)
        .optional()
        .context("checking foreign keys after migrating")?;
    if let Some(violation) = violation {
        bail!(
            "the migrated database has rows in {} pointing at missing {} rows; \
             the backup taken before migrating is next to it",
            violation.child,
            violation.parent
        );
    }
    Ok(())
}

fn stamp_application_id(conn: &mut SqliteConnection) -> Result<()> {
    sql_query(format!("PRAGMA application_id = {MAKORA_APPLICATION_ID}")).execute(conn)?;
    Ok(())
}

fn backup_prefix(db_file_name: &str) -> String {
    format!("{db_file_name}.bak-v")
}

fn back_up(conn: &mut SqliteConnection, db_path: &Path, from_version: i64) -> Result<()> {
    let file_name = db_path
        .file_name()
        .context("database path has no file name")?
        .to_string_lossy()
        .into_owned();
    let stamp = Utc::now().format("%Y%m%d%H%M%S");
    let backup = db_path.with_file_name(format!("{}{from_version}-{stamp}", backup_prefix(&file_name)));
    let target = backup.to_string_lossy().replace('\'', "''");
    conn.batch_execute(&format!("VACUUM INTO '{target}'"))
        .with_context(|| format!("backing up the database to {}", backup.display()))?;
    info!(path = %backup.display(), "backed up the database before migrating");
    prune_backups(db_path, &file_name);
    Ok(())
}

fn prune_backups(db_path: &Path, db_file_name: &str) {
    let Some(dir) = db_path.parent() else {
        return;
    };
    let names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect(),
        Err(err) => {
            warn!(error = %err, "could not list old database backups");
            return;
        }
    };
    for name in stale_backups(&names, db_file_name) {
        let path = dir.join(name);
        if let Err(err) = std::fs::remove_file(&path) {
            warn!(error = %err, path = %path.display(), "could not delete an old database backup");
        }
    }
}

/// The backups of `db_file_name` among `names` beyond the newest [`BACKUPS_KEPT`], judged by
/// the timestamp that ends each name.
fn stale_backups<'a>(names: &'a [String], db_file_name: &str) -> Vec<&'a str> {
    let prefix = backup_prefix(db_file_name);
    let is_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mut backups: Vec<(&str, &str)> = names
        .iter()
        .filter_map(|name| {
            let (version, stamp) = name.strip_prefix(&prefix)?.split_once('-')?;
            (is_digits(version) && stamp.len() == 14 && is_digits(stamp)).then_some((stamp, name.as_str()))
        })
        .collect();
    backups.sort_unstable_by(|a, b| b.cmp(a));
    backups.into_iter().skip(BACKUPS_KEPT).map(|(_, name)| name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_version_without_makora_id_is_legacy() {
        assert_eq!(classify_db(LEGACY_SCHEMA_VERSION, 0), DbState::Legacy);
    }

    #[test]
    fn legacy_version_with_makora_id_is_not_legacy() {
        assert_ne!(classify_db(LEGACY_SCHEMA_VERSION, MAKORA_APPLICATION_ID), DbState::Legacy);
    }

    #[test]
    fn older_versions_are_pending() {
        assert_eq!(classify_db(0, 0), DbState::Pending { from: 0 });
        assert_eq!(
            classify_db(LATEST_SCHEMA_VERSION - 1, MAKORA_APPLICATION_ID),
            DbState::Pending { from: LATEST_SCHEMA_VERSION - 1 }
        );
    }

    #[test]
    fn latest_version_is_current_with_or_without_the_id() {
        assert_eq!(classify_db(LATEST_SCHEMA_VERSION, 0), DbState::Current);
        assert_eq!(classify_db(LATEST_SCHEMA_VERSION, MAKORA_APPLICATION_ID), DbState::Current);
    }

    #[test]
    fn newer_or_negative_versions_are_unsupported() {
        assert_eq!(
            classify_db(LATEST_SCHEMA_VERSION + 1, MAKORA_APPLICATION_ID),
            DbState::Unsupported { version: LATEST_SCHEMA_VERSION + 1 }
        );
        assert_eq!(classify_db(-1, 0), DbState::Unsupported { version: -1 });
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn keeps_the_newest_three_backups_by_timestamp() {
        let names = names(&[
            "makora.sqlite.bak-v9-20260101000000",
            "makora.sqlite.bak-v10-20260301000000",
            "makora.sqlite.bak-v2-20250101000000",
            "makora.sqlite.bak-v10-20260201000000",
            "makora.sqlite.bak-v11-20260401000000",
        ]);
        let mut stale = stale_backups(&names, "makora.sqlite");
        stale.sort_unstable();
        assert_eq!(
            stale,
            ["makora.sqlite.bak-v2-20250101000000", "makora.sqlite.bak-v9-20260101000000"]
        );
    }

    #[test]
    fn ignores_files_that_are_not_backups() {
        let names = names(&[
            "makora.sqlite",
            "makora.sqlite-wal",
            "makora.sqlite.bak-v1-2026",
            "makora.sqlite.bak-vx-20260101000000",
            "other.sqlite.bak-v1-20200101000000",
            "makora.sqlite.bak-v1-20210101000000",
            "makora.sqlite.bak-v1-20220101000000",
            "makora.sqlite.bak-v1-20230101000000",
        ]);
        assert!(stale_backups(&names, "makora.sqlite").is_empty());
    }
}
