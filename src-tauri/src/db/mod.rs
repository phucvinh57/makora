//! Repository layer: typed Diesel access to the SQLite database, and nothing else. Every
//! function here reads or writes rows; the rules for *when* to read or write them (merging
//! sessions, pinning precedence, the review queue, …) live in [`crate::domain`].

pub mod activities;
pub mod ai_labels;
pub mod categories;
pub mod history;
mod migrate;
pub mod models;
pub mod review;
pub mod schema;
pub mod sessions;
mod sql;
pub mod sources;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use diesel::connection::SimpleConnection;
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

/// The one connection to the database, guarded by a mutex like the rest of Makora's shared
/// state. SQLite only allows one writer at a time, so there is nothing to gain from a pool.
pub type Db = Arc<Mutex<SqliteConnection>>;

pub fn open(path: &Path) -> Result<Db> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating database directory {}", parent.display()))?;
    }
    let mut conn = SqliteConnection::establish(&path.to_string_lossy())
        .with_context(|| format!("opening database at {}", path.display()))?;
    migrate::migrate(&mut conn).context("migrating schema")?;
    conn.batch_execute("PRAGMA foreign_keys = ON;")
        .context("enabling foreign keys")?;
    Ok(Arc::new(Mutex::new(conn)))
}
