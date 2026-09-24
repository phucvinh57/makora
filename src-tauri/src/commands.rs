use crate::core::category::url_location;
use crate::core::feedback::FeedbackIndex;
use crate::core::llm::{self, ActivityText, ModelCache};
use crate::core::{
    ActivityStats, AiReviewPage, CategorizationResetOutcome, Category, CategoryFilter,
    CategorySourceFilter, CategoryStore, FeedbackStats, HistoryReader, NewCategory, SeedTaxonomy,
    SessionFilter, SessionRow,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};

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
        Some("ai") => Ok(CategorySourceFilter::Ai),
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

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| format!("invalid timestamp: {value}"))
}

/// Guards against two categorization runs loading the model at once.
#[derive(Default)]
pub struct AiCategorizeState {
    running: AtomicBool,
    cancel: AtomicBool,
}

#[derive(Serialize)]
pub struct AiCategorizeOutcome {
    /// Activities the model gave a category.
    pub labeled: usize,
    /// Activities the model found no category for.
    pub skipped: usize,
    /// Labels that differ from what was stored before the run.
    pub changed: usize,
    /// The user stopped the run before every activity was processed.
    pub stopped: bool,
}

#[derive(Serialize, Clone)]
struct AiCategorizeProgress {
    done: usize,
    total: usize,
}

/// Labels uncategorized activities with the local LLM, or with `recheck` re-labels the AI
/// labels the user has not confirmed. The user's earlier corrections go into the prompt. Runs
/// on a blocking thread and reports `ai-categorize-progress` events; `model` is an Ollama
/// name or a `.gguf` path.
#[tauri::command]
pub async fn categorize(
    app: AppHandle,
    state: State<'_, AiCategorizeState>,
    models: State<'_, ModelCache>,
    categories: State<'_, CategoryStore>,
    model: Option<String>,
    recheck: Option<bool>,
) -> Result<AiCategorizeOutcome, String> {
    if state.running.swap(true, Ordering::AcqRel) {
        return Err("AI categorization is already running".into());
    }
    let taxonomy = categories.list().map_err(|e| e.to_string());
    let pending = categories
        .get_activities_to_label(recheck.unwrap_or(false))
        .map_err(|e| e.to_string());
    let labeled = categories.labeled_activities().map_err(|e| e.to_string());
    let model = model.unwrap_or_else(|| llm::DEFAULT_MODEL.to_string());
    state.cancel.store(false, Ordering::Release);
    let models = models.inner().clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let (taxonomy, pending, labeled) = (taxonomy?, pending?, labeled?);
        let feedback = FeedbackIndex::new(&labeled);
        let texts: Vec<ActivityText> = pending
            .iter()
            .map(|activity| ActivityText {
                app_name: activity.app_name.clone(),
                url: activity.url.as_deref().and_then(url_location),
                title: activity.title.clone(),
                examples: feedback.for_key(&activity.key_kind, &activity.key),
            })
            .collect();
        let total = texts.len();
        info!(total, %model, examples = feedback.global().len(), "categorizing activities with the local model");
        let (mut labeled, mut changed, mut done, mut stopped) = (0, 0, 0, false);
        // Each answer is saved as soon as it arrives, so a stopped run keeps its progress.
        let mut save = |index: usize, answer: Option<i64>| {
            let cancel = &app.state::<AiCategorizeState>().cancel;
            if answer.is_some() {
                labeled += 1;
            }
            match app
                .state::<CategoryStore>()
                .assign_ai(&[(pending[index].id, answer)])
            {
                Ok(n) => changed += n,
                Err(err) => {
                    warn!(%err, "saving AI label");
                    return false;
                }
            }
            done = index + 1;
            let _ = app.emit(
                "ai-categorize-progress",
                AiCategorizeProgress {
                    done,
                    total,
                },
            );
            stopped = cancel.load(Ordering::Acquire) && index + 1 < total;
            !stopped
        };
        llm::classify(&models, &model, &taxonomy, feedback.global(), &texts, &mut save)
            .map_err(|e| format!("{e:#}"))?;
        Ok::<_, String>((labeled, changed, stopped, done))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    state.running.store(false, Ordering::Release);

    let (labeled, changed, stopped, processed) = result?;
    Ok(AiCategorizeOutcome {
        labeled,
        skipped: processed - labeled,
        changed,
        stopped,
    })
}

/// Asks a running categorization to stop after the activity it is on.
#[tauri::command]
pub fn cancel_categorize(state: State<AiCategorizeState>) {
    state.cancel.store(true, Ordering::Release);
}

/// Whether the model is currently held in memory.
#[tauri::command]
pub fn ai_model_loaded(models: State<ModelCache>) -> bool {
    models.is_loaded()
}

/// Releases the cached model's memory.
#[tauri::command]
pub fn unload_ai_model(models: State<ModelCache>) {
    models.unload();
}

#[tauri::command]
pub fn list_ai_review(
    categories: State<CategoryStore>,
    page: i64,
    page_size: i64,
    category: Option<i64>,
) -> Result<AiReviewPage, String> {
    categories
        .ai_review_page(page.max(0), page_size.max(1), category)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn accept_ai_labels(
    categories: State<CategoryStore>,
    activity_ids: Vec<i64>,
) -> Result<(), String> {
    categories
        .accept_ai_labels(&activity_ids)
        .map_err(|e| e.to_string())
}

/// Pins activities to `category_id`; `None` rejects their AI label.
#[tauri::command]
pub fn set_activities_category(
    categories: State<CategoryStore>,
    activity_ids: Vec<i64>,
    category_id: Option<i64>,
) -> Result<(), String> {
    categories
        .assign_activities(&activity_ids, category_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ai_feedback_stats(categories: State<CategoryStore>) -> Result<FeedbackStats, String> {
    categories.feedback_stats().map_err(|e| e.to_string())
}
