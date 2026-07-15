use crate::core::{HistoryReader, SessionRow};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SessionsPage {
    pub rows: Vec<SessionRow>,
    pub total: i64,
}

/// One page of activity rows (app focus and browser-tab rows alike), newest
/// first.
#[tauri::command]
pub fn list_sessions(reader: State<HistoryReader>, page: i64, page_size: i64) -> Result<SessionsPage, String> {
    let page_size = page_size.max(1);
    let offset = page.max(0) * page_size;
    let rows = reader.list_sessions(page_size, offset).map_err(|e| e.to_string())?;
    let total = reader.count_sessions().map_err(|e| e.to_string())?;
    Ok(SessionsPage { rows, total })
}
