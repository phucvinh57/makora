//! The [`Classifier`] the app uses: the local llama.cpp model, or the remote provider the run's
//! [`LlmSpec`] names, reached with its key from the keychain.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use tracing::warn;

use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::{ActivityText, Classifier, LlmSpec};
use crate::domain::types::Category;
use crate::settings::SettingsStore;
use crate::shared::ids::{CategoryId, ProviderId};

use super::llm::LlamaClassifier;
use super::remote;
use super::secrets::SecretStore;

pub struct LlmRouter {
    local: LlamaClassifier,
    settings: Arc<dyn SettingsStore>,
    secrets: Arc<dyn SecretStore>,
}

impl LlmRouter {
    pub fn new(local: LlamaClassifier, settings: Arc<dyn SettingsStore>, secrets: Arc<dyn SecretStore>) -> Self {
        Self { local, settings, secrets }
    }
}

impl Classifier for LlmRouter {
    fn classify(
        &self,
        llm: &LlmSpec,
        taxonomy: &[Category],
        activities: &[ActivityText],
        related: &dyn Fn(usize) -> Vec<RelatedActivity>,
        on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
    ) -> Result<()> {
        let Some(id) = llm.provider.as_ref().map(ProviderId::as_str) else {
            return self.local.classify(llm, taxonomy, activities, related, on_answer);
        };
        let provider = self
            .settings
            .get()
            .ai
            .provider(id)
            .cloned()
            .ok_or_else(|| anyhow!("the provider \"{id}\" no longer exists"))?;
        // A local server needs no key, so a keychain that cannot be read is not the end of the run.
        let key = self.secrets.get(id).unwrap_or_else(|err| {
            warn!(provider = id, error = format!("{err:#}"), "could not read the provider's API key; going without one");
            None
        });
        let api = remote::api_for(&provider, key);
        remote::classify(&*api, &llm.model, taxonomy, activities, related, on_answer)
    }
}
