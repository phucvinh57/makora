mod commands;
mod core;
mod ingest;
mod tray;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use chrono::Utc;
use tauri::Manager;

use crate::core::{
    open_connection, CategoryStore, FocusWatcher, HistoryReader, IgnoreStore, PlatformFocusWatcher,
    SessionTracker, SqliteStore, WatchEvent, WorkSessionStore,
};

type SharedTracker = Arc<Mutex<SessionTracker<SqliteStore>>>;

pub fn run() {
    let mut builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }));

    if !cfg!(debug_assertions) {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

    builder
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::activity_stats,
            commands::list_categories,
            commands::set_session_row_category,
            commands::set_sessions_category,
            commands::recategorize_all,
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::list_category_rules,
            commands::create_category_rule,
            commands::delete_category_rule,
            commands::list_ignore_rules,
            commands::create_ignore_rule,
            commands::delete_ignore_rule,
            commands::create_work_session,
            commands::list_work_sessions,
            commands::search_work_sessions,
            commands::get_work_session,
            commands::update_work_session,
            commands::stop_work_session,
            commands::delete_work_session,
            commands::work_session_stats,
        ])
        .setup(|app| {
            let db_path =
                database_path(app.handle()).context("resolving application data directory")?;
            eprintln!("makora: writing sessions to {}", db_path.display());

            let conn = open_connection(&db_path).context("opening database")?;
            let ignore = Arc::new(IgnoreStore::new(conn.clone()));
            let store = SqliteStore::new(conn.clone());
            let tracker: SharedTracker = Arc::new(Mutex::new(
                SessionTracker::with_ignore_policy(store, ignore.clone())
                    .context("initializing session tracker")?,
            ));

            app.manage(HistoryReader::new(conn.clone()));
            app.manage(WorkSessionStore::new(conn.clone()));
            app.manage(CategoryStore::new(conn));
            app.manage(tracker.clone());
            app.manage(ignore);

            let (tx, rx) = mpsc::channel::<WatchEvent>();

            #[cfg(target_os = "linux")]
            crate::core::platform::start_presence_watcher(tx.clone())
                .context("starting presence watcher")?;

            let mut watcher = PlatformFocusWatcher::new();
            watcher.start(tx).context("starting focus watcher")?;

            let event_tracker = tracker.clone();
            std::thread::Builder::new()
                .name("makora-activity-tracker".into())
                .spawn(move || {
                    for event in rx {
                        let result = match event {
                            WatchEvent::Focus(event) => {
                                event_tracker.lock().unwrap().handle_event(event)
                            }
                            WatchEvent::Presence(event) => {
                                event_tracker.lock().unwrap().handle_presence_event(event)
                            }
                        };
                        if let Err(err) = result {
                            eprintln!("makora: failed to record session: {err:#}");
                        }
                    }
                })
                .context("spawning activity-tracker thread")?;

            ingest::spawn(tracker).context("starting tab-ingest server")?;
            eprintln!(
                "makora: listening for tab events on http://127.0.0.1:{}",
                ingest::PORT
            );

            tray::build(app.handle())?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the makora application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                    return;
                }
                if let Some(tracker) = app_handle.try_state::<SharedTracker>() {
                    let result = tracker.lock().unwrap().close_current(Utc::now());
                    if let Err(err) = result {
                        eprintln!("makora: failed to close session on shutdown: {err:#}");
                    }
                }
            }
        });
}

fn database_path(app: &tauri::AppHandle) -> anyhow::Result<std::path::PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("could not resolve a per-user application data directory")?;
    Ok(dir.join("makora.sqlite"))
}
