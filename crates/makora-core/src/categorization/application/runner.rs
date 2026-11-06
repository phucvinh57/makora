//! Runs AI categorization with the models the settings name, reporting its progress as
//! [`CoreEvent`]s.

use std::sync::Arc;

use chrono::Utc;

use crate::categorization::application::ai_run::{
    AiModels, AiRunDeps, AiRunError, AiRunOutcome, AiRunState, AiRunStatus, RunRequest,
};
use crate::categorization::application::schedule::ScheduleAnchor;
use crate::categorization::domain::usage::RunTrigger;
use crate::categorization::ports::{Classifier, Embedder, LlmSpec};
use crate::categorization::{AiUsageSvc, ClassificationSvc, EmbedSkips, EmbeddingSvc};
use crate::domain::LabelingSvc;
use crate::events::{CoreEvent, EventSink};
use crate::settings::SettingsStore;
use crate::shared::ids::ProviderId;
use crate::taxonomy::CategorySvc;

pub trait CategorizationRunner: Send + Sync {
    /// Runs `request`. Blocks until the run ends. A finished run over everything there is to
    /// label counts as the scheduler's run.
    fn run(&self, request: &RunRequest, trigger: RunTrigger) -> Result<AiRunOutcome, AiRunError>;
    fn status(&self) -> AiRunStatus;
    fn cancel(&self);
}

pub struct AiCategorizationRunner {
    pub state: Arc<AiRunState>,
    pub anchor: Arc<ScheduleAnchor>,
    pub events: Arc<dyn EventSink>,
    pub settings: Arc<dyn SettingsStore>,
    pub categories: Arc<dyn CategorySvc>,
    pub labeling: Arc<dyn LabelingSvc>,
    pub classification: Arc<dyn ClassificationSvc>,
    pub classifier: Arc<dyn Classifier>,
    pub embedder: Arc<dyn Embedder>,
    pub embeddings: Arc<dyn EmbeddingSvc>,
    pub embed_skips: Arc<EmbedSkips>,
    pub usage: Arc<dyn AiUsageSvc>,
}

impl CategorizationRunner for AiCategorizationRunner {
    fn run(&self, request: &RunRequest, trigger: RunTrigger) -> Result<AiRunOutcome, AiRunError> {
        let settings = self.settings.get();
        let deps = AiRunDeps {
            categories: &*self.categories,
            labeling: &*self.labeling,
            classification: &*self.classification,
            classifier: &*self.classifier,
            embedder: &*self.embedder,
            embeddings: &*self.embeddings,
            embed_skips: &self.embed_skips,
            usage: &*self.usage,
        };
        let models = AiModels {
            llm: LlmSpec {
                provider: settings.ai.llm_provider.map(ProviderId::from),
                model: settings.ai.llm_model,
            },
            embedding: settings.ai.embedding_model,
        };
        let outcome = self.state.run(&deps, &models, request, trigger, &mut |progress| {
            self.events.emit(CoreEvent::CategorizeProgress {
                done: progress.done,
                total: progress.total,
            });
        });
        if !matches!(outcome, Err(AiRunError::AlreadyRunning)) {
            self.events.emit(CoreEvent::CategorizeFinished);
        }
        let outcome = outcome?;
        if request.is_full_label() {
            self.anchor.set(Utc::now());
        }
        Ok(outcome)
    }

    fn status(&self) -> AiRunStatus {
        self.state.status()
    }

    fn cancel(&self) {
        self.state.cancel();
    }
}
