pub mod ai;
pub mod app_core;
pub mod categorization;
pub mod db;
pub mod domain;
pub mod events;
mod host;
pub mod logging;
pub mod logs;
pub mod paths;
pub mod reporting;
pub mod settings;
pub mod shared;
pub mod taxonomy;
pub mod tracking;

use std::sync::Arc;

use tauri::Manager;
use tracing::warn;

use crate::app_core::AppCore;
use crate::host::events::TauriEvents;
use crate::paths::AppPaths;

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
            host::commands::reporting::ai_usage_runs,
            host::commands::reporting::ai_usage_summary,
            host::commands::taxonomy::category_list,
            host::commands::taxonomy::category_create,
            host::commands::taxonomy::category_update,
            host::commands::taxonomy::category_delete,
            host::commands::categorization::session_set_row_category,
            host::commands::categorization::session_set_category,
            host::commands::categorization::session_undo_category,
            host::commands::categorization::activity_set_category,
            host::commands::categorization::ai_review_accept,
            host::commands::categorization::categorization_reset,
            host::commands::categorization::categorization_run,
            host::commands::categorization::categorization_status,
            host::commands::categorization::categorization_next_auto_run,
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
            host::commands::providers::ai_provider_save,
            host::commands::providers::ai_provider_remove,
            host::commands::providers::ai_provider_has_key,
            host::commands::providers::ai_provider_list_models,
            host::commands::settings::settings_get,
            host::commands::settings::settings_take_load_issue,
            host::commands::settings::settings_set_min_session_seconds,
            host::commands::settings::settings_add_exclusion,
            host::commands::settings::settings_remove_exclusion,
            host::commands::settings::settings_set_llm,
            host::commands::settings::settings_set_embedding_model,
            host::commands::settings::settings_set_auto_categorize_hours,
            host::commands::settings::settings_set_model_price,
            host::commands::settings::settings_remove_model_price,
            host::commands::logs::logs_query,
            host::commands::logs::logs_network_summary,
            host::commands::logs::logs_dir,
            host::commands::logs::logs_open_dir,
        ])
        .setup(|app| {
            let tauri_paths = AppPaths {
                data_dir: app.path().app_data_dir()?,
                config_dir: app.path().app_config_dir()?,
            };
            let platform = AppPaths::platform()?;
            let paths = if platform == tauri_paths { AppPaths::resolve()? } else { tauri_paths.clone() };
            if let Err(err) = logging::init(&paths.data_dir) {
                eprintln!("makora: failed to start logging: {err:#}");
            }
            if platform != tauri_paths {
                warn!(
                    tauri = ?tauri_paths,
                    platform = ?platform,
                    "the platform app directories differ from Tauri's; using Tauri's"
                );
            }

            let core = AppCore::open(paths, Arc::new(TauriEvents::new(app.handle().clone())))?;
            app.manage(core.clone());
            core.start_background()?;
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
                if let Some(core) = app_handle.try_state::<Arc<AppCore>>() {
                    core.shutdown();
                }
            }
        });
}
