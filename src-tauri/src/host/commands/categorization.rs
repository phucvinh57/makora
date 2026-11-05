use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use tauri::State;

use crate::app_core::AppCore;
use crate::categorization::application::ai_run::{AiRunOutcome, AiRunStatus};
use crate::categorization::application::rules::{
    CategoryRuleView, PathSuggestion, RuleDraft, RulePreview, SourceSuggestion,
};
use crate::categorization::domain::classification::ActivityClassification;
use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::domain::usage::RunTrigger;
use crate::host::error::IpcError;
use crate::shared::ids::{ActivityId, CategoryId, RuleId, SessionId};
use crate::shared::time;

#[tauri::command]
pub fn session_set_row_category(
    core: State<Arc<AppCore>>,
    session_id: SessionId,
    category_id: Option<CategoryId>,
) -> Result<Vec<ActivityClassification>, IpcError> {
    Ok(core.classification.assign_sessions(&[session_id], category_id)?)
}

#[tauri::command]
pub fn session_set_category(
    core: State<Arc<AppCore>>,
    session_ids: Vec<SessionId>,
    category_id: CategoryId,
) -> Result<Vec<ActivityClassification>, IpcError> {
    Ok(core.classification.assign_sessions(&session_ids, Some(category_id))?)
}

#[tauri::command]
pub fn session_undo_category(
    core: State<Arc<AppCore>>,
    prior: Vec<ActivityClassification>,
) -> Result<(), IpcError> {
    Ok(core.classification.undo_assignment(&prior)?)
}

#[tauri::command]
pub fn activity_set_category(
    core: State<Arc<AppCore>>,
    activity_ids: Vec<ActivityId>,
    category_id: Option<CategoryId>,
) -> Result<Vec<ActivityClassification>, IpcError> {
    Ok(core.classification.assign_activities(&activity_ids, category_id)?)
}

#[tauri::command]
pub fn ai_review_accept(
    core: State<Arc<AppCore>>,
    activity_ids: Vec<ActivityId>,
) -> Result<Vec<ActivityClassification>, IpcError> {
    Ok(core.classification.accept_ai_labels(&activity_ids)?)
}

#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
pub struct CategorizationResetOutcome {
    pub labels_cleared: usize,
}

#[tauri::command]
pub fn categorization_reset(
    core: State<Arc<AppCore>>,
) -> Result<CategorizationResetOutcome, IpcError> {
    let labels_cleared = core.classification.reset()?;
    Ok(CategorizationResetOutcome { labels_cleared })
}

#[tauri::command]
pub fn categorization_status(core: State<Arc<AppCore>>) -> AiRunStatus {
    core.runner.status()
}

/// RFC 3339 time of the next scheduled run; `None` while automatic categorization is off.
#[tauri::command]
pub fn categorization_next_auto_run(core: State<Arc<AppCore>>) -> Option<String> {
    core.schedule.next_run(Utc::now()).map(time::to_db)
}

/// Labels uncategorized activities with the LLM, or with `recheck` re-labels the AI labels the
/// user has not confirmed. Runs on a blocking thread and reports `ai-categorize-progress` events.
#[tauri::command]
pub async fn categorization_run(
    core: State<'_, Arc<AppCore>>,
    recheck: Option<bool>,
) -> Result<AiRunOutcome, IpcError> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || core.runner.run(recheck.unwrap_or(false), RunTrigger::Manual))
        .await
        .map_err(|e| IpcError::internal(e.to_string()))?
        .map_err(IpcError::from)
}

#[tauri::command]
pub fn categorization_cancel(core: State<Arc<AppCore>>) {
    core.runner.cancel();
}

#[tauri::command(async)]
pub fn category_rule_list(core: State<Arc<AppCore>>) -> Result<Vec<CategoryRuleView>, IpcError> {
    Ok(core.rules.list()?)
}

#[tauri::command(async)]
pub fn category_rule_create(
    core: State<Arc<AppCore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
    category_id: CategoryId,
) -> Result<RuleId, IpcError> {
    Ok(core.rules.create(&RuleDraft {
        kind,
        field,
        value,
        category_id,
    })?)
}

#[tauri::command(async)]
pub fn category_rule_update(
    core: State<Arc<AppCore>>,
    id: RuleId,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
    category_id: CategoryId,
) -> Result<(), IpcError> {
    Ok(core.rules.update(
        id,
        &RuleDraft {
            kind,
            field,
            value,
            category_id,
        },
    )?)
}

#[tauri::command(async)]
pub fn category_rule_delete(core: State<Arc<AppCore>>, id: RuleId) -> Result<(), IpcError> {
    Ok(core.rules.delete(id)?)
}

#[tauri::command(async)]
pub fn category_rule_preview(
    core: State<Arc<AppCore>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<RulePreview, IpcError> {
    Ok(core.rules.preview(kind, field, &value)?)
}

#[tauri::command(async)]
pub fn suggestion_sources(core: State<Arc<AppCore>>) -> Result<Vec<SourceSuggestion>, IpcError> {
    Ok(core.rules.source_suggestions()?)
}

#[tauri::command(async)]
pub fn suggestion_paths(
    core: State<Arc<AppCore>>,
    domain: String,
) -> Result<Vec<PathSuggestion>, IpcError> {
    Ok(core.rules.path_suggestions(&domain)?)
}
