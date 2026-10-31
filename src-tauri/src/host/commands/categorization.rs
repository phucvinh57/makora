use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use std::sync::Arc;

use crate::categorization::application::ai_run::{
    AiModels, AiRunDeps, AiRunError, AiRunOutcome, AiRunState, AiRunStatus,
};
use crate::categorization::application::rules::{
    CategoryRuleView, PathSuggestion, RuleDraft, RulePreview, SourceSuggestion,
};
use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::ports::{Classifier, Embedder, LlmSpec};
use crate::categorization::{ClassificationSvc, EmbedSkips, EmbeddingSvc, RuleSvc};
use crate::domain::LabelingSvc;
use crate::host::auto_categorize::{next_run, ScheduleAnchor};
use crate::host::error::IpcError;
use crate::settings::SettingsStore;
use crate::shared::ids::{ActivityId, CategoryId, ProviderId, RuleId, SessionId};
use crate::shared::time;
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

#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
pub struct CategorizationResetOutcome {
    pub labels_cleared: usize,
}

#[tauri::command]
pub fn categorization_reset(
    classification: State<Box<dyn ClassificationSvc>>,
) -> Result<CategorizationResetOutcome, IpcError> {
    let labels_cleared = classification.reset()?;
    Ok(CategorizationResetOutcome { labels_cleared })
}

#[tauri::command]
pub fn categorization_status(state: State<AiRunState>) -> AiRunStatus {
    state.status()
}

/// RFC 3339 time of the next scheduled run; `None` while automatic categorization is off.
#[tauri::command]
pub fn categorization_next_auto_run(
    settings: State<Arc<dyn SettingsStore>>,
    anchor: State<ScheduleAnchor>,
) -> Option<String> {
    next_run(&anchor, settings.get().ai.auto_categorize_hours, Utc::now()).map(time::to_db)
}

/// Labels uncategorized activities with the LLM, or with `recheck` re-labels the AI labels the
/// user has not confirmed. Runs on a blocking thread and reports `ai-categorize-progress` events.
#[tauri::command]
pub async fn categorization_run(app: AppHandle, recheck: Option<bool>) -> Result<AiRunOutcome, IpcError> {
    tauri::async_runtime::spawn_blocking(move || run_categorization(&app, recheck.unwrap_or(false)))
        .await
        .map_err(|e| IpcError::internal(e.to_string()))?
        .map_err(IpcError::from)
}

/// Blocks until the run ends. A finished labeling run counts as the day's run for the scheduler.
pub fn run_categorization(app: &AppHandle, recheck: bool) -> Result<AiRunOutcome, AiRunError> {
    let state = app.state::<AiRunState>();
    let categories = app.state::<Box<dyn CategorySvc>>();
    let labeling = app.state::<Box<dyn LabelingSvc>>();
    let classification = app.state::<Box<dyn ClassificationSvc>>();
    let classifier = app.state::<Box<dyn Classifier>>();
    let embedder = app.state::<Arc<dyn Embedder>>();
    let embeddings = app.state::<Arc<dyn EmbeddingSvc>>();
    let embed_skips = app.state::<Arc<EmbedSkips>>();
    let settings = app.state::<Arc<dyn SettingsStore>>().get();
    let deps = AiRunDeps {
        categories: &**categories,
        labeling: &**labeling,
        classification: &**classification,
        classifier: &**classifier,
        embedder: &**embedder,
        embeddings: &**embeddings,
        embed_skips: &embed_skips,
    };
    let models = AiModels {
        llm: LlmSpec {
            provider: settings.ai.llm_provider.map(ProviderId::from),
            model: settings.ai.llm_model,
        },
        embedding: settings.ai.embedding_model,
    };
    let outcome = state.run(&deps, &models, recheck, &mut |progress| {
        let _ = app.emit("ai-categorize-progress", progress);
    })?;
    if !recheck {
        app.state::<ScheduleAnchor>().set(Utc::now());
    }
    Ok(outcome)
}

#[tauri::command]
pub fn categorization_cancel(state: State<AiRunState>) {
    state.cancel();
}

#[tauri::command(async)]
pub fn category_rule_list(rules: State<Box<dyn RuleSvc>>) -> Result<Vec<CategoryRuleView>, IpcError> {
    Ok(rules.list()?)
}

#[tauri::command(async)]
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

#[tauri::command(async)]
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

#[tauri::command(async)]
pub fn category_rule_delete(rules: State<Box<dyn RuleSvc>>, id: RuleId) -> Result<(), IpcError> {
    Ok(rules.delete(id)?)
}

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
