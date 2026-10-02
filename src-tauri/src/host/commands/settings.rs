use std::sync::Arc;

use tauri::State;

use crate::host::error::IpcError;
use crate::settings::{Settings, SettingsStore};
use crate::tracking::domain::session_policy::MIN_SESSION_SECONDS_RANGE;

#[tauri::command]
pub fn get_settings(settings: State<Arc<dyn SettingsStore>>) -> Settings {
    settings.get()
}

#[tauri::command]
pub fn set_min_session_seconds(settings: State<Arc<dyn SettingsStore>>, seconds: i64) -> Result<Settings, IpcError> {
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
