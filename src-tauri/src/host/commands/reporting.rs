use std::sync::Arc;

use chrono::Utc;
use makora_lib::app_core::AppCore;
use makora_lib::categorization::application::usage::{since_days_ago, AiRunView, AiUsageSummary, DEFAULT_RECENT_RUNS};
use makora_lib::domain::types::{ActivityStats, AiReviewGroupPage, AiReviewPage, FeedbackStats, ReviewFilter, SessionsPage};
use makora_lib::reporting::query::{self, SessionListQuery};
use tauri::State;

use crate::host::error::IpcError;

#[tauri::command(async)]
pub fn session_list(core: State<Arc<AppCore>>, query: SessionListQuery) -> Result<SessionsPage, IpcError> {
    Ok(query::list_sessions(&*core.history, query)?)
}

#[tauri::command(async)]
pub fn activity_stats(core: State<Arc<AppCore>>, start: String, end: String) -> Result<ActivityStats, IpcError> {
    Ok(query::activity_stats(&*core.history, &start, &end)?)
}

#[tauri::command(async)]
pub fn ai_review_list(
    core: State<Arc<AppCore>>,
    page: i64,
    page_size: i64,
    filter: ReviewFilter,
) -> Result<AiReviewPage, IpcError> {
    Ok(core.labeling.get_review_page(page, page_size, &filter)?)
}

#[tauri::command(async)]
pub fn ai_review_list_groups(
    core: State<Arc<AppCore>>,
    page: i64,
    page_size: i64,
    filter: ReviewFilter,
) -> Result<AiReviewGroupPage, IpcError> {
    Ok(core.labeling.get_review_groups(page, page_size, &filter)?)
}

#[tauri::command(async)]
pub fn ai_review_feedback_stats(core: State<Arc<AppCore>>) -> Result<FeedbackStats, IpcError> {
    Ok(core.labeling.get_feedback_stats()?)
}

/// The most recent AI runs, newest first.
#[tauri::command(async)]
pub fn ai_usage_runs(core: State<Arc<AppCore>>, limit: Option<u32>) -> Result<Vec<AiRunView>, IpcError> {
    Ok(core.usage.get_recent_runs(limit.unwrap_or(DEFAULT_RECENT_RUNS) as usize)?)
}

/// What AI runs cost over the last `days`, or ever when `None`, in total and per model.
#[tauri::command(async)]
pub fn ai_usage_summary(core: State<Arc<AppCore>>, days: Option<u32>) -> Result<AiUsageSummary, IpcError> {
    Ok(core.usage.get_summary(since_days_ago(days, Utc::now()))?)
}
