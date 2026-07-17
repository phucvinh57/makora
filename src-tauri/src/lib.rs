mod commands;
mod core;
mod ingest;
mod tray;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use chrono::Utc;
use directories::ProjectDirs;
use tauri::{Manager, WindowEvent};

use crate::core::{
    open_connection, CategoryStore, FocusEvent, FocusWatcher, HistoryReader, IgnoreStore,
    PlatformFocusWatcher, QueryWhitelistStore, SessionTracker, SqliteStore,
};

/// Shared by the focus-watcher thread, the tab-ingest server, and the
/// shutdown handler.
type SharedTracker = Arc<Mutex<SessionTracker<SqliteStore>>>;

pub fn run() {
    let db_path = database_path().expect("resolving application data directory");
    eprintln!("makora: writing sessions to {}", db_path.display());

    // One SQLite connection shared by the tracker's write path, the UI's
    // read-only queries, and category edits — SQLite serializes writes at
    // the file level regardless of connection count, so separate
    // connections here would only add the risk of a `SQLITE_BUSY` error
    // when two of them write at once, not any real concurrency.
    let conn = open_connection(&db_path).expect("opening database");
    let ignore = Arc::new(IgnoreStore::new(conn.clone()));
    let whitelist = Arc::new(QueryWhitelistStore::new(conn.clone()));
    let store = SqliteStore::new(conn.clone());
    let tracker: SharedTracker = Arc::new(Mutex::new(
        SessionTracker::with_ignore_policy(store, ignore.clone())
            .expect("initializing session tracker"),
    ));
    let reader = HistoryReader::new(conn.clone());
    let categories = CategoryStore::new(conn);

    let setup_tracker = tracker.clone();
    let setup_whitelist = whitelist.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(tracker)
        .manage(reader)
        .manage(categories)
        .manage(ignore)
        .manage(whitelist)
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::activity_stats,
            commands::list_categories,
            commands::set_session_category,
            commands::set_session_row_category,
            commands::preview_session_category,
            commands::set_sessions_category,
            commands::list_recategorization_keys,
            commands::recategorize_keys,
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::list_category_rules,
            commands::create_category_rule,
            commands::delete_category_rule,
            commands::list_ignore_rules,
            commands::create_ignore_rule,
            commands::delete_ignore_rule,
            commands::list_query_whitelist,
            commands::create_query_whitelist_entry,
            commands::delete_query_whitelist_entry,
        ])
        .setup(move |app| {
            let (tx, rx) = mpsc::channel::<FocusEvent>();
            let mut watcher = PlatformFocusWatcher::new();
            watcher.start(tx).context("starting focus watcher")?;

            let event_tracker = setup_tracker.clone();
            std::thread::Builder::new()
                .name("makora-focus-tracker".into())
                .spawn(move || {
                    for event in rx {
                        if let Err(err) = event_tracker.lock().unwrap().handle_event(event) {
                            eprintln!("makora: failed to record session: {err:#}");
                        }
                    }
                })
                .context("spawning focus-tracker thread")?;

            ingest::spawn(setup_tracker.clone(), setup_whitelist.clone())
                .context("starting tab-ingest server")?;
            eprintln!(
                "makora: listening for tab events on http://127.0.0.1:{}",
                ingest::PORT
            );

            tray::build(app.handle())?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Close (and minimize) hide the window instead of quitting — the
            // watcher and ingest server keep running in the background; the
            // tray icon is the only way back to the window from here on.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the makora application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let tracker = app_handle.state::<SharedTracker>();
                let result = tracker.lock().unwrap().close_current(Utc::now());
                if let Err(err) = result {
                    eprintln!("makora: failed to close session on shutdown: {err:#}");
                }
            }
        });
}

/// `%APPDATA%\makora\data\data.db` on Windows, `~/Library/Application
/// Support/dev.makora.makora/data.db` once macOS support lands — resolved
/// through `directories` so the app never hardcodes an OS-specific path.
fn database_path() -> anyhow::Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("dev", "phucvinh57", "makora")
        .context("could not resolve a per-user application data directory")?;
    Ok(dirs.data_dir().join("makora.sqlite"))
}
