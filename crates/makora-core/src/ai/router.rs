//! The [`Classifier`] and [`Suggester`] the app uses: the local llama.cpp model, or the remote
//! provider the run's [`LlmSpec`] names, reached with its key from the keychain. Only what goes
//! to a remote provider is passed through the [`Redactor`] first; the local model sees everything.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use tracing::warn;

use crate::ai::privacy::{PrivacySettings, RedactionReport, Redactor};
use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::{ActivityText, Classifier, ClusterName, LlmSpec, MovedActivity, Suggester, UsageMeter};
use crate::domain::types::Category;
use crate::settings::SettingsStore;
use crate::shared::ids::{CategoryId, ProviderId};

use super::llm::LlamaClassifier;
use super::choices::choices;
use super::remote::{self, ChatApi, JsonRequest};
use super::suggest::{
    describe_grammar, describe_lines, describe_schema, describe_system_prompt, naming_grammar, naming_lines, naming_schema,
    naming_system_prompt, parse_description, parse_name, sample_lines, DESCRIBE_FIELDS, DESCRIBE_MAX_TOKENS, NAMING_FIELDS,
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
        let ask = JsonAsk {
            system: naming_system_prompt(&choices),
            grammar: naming_grammar(&choices),
            schema: naming_schema(&choices),
            max_tokens: NAMING_MAX_TOKENS,
            fields: NAMING_FIELDS,
            samples,
            lines: &|samples| naming_lines(&sample_lines(samples)),
        };
        Ok(self.ask_json(llm, &ask, meter)?.and_then(|answer| parse_name(&answer, &choices)))
    }

    fn describe_category(
        &self,
        llm: &LlmSpec,
        taxonomy: &[Category],
        category: &Category,
        kept: &[ActivityText],
        moved: &[MovedActivity],
        meter: &UsageMeter,
    ) -> Result<Option<String>> {
        let choices = choices(taxonomy);
        let path_of = |id: Option<CategoryId>| {
            id.and_then(|id| choices.iter().find(|choice| choice.id == id))
                .map_or_else(|| "Uncategorized".to_string(), |choice| choice.path.clone())
        };
        let path = path_of(Some(category.id));
        let destinations: Vec<String> = moved.iter().map(|activity| path_of(activity.to)).collect();
        let samples: Vec<ActivityText> = kept
            .iter()
            .cloned()
            .chain(moved.iter().map(|activity| activity.text.clone()))
            .collect();
        let lines = |samples: &[ActivityText]| {
            let all = sample_lines(samples);
            let (kept, moved) = all.split_at(kept.len());
            let moved: Vec<(String, String)> = moved.iter().cloned().zip(destinations.iter().cloned()).collect();
            describe_lines(&path, kept, &moved)
        };
        let ask = JsonAsk {
            system: describe_system_prompt(&choices, &path, category.description.as_deref()),
            grammar: describe_grammar(),
            schema: describe_schema(),
            max_tokens: DESCRIBE_MAX_TOKENS,
            fields: DESCRIBE_FIELDS,
            samples: &samples,
            lines: &lines,
        };
        Ok(self.ask_json(llm, &ask, meter)?.as_ref().and_then(parse_description))
    }
}

/// One request for a JSON answer about `samples`. `lines` writes the user message from them, after
/// redaction when they go to a remote provider.
struct JsonAsk<'a> {
    system: String,
    grammar: String,
    schema: serde_json::Value,
    max_tokens: u32,
    fields: &'static str,
    samples: &'a [ActivityText],
    lines: &'a dyn Fn(&[ActivityText]) -> Vec<String>,
}

impl LlmRouter {
    /// Asks the run's model; `None` when the local model's answer is not JSON.
    fn ask_json(&self, llm: &LlmSpec, ask: &JsonAsk, meter: &UsageMeter) -> Result<Option<serde_json::Value>> {
        let Some(id) = llm.provider.as_ref().map(ProviderId::as_str) else {
            let lines = (ask.lines)(ask.samples);
            let answer = self.local.complete(
                &llm.model,
                &ask.system,
                &lines,
                &ask.grammar,
                ask.max_tokens as usize,
                meter,
            )?;
            return Ok(remote::json_in(&answer));
        };
        let (api, base_url, privacy) = self.remote(id)?;
        ask_remote(&*api, &base_url, &privacy, &llm.model, ask, meter).map(Some)
    }
}

