use chrono::Utc;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::host::error::IpcError;
use crate::logs::reader::{HostTraffic, LogPage, LogQuery};
use crate::logs::LogReader;

#[tauri::command(async)]
pub fn logs_query(reader: State<Box<dyn LogReader>>, query: LogQuery) -> Result<LogPage, IpcError> {
    Ok(reader.query(&query, Utc::now())?)
}

#[tauri::command(async)]
pub fn logs_network_summary(reader: State<Box<dyn LogReader>>, days: u32) -> Result<Vec<HostTraffic>, IpcError> {
    Ok(reader.network_summary(days, Utc::now())?)
}

#[tauri::command]
pub fn logs_dir(reader: State<Box<dyn LogReader>>) -> String {
    reader.dir().display().to_string()
}

#[tauri::command]
pub fn logs_open_dir(app: AppHandle, reader: State<Box<dyn LogReader>>) -> Result<(), IpcError> {
    app.opener()
        .open_path(reader.dir().to_string_lossy(), None::<&str>)
        .map_err(|err| IpcError::internal(format!("opening the log folder: {err}")))
}
