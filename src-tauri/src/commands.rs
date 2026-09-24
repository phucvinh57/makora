use std::sync::Arc;

use crate::core::{
    ActivityStats, CategorizationResetOutcome, Category, CategoryFilter, CategorySourceFilter,
    CategoryStore, HistoryReader, NewCategory, NewWorkSession, SeedTaxonomy, SessionFilter,
    SessionRow, UrlQueryParam, UrlQueryStore, WorkSession, WorkSessionFields, WorkSessionStats,
    WorkSessionStore, WorkSessionUpdate,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SessionsPage {
    pub rows: Vec<SessionRow>,
    pub total: i64,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn list_sessions(
    reader: State<HistoryReader>,
    page: i64,
    page_size: i64,
    search: Option<String>,
    category: Option<String>,
    source: Option<String>,
    start: Option<String>,
    end: Option<String>,
) -> Result<SessionsPage, String> {
    let page_size = page_size.max(1);
    let offset = page.max(0) * page_size;
    let start = start.as_deref().map(parse_rfc3339).transpose()?;
    let end = end.as_deref().map(parse_rfc3339).transpose()?;
    let filter = SessionFilter {
        search,
        category: parse_category_filter(category.as_deref())?,
        source: parse_category_source_filter(source.as_deref())?,
        start: start.map(|t| t.to_rfc3339()),
        end: end.map(|t| t.to_rfc3339()),
    };
    let rows = reader
        .list_sessions(page_size, offset, &filter)
        .map_err(|e| e.to_string())?;
    let total = reader.count_sessions(&filter).map_err(|e| e.to_string())?;
    Ok(SessionsPage { rows, total })
}

fn parse_category_source_filter(source: Option<&str>) -> Result<CategorySourceFilter, String> {
    match source {
        None => Ok(CategorySourceFilter::All),
        Some("manual") => Ok(CategorySourceFilter::Manual),
        Some(value) => Err(format!("invalid category source filter: {value}")),
    }
}

#[tauri::command]
pub fn activity_stats(
    reader: State<HistoryReader>,
    start: String,
    end: String,
) -> Result<ActivityStats, String> {
    reader
        .activity_stats(&start, &end)
        .map_err(|e| e.to_string())
}

fn parse_category_filter(category: Option<&str>) -> Result<CategoryFilter, String> {
    match category {
        None => Ok(CategoryFilter::All),
        Some("uncategorized") => Ok(CategoryFilter::Uncategorized),
        Some(id) => id
            .parse::<i64>()
            .map(CategoryFilter::Category)
            .map_err(|_| format!("invalid category filter: {id}")),
    }
}

#[tauri::command]
pub fn list_categories(categories: State<CategoryStore>) -> Result<Vec<Category>, String> {
    categories.list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_session_row_category(
    categories: State<CategoryStore>,
    session_id: i64,
    category_id: Option<i64>,
) -> Result<(), String> {
    categories
        .assign_sessions(&[session_id], category_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_sessions_category(
    categories: State<CategoryStore>,
    session_ids: Vec<i64>,
    category_id: i64,
) -> Result<(), String> {
    categories
        .assign_sessions(&session_ids, Some(category_id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_category(
    categories: State<CategoryStore>,
    name: String,
    color: String,
    description: Option<String>,
    parent_id: Option<i64>,
) -> Result<Category, String> {
    categories
        .create(&NewCategory {
            name,
            color,
            description,
            parent_id,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_category(
    categories: State<CategoryStore>,
    id: i64,
    name: String,
    color: String,
    description: Option<String>,
    parent_id: Option<i64>,
) -> Result<(), String> {
    categories
        .update(
            id,
            &NewCategory {
                name,
                color,
                description,
                parent_id,
            },
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_category(categories: State<CategoryStore>, id: i64) -> Result<(), String> {
    categories.delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_seed_taxonomy(categories: State<CategoryStore>) -> Result<(), String> {
    categories
        .apply_seed_taxonomy(include_str!("../resources/seed_taxonomy.json"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_seed_taxonomy_preview() -> Result<SeedTaxonomy, String> {
    CategoryStore::seed_taxonomy_preview(include_str!("../resources/seed_taxonomy.json"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reset_categorization(
    categories: State<CategoryStore>,
) -> Result<CategorizationResetOutcome, String> {
    categories
        .reset_categorization()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_url_query_params(
    params: State<Arc<UrlQueryStore>>,
) -> Result<Vec<UrlQueryParam>, String> {
    params.list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_url_query_param(
    params: State<Arc<UrlQueryStore>>,
    domain: String,
    param: String,
) -> Result<UrlQueryParam, String> {
    params.create(&domain, &param).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_url_query_param(params: State<Arc<UrlQueryStore>>, id: i64) -> Result<(), String> {
    params.delete(id).map_err(|e| e.to_string())
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| format!("invalid timestamp: {value}"))
}

fn validate_work_session(name: &str, target_seconds: i64) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("work session name must not be empty".to_string());
    }
    if target_seconds <= 0 {
        return Err("target duration must be positive".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn create_work_session(
    work_sessions: State<WorkSessionStore>,
    name: String,
    note: Option<String>,
    target_seconds: i64,
    start_time: Option<String>,
    category_ids: Vec<i64>,
) -> Result<WorkSession, String> {
    validate_work_session(&name, target_seconds)?;
    let start = start_time.as_deref().map(parse_rfc3339).transpose()?;
    work_sessions
        .create(&NewWorkSession {
            name: name.trim().to_string(),
            note,
            target_seconds,
            start_time: start,
            category_ids,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_work_sessions(
    work_sessions: State<WorkSessionStore>,
) -> Result<Vec<WorkSession>, String> {
    work_sessions.list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_work_sessions(
    work_sessions: State<WorkSessionStore>,
    search: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<Vec<WorkSession>, String> {
    work_sessions
        .search(search.as_deref(), limit, offset)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_work_session(
    work_sessions: State<WorkSessionStore>,
    id: i64,
) -> Result<Option<WorkSession>, String> {
    work_sessions.get(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_work_session(
    work_sessions: State<WorkSessionStore>,
    update: WorkSessionUpdate,
) -> Result<(), String> {
    let WorkSessionUpdate { id, fields } = update;
    validate_work_session(&fields.name, fields.target_seconds)?;
    let start = parse_rfc3339(&fields.start_time)?;
    let end = parse_rfc3339(&fields.end_time)?;
    if end <= start {
        return Err("end time must be after start time".to_string());
    }
    let fields = WorkSessionFields {
        name: fields.name.trim().to_string(),
        start_time: start.to_rfc3339(),
        end_time: end.to_rfc3339(),
        ..fields
    };
    work_sessions.update(id, &fields).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_work_session(work_sessions: State<WorkSessionStore>, id: i64) -> Result<(), String> {
    work_sessions.stop(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_work_session(work_sessions: State<WorkSessionStore>, id: i64) -> Result<(), String> {
    work_sessions.delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn work_session_stats(
    work_sessions: State<WorkSessionStore>,
    id: i64,
) -> Result<WorkSessionStats, String> {
    work_sessions.stats(id).map_err(|e| e.to_string())
}
