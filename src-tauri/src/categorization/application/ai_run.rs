//! Labeling uncategorized activities with the LLM, shown the filed activities most like each
//! one. A run catches up on any activity still without an embedding, then for every activity to
//! label ranks the activities the user has filed — by pin or by rule — against it and hands the
//! closest few to the [`Classifier`]. Every answer is saved the moment it arrives, so a stopped
//! run keeps its progress.
//!
//! [`AiRunState`] also lets a page that reloaded mid-run find out one is still going and resync
//! its progress bar. Nothing here knows about Tauri: progress is reported through a callback.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use chrono::Utc;

use serde::Serialize;
use tracing::{info, warn};

use crate::categorization::application::embeddings::{embed_missing, EmbedSkips, EmbeddingSvc};
use crate::categorization::application::ClassificationSvc;
use crate::categorization::domain::retrieval::{top_related, RELATED_COUNT};
use crate::categorization::application::usage::AiUsageSvc;
use crate::categorization::domain::usage::{AiRunRecord, RunStatus, RunTrigger};
use crate::categorization::ports::{ActivityText, Classifier, Embedder, LlmSpec, RunUsage, UsageMeter};
use crate::domain::types::{AiVerdict, Category, UncategorizedActivity};
use crate::domain::LabelingSvc;
use crate::shared::ids::{ActivityId, CategoryId, ProviderId};
use crate::shared::sync::RunGuard;
use crate::taxonomy::CategorySvc;
use crate::tracking::domain::source_key::url_location;

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
    recheck: AtomicBool,
    done: AtomicUsize,
    total: AtomicUsize,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AiRunStatus {
    pub running: bool,
    pub recheck: bool,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AiRunProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AiRunOutcome {
    pub labeled: usize,
    pub skipped: usize,
    /// Labels that differ from what was stored before the run.
    pub changed: usize,
    pub stopped: bool,
    pub usage: RunUsage,
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
}

impl AiRunState {
    pub fn status(&self) -> AiRunStatus {
        AiRunStatus {
            running: self.running.load(Ordering::Acquire),
            recheck: self.recheck.load(Ordering::Acquire),
            done: self.done.load(Ordering::Acquire),
            total: self.total.load(Ordering::Acquire),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Labels uncategorized activities, or with `recheck` re-labels the AI labels the user has
    /// not confirmed. Blocks until the run ends; `on_progress` is called after every activity.
    /// A run with something to label is recorded, with what it cost, however it ends.
    pub fn run(
        &self,
        deps: &AiRunDeps<'_>,
        models: &AiModels,
        recheck: bool,
        trigger: RunTrigger,
        on_progress: &mut dyn FnMut(AiRunProgress),
    ) -> Result<AiRunOutcome, AiRunError> {
        let _running = RunGuard::start(&self.running).ok_or(AiRunError::AlreadyRunning)?;
        self.cancel.store(false, Ordering::Release);
        self.recheck.store(recheck, Ordering::Release);
        self.done.store(0, Ordering::Release);
        self.total.store(0, Ordering::Release);

        let taxonomy = deps.categories.get_list().map_err(anyhow::Error::from)?;
        if taxonomy.is_empty() {
            return Err(AiRunError::NoCategories);
        }
        let pending = deps.labeling.get_activities_to_label(recheck)?;

        let total = pending.len();
        info!(total, provider = models.llm.provider.as_ref().map_or("local", ProviderId::as_str), model = %models.llm.model, categories = taxonomy.len(), "categorizing activities");
        self.total.store(total, Ordering::Release);
        if total == 0 {
            return Ok(AiRunOutcome {
                labeled: 0,
                skipped: 0,
                changed: 0,
                stopped: false,
                usage: RunUsage::default(),
            });
        }

        let started_at = Utc::now();
        let meter = UsageMeter::default();
        let mut tally = RunTally::default();
        let result = self.label(deps, models, &taxonomy, &pending, &meter, &mut tally, on_progress);
        let usage = RunUsage {
            llm_ms: tally.llm_ms,
            embed_count: tally.embed_count,
            embed_ms: tally.embed_ms,
            ..meter.snapshot()
        };
        let (status, error) = match &result {
            Ok(outcome) if outcome.stopped => (RunStatus::Stopped, None),
            Ok(_) => (RunStatus::Completed, None),
            Err(AiRunError::Failed(err)) => (RunStatus::Failed, Some(format!("{err:#}"))),
            Err(err) => (RunStatus::Failed, Some(err.to_string())),
        };
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
        if let Err(err) = deps.usage.record(&record) {
            warn!(error = format!("{err:#}"), "could not record the AI run's usage");
        }
        result.map(|outcome| AiRunOutcome { usage, ..outcome })
    }

    /// The run past its checks: embeds what is missing, then labels `pending`, keeping count in
    /// `tally` as it goes so a failed run is recorded with what it got done.
    #[allow(clippy::too_many_arguments)]
    fn label(
        &self,
        deps: &AiRunDeps<'_>,
        models: &AiModels,
        taxonomy: &[Category],
        pending: &[UncategorizedActivity],
        meter: &UsageMeter,
        tally: &mut RunTally,
        on_progress: &mut dyn FnMut(AiRunProgress),
    ) -> Result<AiRunOutcome, AiRunError> {
        let total = pending.len();
        let started = Instant::now();
        let cancelled = || self.cancel.load(Ordering::Acquire);
        // Related examples only sharpen the answers, so the run goes on without them.
        match embed_missing(deps.embeddings, deps.embedder, &models.embedding, deps.embed_skips, &cancelled) {
            Ok(0) => {}
            Ok(embedded) => {
                tally.embed_count = embedded as u64;
                info!(embedded, model = %models.embedding, "embedded activities before the run");
            }
            Err(err) => warn!(error = format!("{err:#}"), model = %models.embedding, "embedding activities failed; labeling without related examples"),
        }
        let embed_ms = started.elapsed().as_millis();
        tally.embed_ms = u64::try_from(embed_ms).unwrap_or(u64::MAX);
        if cancelled() {
            return Ok(AiRunOutcome {
                labeled: 0,
                skipped: 0,
                changed: 0,
                stopped: true,
                usage: RunUsage::default(),
            });
        }
        let candidates = deps.embeddings.get_candidates(&models.embedding)?;
        let ids: Vec<ActivityId> = pending.iter().map(|activity| activity.id).collect();
        let vectors = deps.embeddings.get_vectors(&models.embedding, &ids)?;
        let activities: Vec<ActivityText> = pending
            .iter()
            .map(|activity| ActivityText {
                app_name: activity.app_name.clone(),
                url: activity.url.as_deref().and_then(url_location),
                title: activity.title.clone(),
            })
            .collect();
        let related = |position: usize| {
            let id = pending[position].id;
            vectors
                .get(&id)
                .map(|query| top_related(query, &candidates, id, RELATED_COUNT))
                .unwrap_or_default()
        };
        info!(
            embed_ms,
            candidates = candidates.len(),
            pending = total,
            "run prepared"
        );

        let (mut changed, mut stopped) = (0, false);
        let mut save_error = None;
        let labeling = Instant::now();
        let mut save = |position: usize, answer: Option<CategoryId>| {
            let verdict = AiVerdict {
                activity_id: pending[position].id,
                category_id: answer,
            };
            match deps.classification.assign_ai(&[verdict]) {
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
            usage: RunUsage::default(),
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
