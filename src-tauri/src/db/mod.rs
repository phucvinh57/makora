//! Repository layer: typed Diesel access to the SQLite database, and nothing else. Every
//! function here reads or writes rows; the rules for *when* to read or write them (merging
//! sessions, pinning precedence, …) live in [`crate::domain`] and [`crate::categorization`].
//! The read models behind the history and review pages are in [`crate::reporting`].

pub mod activities;
pub mod ai_labels;
pub mod categories;
mod migrate;
pub mod models;
pub mod schema;
pub mod sessions;
pub(crate) mod sql;
pub mod sources;
mod traits;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use diesel::connection::SimpleConnection;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

/// How long a connection waits on a lock held by another one before giving up.
const BUSY_TIMEOUT_MS: u32 = 5000;
/// Readers alive at once. Each is a cheap SQLite handle, and a read never waits on the writer.
const MAX_READERS: u32 = 4;

type ReadPool = Pool<ConnectionManager<SqliteConnection>>;
pub type ReadConn = PooledConnection<ConnectionManager<SqliteConnection>>;

/// The database: one write connection, serialized by a mutex because SQLite allows a single
/// writer, and a pool of read-only connections. The file is in WAL mode, so readers see a
/// consistent snapshot and neither wait on the writer nor on each other.
#[derive(Clone)]
pub struct Db {
    writer: Arc<Mutex<SqliteConnection>>,
    readers: ReadPool,
}

impl Db {
    /// The write connection. Holding it blocks other writers, never readers.
    pub fn write(&self) -> MutexGuard<'_, SqliteConnection> {
        self.writer.lock().unwrap()
    }

    /// A read-only connection from the pool. Writing through it fails.
    pub fn read(&self) -> Result<ReadConn> {
        self.readers.get().context("getting a read connection")
    }
}

/// Puts each pooled connection into read-only mode.
#[derive(Debug)]
struct ReaderSetup;

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ReaderSetup {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        conn.batch_execute(&format!(
            "PRAGMA busy_timeout = {BUSY_TIMEOUT_MS}; PRAGMA foreign_keys = ON; PRAGMA query_only = ON;"
        ))
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

pub fn open_sqlite(path: &Path) -> Result<Db> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating database directory {}", parent.display()))?;
    }
    let url = path.to_string_lossy();
    let mut conn = SqliteConnection::establish(&url)
        .with_context(|| format!("opening database at {}", path.display()))?;
    // WAL is stored in the file, so this is what lets the readers below run beside the writer.
    conn.batch_execute(&format!(
        "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = {BUSY_TIMEOUT_MS};"
    ))
    .context("enabling WAL")?;
    migrate::migrate(&mut conn).context("migrating schema")?;
    conn.batch_execute("PRAGMA foreign_keys = ON;")
        .context("enabling foreign keys")?;

    // Built after the migrations so no reader ever sees a half-migrated schema.
    let readers = Pool::builder()
        .max_size(MAX_READERS)
        .connection_customizer(Box::new(ReaderSetup))
        .build(ConnectionManager::<SqliteConnection>::new(url.as_ref()))
        .context("opening read connections")?;
    Ok(Db {
        writer: Arc::new(Mutex::new(conn)),
        readers,
    })
}
