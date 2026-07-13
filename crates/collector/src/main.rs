use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::Utc;
use directories::ProjectDirs;

use makora_core::{FocusEvent, FocusWatcher, PlatformFocusWatcher, SessionTracker, SqliteStore};

mod ingest;

fn main() -> Result<()> {
    let db_path = database_path()?;
    eprintln!("makora: writing sessions to {}", db_path.display());

    let store = SqliteStore::open(&db_path)?;
    let tracker = Arc::new(Mutex::new(SessionTracker::new(store)?));

    let (tx, rx) = mpsc::channel::<FocusEvent>();
    let mut watcher = PlatformFocusWatcher::new();
    watcher.start(tx).context("starting focus watcher")?;

    ingest::spawn(tracker.clone()).context("starting tab-ingest server")?;
    eprintln!("makora: listening for tab events on http://127.0.0.1:{}", ingest::PORT);

    set_shutdown_handler(tracker.clone())?;

    for event in rx {
        if let Err(err) = tracker.lock().unwrap().handle_event(event) {
            eprintln!("makora: failed to record session: {err:#}");
        }
    }

    Ok(())
}

/// `%APPDATA%\makora\data\data.db` on Windows, `~/Library/Application
/// Support/dev.makora.makora/data.db` once macOS support lands — resolved
/// through `directories` so the collector never hardcodes an OS-specific path.
fn database_path() -> Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("dev", "phucvinh57", "makora")
        .context("could not resolve a per-user application data directory")?;
    Ok(dirs.data_dir().join("data.db"))
}

/// On Ctrl+C (or the equivalent console close event on Windows), close
/// whatever session is currently open so its `end_time` reflects reality
/// instead of being recovered later as a dangling, zero-length session.
fn set_shutdown_handler(tracker: Arc<Mutex<SessionTracker<SqliteStore>>>) -> Result<()> {
    ctrlc::set_handler(move || {
        if let Err(err) = tracker.lock().unwrap().close_current(Utc::now()) {
            eprintln!("makora: failed to close session on shutdown: {err:#}");
        }
        std::process::exit(0);
    })
    .context("installing shutdown handler")
}
