use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::categorization::application::ai_run::{AiRunDeps, AiRunOutcome, AiRunState, AiRunStatus};
use crate::categorization::application::rules::{
    CategoryRuleView, PathSuggestion, RuleDraft, RulePreview, SourceSuggestion,
};
use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::ports::Classifier;
use crate::categorization::{ClassificationSvc, RuleSvc};
use crate::domain::LabelingSvc;
use crate::host::error::IpcError;
use crate::shared::ids::{ActivityId, CategoryId, RuleId, SessionId};
use crate::taxonomy::CategorySvc;

#[tauri::command]
pub fn session_set_row_category(
    classification: State<Box<dyn ClassificationSvc>>,
    session_id: SessionId,
    category_id: Option<CategoryId>,
) -> Result<(), IpcError> {
    Ok(classification.assign_sessions(&[session_id], category_id)?)
}

#[tauri::command]
pub fn session_set_category(
    classification: State<Box<dyn ClassificationSvc>>,
    session_ids: Vec<SessionId>,
    category_id: CategoryId,
) -> Result<(), IpcError> {
    Ok(classification.assign_sessions(&session_ids, Some(category_id))?)
}

/// Pins activities to `category_id`; `None` rejects their AI label.
#[tauri::command]
pub fn activity_set_category(
    classification: State<Box<dyn ClassificationSvc>>,
    activity_ids: Vec<ActivityId>,
    category_id: Option<CategoryId>,
) -> Result<(), IpcError> {
    Ok(classification.assign_activities(&activity_ids, category_id)?)
}

#[tauri::command]
pub fn ai_review_accept(
    classification: State<Box<dyn ClassificationSvc>>,
    activity_ids: Vec<ActivityId>,
) -> Result<(), IpcError> {
    Ok(classification.accept_ai_labels(&activity_ids)?)
}

#[derive(Serialize)]
pub struct CategorizationResetOutcome {
    /// Manual pins and AI labels removed.
    pub labels_cleared: usize,
}

#[tauri::command]
pub fn categorization_reset(
    classification: State<Box<dyn ClassificationSvc>>,
) -> Result<CategorizationResetOutcome, IpcError> {
    let labels_cleared = classification.reset()?;
    Ok(CategorizationResetOutcome { labels_cleared })
}

/// Lets a page that reloaded mid-run find out a categorization is still going and resync its
/// progress bar, instead of just losing track of it.
#[tauri::command]
pub fn categorization_status(state: State<AiRunState>) -> AiRunStatus {
    state.status()
}

/// Labels uncategorized activities with the local LLM, or with `recheck` re-labels the AI
/// labels the user has not confirmed. Runs on a blocking thread and reports
/// `ai-categorize-progress` events; `model` is an Ollama name or a `.gguf` path.
#[tauri::command]
pub async fn categorization_run(
    app: AppHandle,
    model: Option<String>,
    recheck: Option<bool>,
) -> Result<AiRunOutcome, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AiRunState>();
        let categories = app.state::<Box<dyn CategorySvc>>();
        let labeling = app.state::<Box<dyn LabelingSvc>>();
        let classification = app.state::<Box<dyn ClassificationSvc>>();
        let classifier = app.state::<Box<dyn Classifier>>();
        let deps = AiRunDeps {
            categories: &**categories,
            labeling: &**labeling,
            classification: &**classification,
            classifier: &**classifier,
        };
        state.run(&deps, model.as_deref(), recheck.unwrap_or(false), &mut |progress| {
            let _ = app.emit("ai-categorize-progress", progress);
        })
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
    .map_err(IpcError::from)
}

/// Asks a running categorization to stop after the activity it is on.
#[tauri::command]
pub fn categorization_cancel(state: State<AiRunState>) {
    state.cancel();
}

#[tauri::command(async)]
pub fn category_rule_list(rules: State<Box<dyn RuleSvc>>) -> Result<Vec<CategoryRuleView>, IpcError> {
    Ok(rules.list()?)
}

#[tauri::command]
pub fn category_rule_create(
    rules: State<Box<dyn RuleSvc>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
    category_id: CategoryId,
) -> Result<RuleId, IpcError> {
    Ok(rules.create(&RuleDraft {
        kind,
        field,
        value,
        category_id,
    })?)
}

#[tauri::command]
pub fn category_rule_update(
    rules: State<Box<dyn RuleSvc>>,
    id: RuleId,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
    category_id: CategoryId,
) -> Result<(), IpcError> {
    Ok(rules.update(
        id,
        &RuleDraft {
            kind,
            field,
            value,
            category_id,
        },
    )?)
}

#[tauri::command]
pub fn category_rule_delete(rules: State<Box<dyn RuleSvc>>, id: RuleId) -> Result<(), IpcError> {
    Ok(rules.delete(id)?)
}

/// What a rule would catch among the tracked history, without saving it.
#[tauri::command(async)]
pub fn category_rule_preview(
    rules: State<Box<dyn RuleSvc>>,
    kind: RuleKind,
    field: Option<PatternField>,
    value: String,
) -> Result<RulePreview, IpcError> {
    Ok(rules.preview(kind, field, &value)?)
}

#[tauri::command(async)]
pub fn suggestion_sources(rules: State<Box<dyn RuleSvc>>) -> Result<Vec<SourceSuggestion>, IpcError> {
    Ok(rules.source_suggestions()?)
}

#[tauri::command(async)]
pub fn suggestion_paths(
    rules: State<Box<dyn RuleSvc>>,
    domain: String,
) -> Result<Vec<PathSuggestion>, IpcError> {
    Ok(rules.path_suggestions(&domain)?)
}
