//! Labeling uncategorized activities with the LLM, shown the filed activities most like each
//! one. A run catches up on any activity still without an embedding, then for every activity to
//! label ranks the activities the user has filed — by pin or by rule — against it and hands the
//! closest few to the [`Classifier`]. Every answer is saved the moment it arrives, so a stopped
//! run keeps its progress.
//!
//! [`AiRunState`] also lets a window opened mid-run find out one is still going and resync
//! its progress bar. Nothing here knows about the UI: progress is reported through a callback.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::time::Instant;

use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::categorization::application::embeddings::{embed_missing, EmbedSkips, EmbeddingSvc};
use crate::categorization::application::ClassificationSvc;
use crate::categorization::domain::retrieval::{top_related, RELATED_COUNT};
use crate::categorization::application::suggestions::{generate, SuggestionWrite, TaxonomySuggestionSvc};
use crate::categorization::application::usage::AiUsageSvc;
use crate::categorization::domain::usage::{AiRunRecord, RunStatus, RunTrigger};
use crate::categorization::ports::{ActivityText, Classifier, Embedder, LlmSpec, RunUsage, Suggester, UsageMeter};
use crate::domain::types::{ActivitySample, AiVerdict, Category};
use crate::domain::LabelingSvc;
use crate::shared::ids::{ActivityId, CategoryId, ProviderId};
use crate::shared::sync::RunGuard;
use crate::taxonomy::CategorySvc;

#[derive(Debug, thiserror::Error)]
pub enum AiRunError {
    #[error("AI categorization is already running")]
    AlreadyRunning,
    #[error("there are no categories yet; create one first")]
    NoCategories,
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

#[derive(Default)]
pub struct AiRunState {
    running: AtomicBool,
    cancel: AtomicBool,
    mode: AtomicU8,
    done: AtomicUsize,
    total: AtomicUsize,
}

/// What a run asks the model about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunMode {
    /// Activities the model has never labeled.
    #[default]
    Label,
    /// Activities whose AI label the user has not confirmed, to label them again.
    Recheck,
    /// No labeling: only look for groups of unlabeled activities to propose as new categories.
    FindCategories,
}

impl RunMode {
    fn to_u8(self) -> u8 {
        match self {
            Self::Label => 0,
            Self::Recheck => 1,
            Self::FindCategories => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Recheck,
            2 => Self::FindCategories,
            _ => Self::Label,
        }
    }
}

/// A run: its mode, and with `only` just those of the activities it would take.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRequest {
    pub mode: RunMode,
    pub only: Option<Vec<ActivityId>>,
}

impl RunRequest {
    pub fn label() -> Self {
        Self::default()
    }

    pub fn recheck() -> Self {
        Self {
            mode: RunMode::Recheck,
            only: None,
        }
    }

    pub fn find_categories() -> Self {
        Self {
            mode: RunMode::FindCategories,
            only: None,
        }
    }

    pub fn scoped(mode: RunMode, only: Vec<ActivityId>) -> Self {
        Self { mode, only: Some(only) }
    }

