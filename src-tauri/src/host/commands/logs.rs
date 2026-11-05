use std::sync::Arc;

use chrono::Utc;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::app_core::AppCore;
use crate::host::error::IpcError;
use crate::logs::reader::{HostTraffic, LogPage, LogQuery};

#[tauri::command(async)]
pub fn logs_query(core: State<Arc<AppCore>>, query: LogQuery) -> Result<LogPage, IpcError> {
    Ok(core.logs.query(&query, Utc::now())?)
}

#[tauri::command(async)]
pub fn logs_network_summary(core: State<Arc<AppCore>>, days: u32) -> Result<Vec<HostTraffic>, IpcError> {
    Ok(core.logs.network_summary(days, Utc::now())?)
}

#[tauri::command]
pub fn logs_dir(core: State<Arc<AppCore>>) -> String {
    core.logs.dir().display().to_string()
}

#[tauri::command]
pub fn logs_open_dir(app: AppHandle, core: State<Arc<AppCore>>) -> Result<(), IpcError> {
    app.opener()
        .open_path(core.logs.dir().to_string_lossy(), None::<&str>)
        .map_err(|err| IpcError::internal(format!("opening the log folder: {err}")))
}
