use chrono::{DateTime, Utc};
use tauri::State;

use crate::domain::types::{
    ActivityStats, AiReviewGroupPage, AiReviewPage, CategoryFilter, CategorySourceFilter, FeedbackStats, ReviewFilter,
    SessionFilter, SessionsPage,
};
use crate::domain::{HistorySvc, LabelingSvc};
use crate::host::error::IpcError;
use crate::shared::ids::CategoryId;
use crate::shared::time::TimeRange;

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn session_list(
    history: State<Box<dyn HistorySvc>>,
    page: i64,
    page_size: i64,
    search: Option<String>,
    category: Option<String>,
    source: Option<String>,
    start: Option<String>,
    end: Option<String>,
) -> Result<SessionsPage, IpcError> {
    let start = start.as_deref().map(parse_rfc3339).transpose()?;
    let end = end.as_deref().map(parse_rfc3339).transpose()?;
    let filter = SessionFilter {
        search,
        category: parse_category_filter(category.as_deref())?,
        source: parse_category_source_filter(source.as_deref())?,
        start,
        end,
    };
    Ok(history.get_sessions(page, page_size, &filter)?)
}

#[tauri::command(async)]
pub fn activity_stats(
    history: State<Box<dyn HistorySvc>>,
    start: String,
    end: String,
) -> Result<ActivityStats, IpcError> {
    let range = TimeRange {
        start: parse_rfc3339(&start)?,
        end: parse_rfc3339(&end)?,
    };
    Ok(history.get_activity_stats(&range)?)
}

#[tauri::command(async)]
pub fn ai_review_list(
    labeling: State<Box<dyn LabelingSvc>>,
    page: i64,
    page_size: i64,
    filter: ReviewFilter,
) -> Result<AiReviewPage, IpcError> {
    Ok(labeling.get_review_page(page, page_size, &filter)?)
}

#[tauri::command(async)]
pub fn ai_review_list_groups(
    labeling: State<Box<dyn LabelingSvc>>,
    page: i64,
    page_size: i64,
    filter: ReviewFilter,
) -> Result<AiReviewGroupPage, IpcError> {
    Ok(labeling.get_review_groups(page, page_size, &filter)?)
}

#[tauri::command(async)]
pub fn ai_review_feedback_stats(labeling: State<Box<dyn LabelingSvc>>) -> Result<FeedbackStats, IpcError> {
    Ok(labeling.get_feedback_stats()?)
}

fn parse_category_source_filter(source: Option<&str>) -> Result<CategorySourceFilter, IpcError> {
    match source {
        None => Ok(CategorySourceFilter::All),
        Some("manual") => Ok(CategorySourceFilter::Manual),
        Some("ai") => Ok(CategorySourceFilter::Ai),
        Some(value) => Err(IpcError::invalid_input(format!(
            "invalid category source filter: {value}"
        ))),
    }
}

fn parse_category_filter(category: Option<&str>) -> Result<CategoryFilter, IpcError> {
    match category {
        None => Ok(CategoryFilter::All),
        Some("uncategorized") => Ok(CategoryFilter::Uncategorized),
        Some(id) => id
            .parse::<i64>()
            .map(|id| CategoryFilter::Category(CategoryId::from(id)))
            .map_err(|_| IpcError::invalid_input(format!("invalid category filter: {id}"))),
    }
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, IpcError> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| IpcError::invalid_input(format!("invalid timestamp: {value}")))
}
