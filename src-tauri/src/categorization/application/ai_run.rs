//! Labeling uncategorized activities with the model. A run loads the taxonomy, the activities
//! still without a label and what the user has taught so far, hands them to the [`Classifier`],
//! and saves every answer the moment it arrives, so a stopped run keeps its progress.
//!
//! [`AiRunState`] also lets a page that reloaded mid-run find out one is still going and resync
//! its progress bar. Nothing here knows about Tauri: progress is reported through a callback.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Serialize;
use tracing::{info, warn};

use crate::categorization::application::ClassificationSvc;
use crate::categorization::domain::feedback::FeedbackIndex;
use crate::categorization::ports::{ActivityText, Classifier};
use crate::domain::types::AiVerdict;
use crate::domain::LabelingSvc;
use crate::shared::confidence::Confidence;
use crate::shared::ids::CategoryId;
use crate::taxonomy::CategorySvc;
use crate::tracking::domain::source_key::url_location;

#[derive(Debug, thiserror::Error)]
pub enum AiRunError {
    #[error("AI categorization is already running")]
    AlreadyRunning,
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

/// Guards against two runs loading the model at once, and carries enough progress to let a page
/// that reloaded mid-run resync instead of losing track of it.
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
    /// Activities the model gave a category.
    pub labeled: usize,
    /// Activities the model found no category for.
    pub skipped: usize,
    /// Labels that differ from what was stored before the run.
    pub changed: usize,
    /// The user stopped the run before every activity was processed.
    pub stopped: bool,
}

/// The services a run works with.
pub struct AiRunDeps<'a> {
    pub categories: &'a dyn CategorySvc,
    pub labeling: &'a dyn LabelingSvc,
    pub classification: &'a dyn ClassificationSvc,
    pub classifier: &'a dyn Classifier,
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

    /// Asks a running categorization to stop after the activity it is on.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Labels uncategorized activities, or with `recheck` re-labels the AI labels the user has
    /// not confirmed. The user's earlier corrections go into the prompt. `model` falls back to
    /// the classifier's default. Blocks until the run ends; `on_progress` is called after every
    /// activity.
    pub fn run(
        &self,
        deps: &AiRunDeps<'_>,
        model: Option<&str>,
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
        let pending = deps.labeling.get_activities_to_label(recheck)?;
        let judged = deps.labeling.get_labeled_activities()?;
        let model = model.unwrap_or_else(|| deps.classifier.default_model());

        let feedback = FeedbackIndex::new(&judged);
        let texts: Vec<ActivityText> = pending
            .iter()
            .map(|activity| ActivityText {
                app_name: activity.app_name.clone(),
                url: activity.url.as_deref().and_then(url_location),
                title: activity.title.clone(),
                examples: feedback.for_key(activity.key_kind, &activity.key),
            })
            .collect();
        let total = texts.len();
        info!(total, %model, examples = feedback.global().len(), "categorizing activities with the local model");
        self.total.store(total, Ordering::Release);

        let (mut labeled, mut changed, mut done, mut stopped) = (0, 0, 0, false);
        // Each answer is saved as soon as it arrives, so a stopped run keeps its progress.
        let mut save = |index: usize, answer: Option<CategoryId>, confidence: Confidence| {
            let verdict = AiVerdict {
                activity_id: pending[index].id,
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
            done = index + 1;
            self.done.store(done, Ordering::Release);
            on_progress(AiRunProgress { done, total });
            stopped = self.cancel.load(Ordering::Acquire) && index + 1 < total;
            !stopped
        };
        deps.classifier
            .classify(model, &taxonomy, feedback.global(), &texts, &mut save)?;

        Ok(AiRunOutcome {
            labeled,
            skipped: done - labeled,
            changed,
            stopped,
        })
    }
}
