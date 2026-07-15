use crate::core::{Category, CategoryStore, HistoryReader, SessionRow};
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

/// All categories (built-in defaults plus any the user has added), for
/// populating a "change category" picker in the UI.
#[tauri::command]
pub fn list_categories(categories: State<CategoryStore>) -> Result<Vec<Category>, String> {
    categories.list_categories().map_err(|e| e.to_string())
}

/// Manually assigns a category to a session — and to every other session
/// sharing the same app/site, since that's what a user picking a category
/// for one row actually expects.
#[tauri::command]
pub fn set_session_category(categories: State<CategoryStore>, session_id: i64, category_id: i64) -> Result<(), String> {
    categories.set_override(session_id, category_id).map_err(|e| e.to_string())
}

/// Reverts a session's app/site to whatever the built-in rules say (or
/// "Uncategorized" if none match), undoing a manual override.
#[tauri::command]
pub fn clear_session_category(categories: State<CategoryStore>, session_id: i64) -> Result<(), String> {
    categories.clear_override(session_id).map_err(|e| e.to_string())
}
