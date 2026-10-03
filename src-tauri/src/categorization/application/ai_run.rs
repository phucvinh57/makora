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

use serde::Serialize;
use tracing::{info, warn};

use crate::categorization::application::embeddings::{embed_missing, EmbeddingSvc};
use crate::categorization::application::ClassificationSvc;
use crate::categorization::domain::retrieval::{top_related, RELATED_COUNT};
use crate::categorization::ports::{ActivityText, Classifier, Embedder};
use crate::domain::types::AiVerdict;
use crate::domain::LabelingSvc;
use crate::shared::confidence::Confidence;
use crate::shared::ids::{ActivityId, CategoryId};
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct AiRunOutcome {
    pub labeled: usize,
    pub skipped: usize,
    /// Labels that differ from what was stored before the run.
    pub changed: usize,
    pub stopped: bool,
}

pub struct AiModels {
    pub llm: String,
    pub embedding: String,
}

pub struct AiRunDeps<'a> {
    pub categories: &'a dyn CategorySvc,
    pub labeling: &'a dyn LabelingSvc,
    pub classification: &'a dyn ClassificationSvc,
    pub classifier: &'a dyn Classifier,
    pub embedder: &'a dyn Embedder,
    pub embeddings: &'a dyn EmbeddingSvc,
}

/// Clears the running flag however the run ends, panics included.
struct RunGuard<'a>(&'a AtomicBool);

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
    pub fn run(
        &self,
        deps: &AiRunDeps<'_>,
        models: &AiModels,
        recheck: bool,
        on_progress: &mut dyn FnMut(AiRunProgress),
    ) -> Result<AiRunOutcome, AiRunError> {
        if self.running.swap(true, Ordering::AcqRel) {
            return Err(AiRunError::AlreadyRunning);
        }
        let _running = RunGuard(&self.running);
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
        info!(total, model = %models.llm, categories = taxonomy.len(), "categorizing activities");
        self.total.store(total, Ordering::Release);
        if total == 0 {
            return Ok(AiRunOutcome {
                labeled: 0,
                skipped: 0,
                changed: 0,
                stopped: false,
            });
        }

        let started = Instant::now();
        let embedded = embed_missing(deps.embeddings, deps.embedder, &models.embedding)?;
        if embedded > 0 {
            info!(embedded, model = %models.embedding, "embedded activities before the run");
        }
        let embed_ms = started.elapsed().as_millis();
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

        let (mut labeled, mut changed, mut done, mut stopped) = (0, 0, 0, false);
        let mut save = |position: usize, answer: Option<CategoryId>, confidence: Confidence| {
            let verdict = AiVerdict {
                activity_id: pending[position].id,
                category_id: answer,
                confidence,
            };
            match deps.classification.assign_ai(&[verdict]) {
                Ok(n) => changed += n,
                Err(err) => {
                    warn!(%err, "saving AI label");
                    return false;
                }
            }
            if answer.is_some() {
                labeled += 1;
            }
            done = position + 1;
            self.done.store(done, Ordering::Release);
            on_progress(AiRunProgress { done, total });
            stopped = self.cancel.load(Ordering::Acquire) && position + 1 < total;
            !stopped
        };
        deps.classifier
            .classify(&models.llm, &taxonomy, &activities, &related, &mut save)?;

        Ok(AiRunOutcome {
            labeled,
            skipped: done - labeled,
            changed,
            stopped,
        })
    }
}
