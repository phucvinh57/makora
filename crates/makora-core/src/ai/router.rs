//! The [`Classifier`] and [`Suggester`] the app uses: the local llama.cpp model, or the remote
//! provider the run's [`LlmSpec`] names, reached with its key from the keychain. Only what goes
//! to a remote provider is passed through the [`Redactor`] first; the local model sees everything.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use tracing::warn;

use crate::ai::privacy::{PrivacySettings, RedactionReport, Redactor};
use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::{ActivityText, Classifier, ClusterName, LlmSpec, Suggester, UsageMeter};
use crate::domain::types::Category;
use crate::settings::SettingsStore;
use crate::shared::ids::{CategoryId, ProviderId};

use super::llm::LlamaClassifier;
use super::choices::choices;
use super::remote::{self, ChatApi, JsonRequest};
use super::suggest::{
    naming_grammar, naming_schema, naming_system_prompt, naming_user_prompt, parse_name, sample_lines, NAMING_FIELDS,
    NAMING_MAX_TOKENS,
};
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

    /// The remote provider `id`, its API and its base URL, and the privacy settings to apply.
    fn remote(&self, id: &str) -> Result<(Box<dyn ChatApi>, String, PrivacySettings)> {
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
        Ok((remote::api_for(&provider, key), provider.base_url, ai.privacy))
    }
}

impl Suggester for LlmRouter {
    fn name_cluster(
        &self,
        llm: &LlmSpec,
        taxonomy: &[Category],
        samples: &[ActivityText],
        meter: &UsageMeter,
    ) -> Result<Option<ClusterName>> {
        let choices = choices(taxonomy);
        let system = naming_system_prompt(&choices);
        let Some(id) = llm.provider.as_ref().map(ProviderId::as_str) else {
            let mut lines = vec!["Activities that fit no category:".to_string()];
            lines.extend(sample_lines(samples));
            let answer = self.local.complete(
                &llm.model,
                &system,
                &lines,
                &naming_grammar(&choices),
                NAMING_MAX_TOKENS as usize,
                meter,
            )?;
            return Ok(remote::json_in(&answer).and_then(|answer| parse_name(&answer, &choices)));
        };
        let (api, base_url, privacy) = self.remote(id)?;
        let answer = name_remote(&*api, &base_url, &privacy, &llm.model, &system, &choices, samples, meter)?;
        Ok(parse_name(&answer, &choices))
    }
}

/// Asks the remote provider to name `samples`, as `privacy` lets them out.
#[allow(clippy::too_many_arguments)]
fn name_remote(
    api: &dyn ChatApi,
    base_url: &str,
    privacy: &PrivacySettings,
    model: &str,
    system: &str,
    choices: &[super::choices::Choice],
    samples: &[ActivityText],
    meter: &UsageMeter,
) -> Result<serde_json::Value> {
    let redactor = Redactor::new(privacy);
    let report = RedactionReport::default();
    let redacted: Vec<ActivityText> = samples
        .iter()
        .map(|sample| redactor.redact_activity(sample, &report))
        .collect();
    let user = naming_user_prompt(&sample_lines(&redacted));
    let schema = naming_schema(choices);
    let request = JsonRequest {
        system,
        user: &user,
        schema: &schema,
        max_tokens: NAMING_MAX_TOKENS,
        purpose: "Category suggestions",
        fields: NAMING_FIELDS,
        activities_sent: redacted.len(),
    };
    remote::complete_json(api, base_url, model, &request, meter, &report)
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
        let (api, base_url, privacy) = self.remote(id)?;
        classify_remote(
            &*api,
            &base_url,
            &privacy,
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

    use crate::ai::remote::{JsonReply, Pick};
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

        fn complete_json(
            &self,
            _model: &str,
            system: &str,
            user: &str,
            _schema: &serde_json::Value,
            _max_tokens: u32,
        ) -> Result<JsonReply> {
            lock(&self.0).push(format!("{system}\n{user}"));
            Ok(JsonReply {
                value: serde_json::json!({ "name": "Games", "description": "Playing games.", "parent": "None" }),
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

    #[test]
    fn naming_a_group_sends_only_redacted_samples() {
        let taxonomy = [Category {
            id: CategoryId(1),
            name: "Work".to_string(),
            color: "#000000".to_string(),
            parent_id: None,
            description: None,
        }];
        let choices = choices(&taxonomy);
        let samples = [page("mail.google.com", "/mail/u/0", "Re: lunch - jane.doe@acme.com")];
        let api = RecordingApi::default();
        let answer = name_remote(
            &api,
            "https://api.example.com/v1",
            &PrivacySettings::default(),
            "model",
            &naming_system_prompt(&choices),
            &choices,
            &samples,
            &UsageMeter::default(),
        )
        .unwrap();

        assert_eq!(parse_name(&answer, &choices).unwrap().name, "Games");
        let sent = lock(&api.0).join("\n");
        assert!(!sent.contains("jane.doe@acme.com"), "{sent}");
        assert!(sent.contains("[email]"));
    }
}
