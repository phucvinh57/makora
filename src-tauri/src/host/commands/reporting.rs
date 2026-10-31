use chrono::{DateTime, Utc};
use serde::Deserialize;
use tauri::State;

use crate::domain::types::{
    ActivityStats, AiReviewGroupPage, AiReviewPage, CategoryFilter, CategorySourceFilter, FeedbackStats, ReviewFilter,
    SessionFilter, SessionsPage,
};
use crate::categorization::application::usage::{AiRunView, AiUsageSummary};
use crate::categorization::AiUsageSvc;
use crate::domain::{HistorySvc, LabelingSvc};
use crate::host::error::IpcError;
use crate::shared::ids::CategoryId;
use crate::shared::time::TimeRange;

#[derive(Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SessionListQuery {
    page: i64,
    page_size: i64,
    #[ts(optional)]
    search: Option<String>,
    /// A category id, or `"uncategorized"`.
    #[ts(optional)]
    category: Option<String>,
    #[ts(optional, type = "\"manual\" | \"rule\" | \"ai\"")]
    source: Option<String>,
    #[ts(optional)]
    start: Option<String>,
    #[ts(optional)]
    end: Option<String>,
}

#[tauri::command(async)]
pub fn session_list(history: State<Box<dyn HistorySvc>>, query: SessionListQuery) -> Result<SessionsPage, IpcError> {
    let filter = SessionFilter {
        search: query.search,
        category: parse_category_filter(query.category.as_deref())?,
        source: parse_category_source_filter(query.source.as_deref())?,
        start: query.start.as_deref().map(parse_rfc3339).transpose()?,
        end: query.end.as_deref().map(parse_rfc3339).transpose()?,
    };
    Ok(history.get_sessions(query.page, query.page_size, &filter)?)
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
        Some("rule") => Ok(CategorySourceFilter::Rule),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_filter_reads_every_category_source() {
        assert!(matches!(parse_category_source_filter(None), Ok(CategorySourceFilter::All)));
        assert!(matches!(parse_category_source_filter(Some("manual")), Ok(CategorySourceFilter::Manual)));
        assert!(matches!(parse_category_source_filter(Some("rule")), Ok(CategorySourceFilter::Rule)));
        assert!(matches!(parse_category_source_filter(Some("ai")), Ok(CategorySourceFilter::Ai)));
        assert!(parse_category_source_filter(Some("other")).is_err());
    }
}

const DEFAULT_RECENT_RUNS: u32 = 20;

/// The most recent AI runs, newest first.
#[tauri::command(async)]
pub fn ai_usage_runs(usage: State<Box<dyn AiUsageSvc>>, limit: Option<u32>) -> Result<Vec<AiRunView>, IpcError> {
    Ok(usage.get_recent_runs(limit.unwrap_or(DEFAULT_RECENT_RUNS) as usize)?)
}

/// What AI runs cost over the last `days`, or ever when `None`, in total and per model.
#[tauri::command(async)]
pub fn ai_usage_summary(usage: State<Box<dyn AiUsageSvc>>, days: Option<u32>) -> Result<AiUsageSummary, IpcError> {
    let since = days.map(|days| Utc::now() - chrono::Duration::days(days.into()));
    Ok(usage.get_summary(since)?)
}
