//! Changing a setting from the settings page: saved, logged, and passed on to whatever depends on
//! it.

use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use crate::ai::privacy::{PrivacyPreview, PrivacySettings, Redactor};
use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::ports::ActivityText;
use crate::categorization::{ActivityIndexer, AutoCategorizeSchedule};
use crate::domain::LabelingSvc;
use crate::tracking::domain::exclusion::Exclusion;

use super::{ModelPrice, Settings, SettingsResult, SettingsStore};

pub trait SettingsSvc: Send + Sync {
    fn get(&self) -> Settings;
    /// Why the settings file could not be used at startup, once; `None` after that or when it could.
    fn take_load_issue(&self) -> Option<String>;
    fn set_min_session_seconds(&self, seconds: i64) -> SettingsResult;
    fn add_exclusion(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult;
    fn remove_exclusion(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult;
    /// Sets the LLM to `model` on the remote `provider`, or on the local runtime when it is `None`.
    fn set_llm(&self, provider: Option<&str>, model: &str) -> SettingsResult;
    /// Embeds, in the background, the activities the new model has no embedding for.
    fn set_embedding_model(&self, model: &str) -> SettingsResult;
    /// Restarts the schedule: the first run comes one period from now, on the hour.
    fn set_auto_categorize_hours(&self, hours: u32) -> SettingsResult;
    fn set_model_price(&self, price: ModelPrice) -> SettingsResult;
    fn remove_model_price(&self, provider: &str, model: &str) -> SettingsResult;
    /// Everything but the never-send list, which has its own calls.
    fn set_privacy(&self, privacy: PrivacySettings) -> SettingsResult;
    fn add_never_send(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult;
    fn remove_never_send(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult;
    /// The most recent activities as a remote provider would get them with `privacy`, beside what
    /// it gets with no redaction at all.
    fn privacy_preview(&self, privacy: &PrivacySettings) -> Result<Vec<PrivacyPreview>>;
}

/// Activities shown in the privacy preview.
const PRIVACY_PREVIEW_ROWS: i64 = 20;

pub struct SettingsService {
    store: Arc<dyn SettingsStore>,
    indexer: Arc<dyn ActivityIndexer>,
    schedule: Arc<dyn AutoCategorizeSchedule>,
    labeling: Arc<dyn LabelingSvc>,
}

impl SettingsService {
    pub fn new(
        store: Arc<dyn SettingsStore>,
        indexer: Arc<dyn ActivityIndexer>,
        schedule: Arc<dyn AutoCategorizeSchedule>,
        labeling: Arc<dyn LabelingSvc>,
    ) -> Self {
        Self {
            store,
            indexer,
            schedule,
            labeling,
        }
    }
}

impl SettingsSvc for SettingsService {
    fn get(&self) -> Settings {
        self.store.get()
    }

    fn take_load_issue(&self) -> Option<String> {
        self.store.take_load_issue()
    }

    fn set_min_session_seconds(&self, seconds: i64) -> SettingsResult {
        let saved = self.store.set_min_session_seconds(seconds)?;
        info!(setting = "min_session_seconds", value = seconds, "setting changed");
        Ok(saved)
    }

    fn add_exclusion(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult {
        let exclusion = Exclusion::parse(kind, field, value)?;
        let saved = self.store.add_exclusion(exclusion.clone())?;
        log_exclusion("exclusion added", &exclusion);
        Ok(saved)
    }

    /// Parsed as adding does, so the normalized form matches what adding saved.
    fn remove_exclusion(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult {
        let exclusion = Exclusion::parse(kind, field, value)?;
        let saved = self.store.remove_exclusion(&exclusion)?;
        log_exclusion("exclusion removed", &exclusion);
        Ok(saved)
    }

    fn set_llm(&self, provider: Option<&str>, model: &str) -> SettingsResult {
        let saved = self.store.set_llm(provider, model)?;
        let provider = provider.unwrap_or("local");
        info!(setting = "llm", provider, model, "setting changed");
        Ok(saved)
    }

    fn set_embedding_model(&self, model: &str) -> SettingsResult {
        let saved = self.store.set_embedding_model(model)?;
        info!(setting = "embedding_model", value = model, "setting changed");
        self.indexer.refresh();
        Ok(saved)
    }

    fn set_auto_categorize_hours(&self, hours: u32) -> SettingsResult {
        let saved = self.store.set_auto_categorize_hours(hours)?;
        info!(setting = "auto_categorize_hours", value = hours, "setting changed");
        self.schedule.restart();
        Ok(saved)
    }

    fn set_model_price(&self, price: ModelPrice) -> SettingsResult {
        let saved = self.store.set_model_price(price.clone())?;
        let (provider, model) = (price.provider.as_str(), price.model.as_str());
        info!(setting = "model_price", provider, model, "setting changed");
        Ok(saved)
    }

    fn remove_model_price(&self, provider: &str, model: &str) -> SettingsResult {
        let saved = self.store.remove_model_price(provider, model)?;
        info!(setting = "model_price", provider, model, "price removed");
        Ok(saved)
    }

    fn set_privacy(&self, privacy: PrivacySettings) -> SettingsResult {
        let saved = self.store.set_privacy(privacy)?;
        let privacy = &saved.ai.privacy;
        let redact: Vec<&str> = privacy.redact.iter().map(|kind| kind.as_str()).collect();
        info!(
            setting = "privacy",
            redact = redact.join(","),
            redact_account = privacy.redact_account,
            terms = privacy.terms.len(),
            url_policy = privacy.url_policy.as_str(),
            "setting changed"
        );
        Ok(saved)
    }

    fn add_never_send(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult {
        let exclusion = Exclusion::parse(kind, field, value)?;
        let saved = self.store.add_never_send(exclusion.clone())?;
        info!(setting = "never_send", kind = ?exclusion.kind, "never-send entry added");
        Ok(saved)
    }

    fn remove_never_send(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> SettingsResult {
        let exclusion = Exclusion::parse(kind, field, value)?;
        let saved = self.store.remove_never_send(&exclusion)?;
        info!(setting = "never_send", kind = ?exclusion.kind, "never-send entry removed");
        Ok(saved)
    }

    fn privacy_preview(&self, privacy: &PrivacySettings) -> Result<Vec<PrivacyPreview>> {
        let redactor = Redactor::new(privacy);
        Ok(self
            .labeling
            .get_recent_activities(PRIVACY_PREVIEW_ROWS)?
            .iter()
            .map(|sample| redactor.preview(&ActivityText::of(sample)))
            .collect())
    }
}

fn log_exclusion(message: &str, exclusion: &Exclusion) {
    let field = exclusion.field.map(|field| format!("{field:?}"));
    info!(setting = "exclusions", kind = ?exclusion.kind, field, value = exclusion.value.as_str(), "{message}");
}
