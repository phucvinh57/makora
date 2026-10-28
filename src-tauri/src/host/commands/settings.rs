use std::sync::Arc;

use chrono::Utc;
use tauri::State;

use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::ActivityIndexer;
use crate::host::auto_categorize::ScheduleAnchor;
use crate::host::error::IpcError;
use crate::settings::{Settings, SettingsStore, AUTO_CATEGORIZE_HOURS_OPTIONS};
use crate::tracking::domain::exclusion::Exclusion;
use crate::tracking::domain::session_policy::MIN_SESSION_SECONDS_RANGE;

#[tauri::command]
pub fn settings_get(settings: State<Arc<dyn SettingsStore>>) -> Settings {
    settings.get()
}

#[tauri::command]
pub fn settings_set_min_session_seconds(settings: State<Arc<dyn SettingsStore>>, seconds: i64) -> Result<Settings, IpcError> {
    // Checked here as well so a bad value is the caller's error, not an internal one.
    if !MIN_SESSION_SECONDS_RANGE.contains(&seconds) {
        return Err(IpcError::invalid_input(format!(
            "minimum session length must be between {} and {} seconds",
            MIN_SESSION_SECONDS_RANGE.start(),
            MIN_SESSION_SECONDS_RANGE.end()
        )));
    }
    Ok(settings.set_min_session_seconds(seconds)?)
}

#[tauri::command]
pub fn settings_add_exclusion(
    settings: State<Arc<dyn SettingsStore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<Settings, IpcError> {
    let exclusion = Exclusion::parse(kind, field, &value).map_err(|err| IpcError::new("invalid_rule", err.to_string()))?;
    Ok(settings.add_exclusion(exclusion)?)
}

#[tauri::command]
pub fn settings_remove_exclusion(
    settings: State<Arc<dyn SettingsStore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<Settings, IpcError> {
    Ok(settings.remove_exclusion(&Exclusion { kind, field, value })?)
}

/// Sets the LLM to `model` on the remote `provider`, or on the local runtime when it is left out.
#[tauri::command]
pub fn settings_set_llm(
    settings: State<Arc<dyn SettingsStore>>,
    provider: Option<String>,
    model: String,
) -> Result<Settings, IpcError> {
    if model.trim().is_empty() {
        return Err(IpcError::invalid_input("a model needs a name"));
    }
    settings
        .set_llm(provider.as_deref(), &model)
        .map_err(|err| IpcError::invalid_input(err.to_string()))
}

#[tauri::command]
pub fn settings_set_embedding_model(
    settings: State<Arc<dyn SettingsStore>>,
    indexer: State<Arc<dyn ActivityIndexer>>,
    model: String,
) -> Result<Settings, IpcError> {
    if model.trim().is_empty() {
        return Err(IpcError::invalid_input("a model needs a name"));
    }
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
    if hours != 0 && !AUTO_CATEGORIZE_HOURS_OPTIONS.contains(&hours) {
        return Err(IpcError::invalid_input(format!(
            "automatic categorization must run every one of {AUTO_CATEGORIZE_HOURS_OPTIONS:?} hours, or 0 to turn it off"
        )));
    }
    let saved = settings.set_auto_categorize_hours(hours)?;
    anchor.set(Utc::now());
    Ok(saved)
}
