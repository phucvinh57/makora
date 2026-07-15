use crate::core::{Category, CategoryFilter, CategoryRule, CategoryStore, HistoryReader, SessionFilter, SessionRow};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SessionsPage {
    pub rows: Vec<SessionRow>,
    pub total: i64,
}

/// One page of activity rows (app focus and browser-tab rows alike) matching
/// `search`/`category`, newest first.
///
/// `category` selects the category filter: omitted or `null` shows every
/// row, `"uncategorized"` shows rows with no resolved category, and any
/// other value is parsed as a category id.
#[tauri::command]
pub fn list_sessions(
    reader: State<HistoryReader>,
    page: i64,
    page_size: i64,
    search: Option<String>,
    category: Option<String>,
) -> Result<SessionsPage, String> {
    let page_size = page_size.max(1);
    let offset = page.max(0) * page_size;
    let filter = SessionFilter {
        search,
        category: parse_category_filter(category.as_deref())?,
    };
    let rows = reader.list_sessions(page_size, offset, &filter).map_err(|e| e.to_string())?;
    let total = reader.count_sessions(&filter).map_err(|e| e.to_string())?;
    Ok(SessionsPage { rows, total })
}

fn parse_category_filter(category: Option<&str>) -> Result<CategoryFilter, String> {
    match category {
        None => Ok(CategoryFilter::All),
        Some("uncategorized") => Ok(CategoryFilter::Uncategorized),
        Some(id) => id.parse::<i64>().map(CategoryFilter::Category).map_err(|_| format!("invalid category filter: {id}")),
    }
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

/// Manually assigns a category to many sessions at once (and to every other
/// session sharing each one's app/site key) — for batch-categorizing a
/// filtered set of history rows in one action.
#[tauri::command]
pub fn set_sessions_category(categories: State<CategoryStore>, session_ids: Vec<i64>, category_id: i64) -> Result<(), String> {
    categories.set_overrides(&session_ids, category_id).map_err(|e| e.to_string())
}

/// Reverts a session's app/site to whatever the built-in rules say (or
/// "Uncategorized" if none match), undoing a manual override.
#[tauri::command]
pub fn clear_session_category(categories: State<CategoryStore>, session_id: i64) -> Result<(), String> {
    categories.clear_override(session_id).map_err(|e| e.to_string())
}

/// Creates a new user-defined category.
#[tauri::command]
pub fn create_category(categories: State<CategoryStore>, name: String, color: String) -> Result<Category, String> {
    categories.create_category(&name, &color).map_err(|e| e.to_string())
}

/// Renames a category or changes its color.
#[tauri::command]
pub fn update_category(categories: State<CategoryStore>, id: i64, name: String, color: String) -> Result<(), String> {
    categories.update_category(id, &name, &color).map_err(|e| e.to_string())
}

/// Deletes a category along with any rules/overrides that reference it.
#[tauri::command]
pub fn delete_category(categories: State<CategoryStore>, id: i64) -> Result<(), String> {
    categories.delete_category(id).map_err(|e| e.to_string())
}

/// All built-in categorization rules (exe name / URL domain patterns), for
/// the category management page.
#[tauri::command]
pub fn list_category_rules(categories: State<CategoryStore>) -> Result<Vec<CategoryRule>, String> {
    categories.list_rules().map_err(|e| e.to_string())
}

/// Adds a new rule matching an exe file name or URL domain to a category.
#[tauri::command]
pub fn create_category_rule(
    categories: State<CategoryStore>,
    key_kind: String,
    pattern: String,
    category_id: i64,
) -> Result<CategoryRule, String> {
    categories
        .create_rule(&key_kind, &pattern, category_id)
        .map_err(|e| e.to_string())
}

/// Removes a categorization rule.
#[tauri::command]
pub fn delete_category_rule(categories: State<CategoryStore>, id: i64) -> Result<(), String> {
    categories.delete_rule(id).map_err(|e| e.to_string())
}
