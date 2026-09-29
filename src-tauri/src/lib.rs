mod ai;
mod category;
mod commands;
mod db;
mod domain;
mod focus;
mod logging;
mod tray;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use chrono::Utc;
use tauri::Manager;
use tracing::{error, info};

use crate::domain::{CategorySvc, HistorySvc, LabelingSvc, SqliteCategories, SqliteHistory, SqliteLabeling, SqliteTracking};
use crate::focus::{ActivitySessionTracker, FocusWatcher, PlatformFocusWatcher, SessionTracker, WatchEvent};

type SharedTracker = Arc<Mutex<Box<dyn SessionTracker>>>;

pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }));

    // Do not autostart on dev mode
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
            commands::create_category,
            commands::update_category,
            commands::delete_category,
            commands::reset_categorization,
            commands::categorize,
            commands::categorize_status,
            commands::cancel_categorize,
            commands::ai_model_loaded,
            commands::unload_ai_model,
            commands::download_ai_model,
            commands::model_download_status,
            commands::cancel_model_download,
            commands::ai_model_installed,
            commands::list_installed_ai_models,
            commands::list_ai_review,
            commands::list_ai_review_groups,
            commands::accept_ai_labels,
            commands::set_activities_category,
            commands::ai_feedback_stats,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            match logging::init(&data_dir) {
                Ok(Some(dir)) => info!(directory = %dir.display(), "writing logs"),
                Ok(None) => {}
                Err(err) => eprintln!("makora: failed to start logging: {err:#}"),
            }

            let db_path = data_dir.join("makora.sqlite");
            info!(path = %db_path.display(), "writing sessions");

            let conn = db::open_sqlite(&db_path).map_err(|err| {
                anyhow::anyhow!("opening database at {}: {err:#}", db_path.display())
            })?;
            let tracking: Box<dyn domain::TrackingSvc> = Box::new(SqliteTracking::new(conn.clone()));
            let session_tracker = ActivitySessionTracker::new(tracking).context("initializing session tracker")?;
            let tracker: SharedTracker = Arc::new(Mutex::new(Box::new(session_tracker) as Box<dyn SessionTracker>));
            ai::llm::set_download_root(data_dir.join("models"));

            app.manage(Box::new(SqliteHistory::new(conn.clone())) as Box<dyn HistorySvc>);
            app.manage(Box::new(SqliteCategories::new(conn.clone())) as Box<dyn CategorySvc>);
            app.manage(Box::new(SqliteLabeling::new(conn)) as Box<dyn LabelingSvc>);
            app.manage(commands::AiCategorizeState::default());
            app.manage(commands::ModelDownloadState::default());
            app.manage(ai::llm::ModelCache::default());
            app.manage(tracker.clone());
            let (tx, rx) = mpsc::channel::<WatchEvent>();

            #[cfg(target_os = "linux")]
            crate::focus::platform::start_presence_watcher(tx.clone())
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
                            error!(error = format!("{err:#}"), "failed to record session");
                        }
                    }
                })
                .context("spawning activity-tracker thread")?;

            focus::ingest::spawn(tracker).context("starting tab-ingest server")?;
            info!(port = focus::ingest::PORT, "listening for tab events");

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
                        error!(
                            error = format!("{err:#}"),
                            "failed to close session on shutdown"
                        );
                    }
                }
            }
        });
}
