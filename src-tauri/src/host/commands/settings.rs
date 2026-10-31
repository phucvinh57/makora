use std::sync::Arc;

use chrono::Utc;
use tauri::State;

use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::ActivityIndexer;
use crate::host::auto_categorize::ScheduleAnchor;
use crate::host::error::IpcError;
use crate::settings::{Settings, SettingsStore};
use crate::tracking::domain::exclusion::Exclusion;

#[tauri::command]
pub fn settings_get(settings: State<Arc<dyn SettingsStore>>) -> Settings {
    settings.get()
}

/// Why the settings file could not be used at startup, once; `None` after that or when it could.
#[tauri::command]
pub fn settings_take_load_issue(settings: State<Arc<dyn SettingsStore>>) -> Option<String> {
    settings.take_load_issue()
}

#[tauri::command]
pub fn settings_set_min_session_seconds(settings: State<Arc<dyn SettingsStore>>, seconds: i64) -> Result<Settings, IpcError> {
    Ok(settings.set_min_session_seconds(seconds)?)
}

#[tauri::command]
pub fn settings_add_exclusion(
    settings: State<Arc<dyn SettingsStore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<Settings, IpcError> {
    Ok(settings.add_exclusion(parse_exclusion(kind, field, &value)?)?)
}

#[tauri::command]
pub fn settings_remove_exclusion(
    settings: State<Arc<dyn SettingsStore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<Settings, IpcError> {
    Ok(settings.remove_exclusion(&parse_exclusion(kind, field, &value)?)?)
}

/// The exclusion in the normalized form it is stored in, so removing matches what adding saved.
fn parse_exclusion(kind: RuleKind, field: Option<PatternField>, value: &str) -> Result<Exclusion, IpcError> {
    Exclusion::parse(kind, field, value).map_err(|err| IpcError::new("invalid_rule", err.to_string()))
}

/// Sets the LLM to `model` on the remote `provider`, or on the local runtime when it is left out.
#[tauri::command]
pub fn settings_set_llm(
    settings: State<Arc<dyn SettingsStore>>,
    provider: Option<String>,
    model: String,
) -> Result<Settings, IpcError> {
    Ok(settings.set_llm(provider.as_deref(), &model)?)
}

#[tauri::command]
pub fn settings_set_embedding_model(
    settings: State<Arc<dyn SettingsStore>>,
    indexer: State<Arc<dyn ActivityIndexer>>,
    model: String,
) -> Result<Settings, IpcError> {
    let settings = settings.set_embedding_model(&model)?;
    indexer.refresh();
    Ok(settings)
}

/// Setting the period restarts the schedule: the first run comes one period from now, on the hour.
#[tauri::command]
pub fn settings_set_auto_categorize_hours(
    settings: State<Arc<dyn SettingsStore>>,
    anchor: State<ScheduleAnchor>,
    hours: u32,
) -> Result<Settings, IpcError> {
    let saved = settings.set_auto_categorize_hours(hours)?;
    anchor.set(Utc::now());
    Ok(saved)
}