/// Sends `ask` to a remote provider, its samples redacted as `privacy` says.
fn ask_remote(
    api: &dyn ChatApi,
    base_url: &str,
    privacy: &PrivacySettings,
    model: &str,
    ask: &JsonAsk,
    meter: &UsageMeter,
) -> Result<serde_json::Value> {
    let redactor = Redactor::new(privacy);
    let report = RedactionReport::default();
    let redacted: Vec<ActivityText> = ask
        .samples
        .iter()
        .map(|sample| redactor.redact_activity(sample, &report))
        .collect();
    let user = (ask.lines)(&redacted).join("\n");
    let request = JsonRequest {
        system: &ask.system,
        user: &user,
        schema: &ask.schema,
        max_tokens: ask.max_tokens,
        purpose: "Category suggestions",
        fields: ask.fields,
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
        let ask = JsonAsk {
            system: naming_system_prompt(&choices),
            grammar: String::new(),
            schema: naming_schema(&choices),
            max_tokens: NAMING_MAX_TOKENS,
            fields: NAMING_FIELDS,
            samples: &samples,
            lines: &|samples| naming_lines(&sample_lines(samples)),
        };
        let answer = ask_remote(
            &api,
            "https://api.example.com/v1",
            &PrivacySettings::default(),
            "model",
            &ask,
            &UsageMeter::default(),
        )
        .unwrap();

        assert_eq!(parse_name(&answer, &choices).unwrap().name, "Games");
        let sent = lock(&api.0).join("\n");
        assert!(!sent.contains("jane.doe@acme.com"), "{sent}");
        assert!(sent.contains("[email]"));
    }

    #[test]
    fn describing_a_category_sends_only_redacted_samples() {
        let categories = [
            Category {
                id: CategoryId(1),
                name: "Work".to_string(),
                color: "#000000".to_string(),
                parent_id: None,
                description: Some("Paid work".to_string()),
            },
            Category {
                id: CategoryId(2),
                name: "Fun".to_string(),
                color: "#000000".to_string(),
                parent_id: None,
                description: None,
            },
        ];
        let choices = choices(&categories);
        let kept = [page("github.com", "/acme/api/pull/42", "Review for jane.doe@acme.com")];
        let moved = [MovedActivity {
            text: page("mail.google.com", "/mail/u/0", "Re: lunch - john.roe@acme.com"),
            to: Some(CategoryId(2)),
        }];
        let samples: Vec<ActivityText> = kept.iter().cloned().chain(moved.iter().map(|m| m.text.clone())).collect();
        let lines = |samples: &[ActivityText]| {
            let all = sample_lines(samples);
            let (kept, moved) = all.split_at(1);
            describe_lines("Work", kept, &[(moved[0].clone(), "Fun".to_string())])
        };
        let ask = JsonAsk {
            system: describe_system_prompt(&choices, "Work", Some("Paid work")),
            grammar: String::new(),
            schema: describe_schema(),
            max_tokens: DESCRIBE_MAX_TOKENS,
            fields: DESCRIBE_FIELDS,
            samples: &samples,
            lines: &lines,
        };
        let api = RecordingApi::default();
        ask_remote(&api, "https://api.example.com/v1", &PrivacySettings::default(), "model", &ask, &UsageMeter::default())
            .unwrap();

        let sent = lock(&api.0).join("\n");
        for secret in ["jane.doe@acme.com", "john.roe@acme.com", "pull/42"] {
            assert!(!sent.contains(secret), "{secret} was sent: {sent}");
        }
        assert!(sent.contains("[email]") && sent.contains("-> Fun"), "{sent}");
    }
}
