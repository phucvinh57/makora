use std::sync::Arc;

use makora_lib::app_core::AppCore;
use makora_lib::categorization::domain::rules::{PatternField, RuleKind};
use makora_lib::settings::{ModelPrice, Settings};
use tauri::State;

use crate::host::error::IpcError;

#[tauri::command]
pub fn settings_get(core: State<Arc<AppCore>>) -> Settings {
    core.settings.get()
}

/// Why the settings file could not be used at startup, once; `None` after that or when it could.
#[tauri::command]
pub fn settings_take_load_issue(core: State<Arc<AppCore>>) -> Option<String> {
    core.settings.take_load_issue()
}

#[tauri::command]
pub fn settings_set_min_session_seconds(core: State<Arc<AppCore>>, seconds: i64) -> Result<Settings, IpcError> {
    Ok(core.settings.set_min_session_seconds(seconds)?)
}

#[tauri::command]
pub fn settings_add_exclusion(
    core: State<Arc<AppCore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<Settings, IpcError> {
    Ok(core.settings.add_exclusion(kind, field, &value)?)
}

#[tauri::command]
pub fn settings_remove_exclusion(
    core: State<Arc<AppCore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<Settings, IpcError> {
    Ok(core.settings.remove_exclusion(kind, field, &value)?)
}

/// Sets the LLM to `model` on the remote `provider`, or on the local runtime when it is left out.
#[tauri::command]
pub fn settings_set_llm(
    core: State<Arc<AppCore>>,
    provider: Option<String>,
    model: String,
) -> Result<Settings, IpcError> {
    Ok(core.settings.set_llm(provider.as_deref(), &model)?)
}

#[tauri::command]
pub fn settings_set_embedding_model(core: State<Arc<AppCore>>, model: String) -> Result<Settings, IpcError> {
    Ok(core.settings.set_embedding_model(&model)?)
}

#[tauri::command]
pub fn settings_set_auto_categorize_hours(core: State<Arc<AppCore>>, hours: u32) -> Result<Settings, IpcError> {
    Ok(core.settings.set_auto_categorize_hours(hours)?)
}

#[tauri::command]
pub fn settings_set_model_price(core: State<Arc<AppCore>>, price: ModelPrice) -> Result<Settings, IpcError> {
    Ok(core.settings.set_model_price(price)?)
}

#[tauri::command]
pub fn settings_remove_model_price(
    core: State<Arc<AppCore>>,
    provider: String,
    model: String,
) -> Result<Settings, IpcError> {
    Ok(core.settings.remove_model_price(&provider, &model)?)
}
