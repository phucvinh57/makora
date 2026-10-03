mod ai;
mod categorization;
mod db;
mod domain;
mod host;
mod logging;
mod reporting;
mod settings;
mod shared;
mod taxonomy;
mod tracking;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use chrono::Utc;
use tauri::Manager;
use tracing::{error, info};

use crate::ai::embedding::LlamaEmbedder;
use crate::ai::llm::{LlamaClassifier, ModelCache};
use crate::ai::model_download::ModelDownloadState;
use crate::categorization::application::ai_run::AiRunState;
use crate::categorization::ports::{Classifier, Embedder};
use crate::categorization::{
    ActivityIndexer, BackgroundIndexer, ClassificationSvc, EmbeddingSvc, RuleSvc, SqliteClassification,
    SqliteEmbeddings, SqliteRules,
};
use crate::settings::{SettingsStore, TomlSettings};
use crate::domain::{HistorySvc, LabelingSvc, SqliteHistory, SqliteLabeling};
use crate::taxonomy::{CategorySvc, SqliteCategories};
use crate::tracking::adapters::model::WatchEvent;
use crate::tracking::adapters::platform::PlatformFocusWatcher;
use crate::tracking::adapters::watcher::FocusWatcher;
use crate::tracking::application::SqliteSessionRecorder;
use crate::tracking::domain::focus_machine::FocusStateMachine;
use crate::tracking::ports::{SessionRecorder, SessionTracker};

type SharedTracker = Arc<Mutex<Box<dyn SessionTracker>>>;

pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            host::tray::show_main_window(app);
        }));

    if !cfg!(debug_assertions) {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

    builder
        .invoke_handler(tauri::generate_handler![
            host::commands::reporting::session_list,
            host::commands::reporting::activity_stats,
            host::commands::reporting::ai_review_list,
            host::commands::reporting::ai_review_list_groups,
            host::commands::reporting::ai_review_feedback_stats,
            host::commands::taxonomy::category_list,
            host::commands::taxonomy::category_create,
            host::commands::taxonomy::category_update,
            host::commands::taxonomy::category_delete,
            host::commands::categorization::session_set_row_category,
            host::commands::categorization::session_set_category,
            host::commands::categorization::activity_set_category,
            host::commands::categorization::ai_review_accept,
            host::commands::categorization::categorization_reset,
            host::commands::categorization::categorization_run,
            host::commands::categorization::categorization_status,
            host::commands::categorization::categorization_cancel,
            host::commands::categorization::category_rule_list,
            host::commands::categorization::category_rule_create,
            host::commands::categorization::category_rule_update,
            host::commands::categorization::category_rule_delete,
            host::commands::categorization::category_rule_preview,
            host::commands::categorization::suggestion_sources,
            host::commands::categorization::suggestion_paths,
            host::commands::models::ai_model_is_loaded,
            host::commands::models::ai_model_unload,
            host::commands::models::ai_model_download,
            host::commands::models::ai_model_download_status,
            host::commands::models::ai_model_cancel_download,
            host::commands::models::ai_model_is_installed,
            host::commands::models::ai_model_list_installed,
            host::commands::settings::settings_get,
            host::commands::settings::settings_set_min_session_seconds,
            host::commands::settings::settings_set_llm_model,
            host::commands::settings::settings_set_embedding_model,
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

            let settings_path = app.path().app_config_dir()?.join("settings.toml");
            info!(path = %settings_path.display(), "reading settings");
            let settings: Arc<dyn SettingsStore> = Arc::new(TomlSettings::load(settings_path));

            let models = ModelCache::default();
            let embedder: Arc<dyn Embedder> = Arc::new(LlamaEmbedder::new(models.clone()));
            let embeddings: Arc<dyn EmbeddingSvc> = Arc::new(SqliteEmbeddings::new(conn.clone()));
            let indexer: Arc<dyn ActivityIndexer> = Arc::new(
                BackgroundIndexer::spawn(embeddings.clone(), embedder.clone(), settings.clone())
                    .context("spawning activity-embedder thread")?,
            );
            // Activities recorded before they were embedded as they were saved.
            indexer.refresh();

            let recorder: Box<dyn SessionRecorder> =
                Box::new(SqliteSessionRecorder::new(conn.clone(), settings.clone(), indexer.clone()));
            let session_tracker = FocusStateMachine::new(recorder).context("initializing session tracker")?;
            let tracker: SharedTracker = Arc::new(Mutex::new(Box::new(session_tracker) as Box<dyn SessionTracker>));
            ai::llm::set_download_root(data_dir.join("models"));

            app.manage(Box::new(SqliteHistory::new(conn.clone())) as Box<dyn HistorySvc>);
            app.manage(Box::new(SqliteCategories::new(conn.clone())) as Box<dyn CategorySvc>);
            app.manage(Box::new(SqliteClassification::new(conn.clone())) as Box<dyn ClassificationSvc>);
            app.manage(Box::new(SqliteRules::new(conn.clone())) as Box<dyn RuleSvc>);
            app.manage(Box::new(SqliteLabeling::new(conn)) as Box<dyn LabelingSvc>);
            app.manage(settings);
            app.manage(AiRunState::default());
            app.manage(ModelDownloadState::default());
            app.manage(Box::new(LlamaClassifier::new(models.clone())) as Box<dyn Classifier>);
            app.manage(embedder);
            app.manage(embeddings);
            app.manage(indexer);
            app.manage(models);
            app.manage(tracker.clone());
            let (tx, rx) = mpsc::channel::<WatchEvent>();

            #[cfg(target_os = "linux")]
            crate::tracking::adapters::platform::start_presence_watcher(tx.clone())
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
                                event_tracker.lock().unwrap().handle_focus(event.into())
                            }
                            WatchEvent::Presence(event) => {
                                event_tracker.lock().unwrap().handle_presence(event.into())
                            }
                        };
                        if let Err(err) = result {
                            error!(error = format!("{err:#}"), "failed to record session");
                        }
                    }
                })
                .context("spawning activity-tracker thread")?;

            tracking::adapters::ingest::spawn(tracker).context("starting tab-ingest server")?;
            info!(port = tracking::adapters::ingest::PORT, "listening for tab events");

            host::tray::build(app.handle())?;

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