    /// A run over everything there is to label, which is what the schedule counts.
    pub fn is_full_label(&self) -> bool {
        self.mode == RunMode::Label && self.only.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiRunStatus {
    pub running: bool,
    pub mode: RunMode,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AiRunProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AiRunOutcome {
    pub labeled: usize,
    pub skipped: usize,
    /// Labels that differ from what was stored before the run.
    pub changed: usize,
    pub stopped: bool,
    pub usage: RunUsage,
    /// New taxonomy suggestions the run came up with.
    pub suggested: usize,
}

pub struct AiModels {
    pub llm: LlmSpec,
    pub embedding: String,
}

pub struct AiRunDeps<'a> {
    pub categories: &'a dyn CategorySvc,
    pub labeling: &'a dyn LabelingSvc,
    pub classification: &'a dyn ClassificationSvc,
    pub classifier: &'a dyn Classifier,
    pub embedder: &'a dyn Embedder,
    pub embeddings: &'a dyn EmbeddingSvc,
    pub embed_skips: &'a EmbedSkips,
    pub usage: &'a dyn AiUsageSvc,
    pub suggester: &'a dyn Suggester,
    pub suggestions: &'a dyn TaxonomySuggestionSvc,
}

impl AiRunState {
    pub fn status(&self) -> AiRunStatus {
        AiRunStatus {
            running: self.running.load(Ordering::Acquire),
            mode: RunMode::from_u8(self.mode.load(Ordering::Acquire)),
            done: self.done.load(Ordering::Acquire),
            total: self.total.load(Ordering::Acquire),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Runs `request`: labels the activities it names, or with [`RunMode::FindCategories`] only
    /// proposes new categories. Blocks until the run ends; `on_progress` is called after every
    /// activity. A full labeling run ends by proposing new categories for the activities the
    /// model found none for. A run with something to do is recorded, with what it cost, however
    /// it ends.
    pub fn run(
        &self,
        deps: &AiRunDeps<'_>,
        models: &AiModels,
        request: &RunRequest,
        trigger: RunTrigger,
        on_progress: &mut dyn FnMut(AiRunProgress),
    ) -> Result<AiRunOutcome, AiRunError> {
        let _running = RunGuard::start(&self.running).ok_or(AiRunError::AlreadyRunning)?;
        self.cancel.store(false, Ordering::Release);
        self.mode.store(request.mode.to_u8(), Ordering::Release);
        self.done.store(0, Ordering::Release);
        self.total.store(0, Ordering::Release);

        let taxonomy = deps.categories.get_list().map_err(anyhow::Error::from)?;
        if taxonomy.is_empty() {
            return Err(AiRunError::NoCategories);
        }
        if request.mode == RunMode::FindCategories {
            return self.find_categories(deps, models, &taxonomy, trigger);
        }
        let pending = deps
            .labeling
            .get_activities_to_label(request.mode == RunMode::Recheck, request.only.as_deref())?;

        let total = pending.len();
        info!(total, provider = models.llm.provider.as_ref().map_or("local", ProviderId::as_str), model = %models.llm.model, categories = taxonomy.len(), "categorizing activities");
        self.total.store(total, Ordering::Release);
        if total == 0 {
            return Ok(AiRunOutcome::default());
        }

        let started_at = Utc::now();
        let meter = UsageMeter::default();
        let mut tally = RunTally::default();
        let result = self.label(deps, models, &taxonomy, &pending, &meter, &mut tally, on_progress);
        let writes = match &result {
            Ok(outcome) if request.is_full_label() && !outcome.stopped => {
                let cancelled = || self.cancel.load(Ordering::Acquire);
                generate(deps.suggestions, deps.suggester, &models.llm, &models.embedding, &taxonomy, &meter, &cancelled)
                    .unwrap_or_else(|err| {
                        warn!(error = format!("{err:#}"), "could not look for new categories");
                        Vec::new()
                    })
            }
            _ => Vec::new(),
        };
        let usage = tally.usage(&meter);
        let (status, error) = match &result {
            Ok(outcome) if outcome.stopped => (RunStatus::Stopped, None),
            Ok(_) => (RunStatus::Completed, None),
            Err(AiRunError::Failed(err)) => (RunStatus::Failed, Some(format!("{err:#}"))),
            Err(err) => (RunStatus::Failed, Some(err.to_string())),
        };
        let run_id = record(deps, models, trigger, started_at, status, error, &tally, usage);
        let suggested = save_suggestions(deps, &writes, run_id);
        result.map(|outcome| AiRunOutcome {
            usage,
            suggested,
            ..outcome
        })
    }

    /// Proposes new categories without labeling anything, after embedding what is missing.
    fn find_categories(
        &self,
        deps: &AiRunDeps<'_>,
        models: &AiModels,
        taxonomy: &[Category],
        trigger: RunTrigger,
    ) -> Result<AiRunOutcome, AiRunError> {
        info!(provider = models.llm.provider.as_ref().map_or("local", ProviderId::as_str), model = %models.llm.model, "looking for new categories");
        let started_at = Utc::now();
        let meter = UsageMeter::default();
        let mut tally = RunTally::default();
        let cancelled = || self.cancel.load(Ordering::Acquire);
        self.embed(deps, models, &mut tally);
        let result = generate(deps.suggestions, deps.suggester, &models.llm, &models.embedding, taxonomy, &meter, &cancelled);
        let stopped = cancelled();
        let usage = tally.usage(&meter);
        let (status, error) = match &result {
            Ok(_) if stopped => (RunStatus::Stopped, None),
            Ok(_) => (RunStatus::Completed, None),
            Err(err) => (RunStatus::Failed, Some(format!("{err:#}"))),
        };
        let run_id = record(deps, models, trigger, started_at, status, error, &tally, usage);
        let suggested = save_suggestions(deps, &result?, run_id);
        Ok(AiRunOutcome {
            stopped,
            usage,
            suggested,
            ..AiRunOutcome::default()
        })
    }

    /// Embeds the activities that have no vector yet, counting it in `tally`. Related examples
    /// and new categories only build on them, so a run goes on without.
    fn embed(&self, deps: &AiRunDeps<'_>, models: &AiModels, tally: &mut RunTally) {
        let started = Instant::now();
        let cancelled = || self.cancel.load(Ordering::Acquire);
        match embed_missing(deps.embeddings, deps.embedder, &models.embedding, deps.embed_skips, &cancelled) {
            Ok(0) => {}
            Ok(embedded) => {
                tally.embed_count = embedded as u64;
                info!(embedded, model = %models.embedding, "embedded activities before the run");
            }
            Err(err) => warn!(error = format!("{err:#}"), model = %models.embedding, "embedding activities failed; going on without them"),
        }
        tally.embed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    }

    /// The run past its checks: embeds what is missing, then labels `pending`, keeping count in
    /// `tally` as it goes so a failed run is recorded with what it got done.
    #[allow(clippy::too_many_arguments)]
    fn label(
        &self,
        deps: &AiRunDeps<'_>,
        models: &AiModels,
        taxonomy: &[Category],
        pending: &[ActivitySample],
        meter: &UsageMeter,
        tally: &mut RunTally,
        on_progress: &mut dyn FnMut(AiRunProgress),
    ) -> Result<AiRunOutcome, AiRunError> {
        let total = pending.len();
        self.embed(deps, models, tally);
        if self.cancel.load(Ordering::Acquire) {
            return Ok(AiRunOutcome {
                stopped: true,
                ..AiRunOutcome::default()
            });
        }
        let candidates = deps.embeddings.get_candidates(&models.embedding)?;
        let ids: Vec<ActivityId> = pending.iter().map(|activity| activity.id).collect();
        let vectors = deps.embeddings.get_vectors(&models.embedding, &ids)?;
        let activities: Vec<ActivityText> = pending.iter().map(ActivityText::of).collect();
        let related = |position: usize| {
            let id = pending[position].id;
            vectors
                .get(&id)
                .map(|query| top_related(query, &candidates, id, RELATED_COUNT))
                .unwrap_or_default()
        };
        let (mut changed, mut stopped) = (0, false);
        let mut save_error = None;
        let labeling = Instant::now();
        let mut save = |position: usize, answer: Option<CategoryId>| {
            let verdict = AiVerdict {
                activity_id: pending[position].id,
                category_id: answer,
            };
            match deps.classification.assign_ai(&[verdict], &models.llm.model) {
                Ok(n) => changed += n,
                Err(err) => {
                    save_error = Some(err.context("saving an AI label"));
                    return false;
                }
            }
            if answer.is_some() {
                tally.labeled += 1;
            }
            tally.done += 1;
            let done = tally.done;
            self.done.store(done, Ordering::Release);
            on_progress(AiRunProgress { done, total });
            stopped = self.cancel.load(Ordering::Acquire) && done < total;
            !stopped
        };
        let classified = deps
            .classifier
            .classify(&models.llm, taxonomy, &activities, &related, meter, &mut save);
        tally.llm_ms = u64::try_from(labeling.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Some(err) = save_error {
            return Err(err.into());
        }
        classified?;

        Ok(AiRunOutcome {
            labeled: tally.labeled,
            skipped: tally.done - tally.labeled,
            changed,
            stopped,
            ..AiRunOutcome::default()
        })
    }
}

/// How far a run got, for its record.
#[derive(Default)]
struct RunTally {
    done: usize,
    labeled: usize,
    embed_count: u64,
    embed_ms: u64,
    llm_ms: u64,
}

impl RunTally {
    fn usage(&self, meter: &UsageMeter) -> RunUsage {
        RunUsage {
            llm_ms: self.llm_ms,
            embed_count: self.embed_count,
            embed_ms: self.embed_ms,
            ..meter.snapshot()
        }
    }
}

/// Logs and records the run that ended, returning its id; `None` when it could not be recorded.
#[allow(clippy::too_many_arguments)]
fn record(
    deps: &AiRunDeps<'_>,
    models: &AiModels,
    trigger: RunTrigger,
    started_at: DateTime<Utc>,
    status: RunStatus,
    error: Option<String>,
    tally: &RunTally,
    usage: RunUsage,
) -> Option<i64> {
    info!(
        status = status.as_str(),
        items = tally.done,
        labeled = tally.labeled,
        requests = usage.requests,
        failed_requests = usage.failed_requests,
        input_tokens = usage.input_tokens,
        cached_input_tokens = usage.cached_input_tokens,
        output_tokens = usage.output_tokens,
        llm_ms = usage.llm_ms,
        busy_ms = usage.busy_ms,
        embed_ms = usage.embed_ms,
        "AI run finished"
    );
    let record = AiRunRecord {
        trigger,
        provider: models.llm.provider.as_ref().map(|id| id.as_str().to_string()),
        model: models.llm.model.clone(),
        status,
        error,
        started_at,
        finished_at: Utc::now(),
        items: tally.done as u64,
        labeled: tally.labeled as u64,
        usage,
    };
    deps.usage
        .record(&record)
        .inspect_err(|err| warn!(error = format!("{err:#}"), "could not record the AI run's usage"))
        .ok()
}

/// Saves the suggestions a run came up with; how many are new.
fn save_suggestions(deps: &AiRunDeps<'_>, writes: &[SuggestionWrite], run_id: Option<i64>) -> usize {
    deps.suggestions
        .save(writes, run_id)
        .inspect_err(|err| warn!(error = format!("{err:#}"), "could not save the suggested categories"))
        .unwrap_or(0)
}
