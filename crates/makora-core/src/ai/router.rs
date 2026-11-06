//! The [`Classifier`] the app uses: the local llama.cpp model, or the remote provider the run's
//! [`LlmSpec`] names, reached with its key from the keychain. Only what goes to a remote provider
//! is passed through the [`Redactor`] first; the local model sees everything.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use tracing::warn;

use crate::ai::privacy::{PrivacySettings, RedactionReport, Redactor};
use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::{ActivityText, Classifier, LlmSpec, UsageMeter};
use crate::domain::types::Category;
use crate::settings::SettingsStore;
use crate::shared::ids::{CategoryId, ProviderId};

use super::llm::LlamaClassifier;
use super::remote::{self, ChatApi};
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
        meter: &UsageMeter,
        on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
    ) -> Result<()> {
        let Some(id) = llm.provider.as_ref().map(ProviderId::as_str) else {
            return self.local.classify(llm, taxonomy, activities, related, meter, on_answer);
        };
        let ai = self.settings.get().ai;
        let provider = ai
            .provider(id)
            .cloned()
            .ok_or_else(|| anyhow!("the provider \"{id}\" no longer exists"))?;
        // A local server needs no key, so a keychain that cannot be read is not the end of the run.
        let key = self.secrets.get(id).unwrap_or_else(|err| {
            warn!(provider = id, error = format!("{err:#}"), "could not read the provider's API key; going without one");
            None
        });
        let api = remote::api_for(&provider, key);
        classify_remote(
            &*api,
            &provider.base_url,
            &ai.privacy,
            &llm.model,
            taxonomy,
            activities,
            related,
            meter,
            on_answer,
        )
    }
}

/// [`remote::classify`] on the activities and their related examples as `privacy` lets them out.
#[allow(clippy::too_many_arguments)]
fn classify_remote(
    api: &dyn ChatApi,
    base_url: &str,
    privacy: &PrivacySettings,
    model: &str,
    taxonomy: &[Category],
    activities: &[ActivityText],
    related: &dyn Fn(usize) -> Vec<RelatedActivity>,
    meter: &UsageMeter,
    on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
) -> Result<()> {
    let redactor = Redactor::new(privacy);
    let report = RedactionReport::default();
    let redacted: Vec<ActivityText> = activities
        .iter()
        .map(|activity| redactor.redact_activity(activity, &report))
        .collect();
    let related = |index: usize| {
        related(index)
            .iter()
            .filter_map(|example| redactor.redact_related(example, &report))
            .collect()
    };
    remote::classify(api, base_url, model, taxonomy, &redacted, &related, meter, &report, on_answer)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::ai::remote::Pick;
    use crate::categorization::domain::retrieval::SourceRef;
    use crate::categorization::domain::rules::RuleKind;
    use crate::categorization::ports::TokenUsage;
    use crate::shared::sync::lock;
    use crate::tracking::domain::exclusion::Exclusion;
    use crate::tracking::domain::source_key::KeyKind;

    use super::*;

    /// Keeps every prompt it is sent and answers "Work".
    #[derive(Default)]
    struct RecordingApi(Mutex<Vec<String>>);

    impl ChatApi for RecordingApi {
        fn pick(&self, _model: &str, system: &str, user: &str, _choices: &[String]) -> Result<Pick> {
            lock(&self.0).push(format!("{system}\n{user}"));
            Ok(Pick {
                answer: "Work".to_string(),
                usage: TokenUsage::default(),
            })
        }

        fn list_models(&self) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    fn page(domain: &str, path: &str, title: &str) -> ActivityText {
        ActivityText {
            app_name: domain.to_string(),
            url: Some(format!("{domain}{path}")),
            title: title.to_string(),
            source: SourceRef {
                kind: KeyKind::Domain,
                key: domain.to_string(),
                path: path.to_string(),
            },
        }
    }

    #[test]
    fn the_remote_provider_never_sees_what_is_redacted() {
        let taxonomy = [Category {
            id: CategoryId(1),
            name: "Work".to_string(),
            color: "#000000".to_string(),
            parent_id: None,
            description: None,
        }];
        let activities = [
            page("mail.google.com", "/mail/u/0", "Re: offer - jane.doe@acme.com"),
            page("mybank.com", "/accounts", "Checking 1234 - Jane Doe"),
        ];
        let example = page("github.com", "/acme/api/pull/42", "Review for jane.doe@acme.com");
        let related = |_: usize| {
            vec![RelatedActivity {
                app_name: example.app_name.clone(),
                url: example.url.clone(),
                title: example.title.clone(),
                category: CategoryId(1),
                source: example.source.clone(),
            }]
        };
        let privacy = PrivacySettings {
            never_send: vec![Exclusion::parse(RuleKind::Site, None, "mybank.com").unwrap()],
            ..PrivacySettings::default()
        };
        let api = RecordingApi::default();
        let mut answered = 0;
        classify_remote(
            &api,
            "https://api.example.com/v1",
            &privacy,
            "model",
            &taxonomy,
            &activities,
            &related,
            &UsageMeter::default(),
            &mut |_, _| {
                answered += 1;
                true
            },
        )
        .unwrap();

        assert_eq!(answered, 2);
        let sent = lock(&api.0).join("\n");
        for secret in ["jane.doe@acme.com", "mybank.com", "Jane Doe", "pull/42"] {
            assert!(!sent.contains(secret), "{secret} was sent: {sent}");
        }
        assert!(sent.contains("[email]") && sent.contains("[hidden]") && sent.contains("pull/{id}"));
        assert_eq!(activities[0].title, "Re: offer - jane.doe@acme.com");
    }
}
