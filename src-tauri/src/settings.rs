//! App-wide preferences, kept in a TOML file the person can also edit by hand.
//!
//! The file is read once at startup. Changes made through the settings page are applied at once
//! and written back; edits made to the file while the app runs are picked up on the next launch.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::ai::{embedding, llm};
use crate::tracking::domain::exclusion::Exclusion;
use crate::tracking::domain::session_policy::{DEFAULT_MIN_SESSION_SECONDS, MIN_SESSION_SECONDS_RANGE};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub tracking: TrackingSettings,
    pub ai: AiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackingSettings {
    pub min_session_seconds: i64,
    /// Apps, sites, pages and patterns that are never recorded.
    pub excluded: Vec<Exclusion>,
}

impl Default for TrackingSettings {
    fn default() -> Self {
        Self {
            min_session_seconds: DEFAULT_MIN_SESSION_SECONDS,
            excluded: Vec::new(),
        }
    }
}

pub const DEFAULT_AUTO_CATEGORIZE_HOURS: u32 = 24;
/// The periods automatic categorization can be set to: 3, 6 and 12 hours, 1 and 3 days, a week.
pub const AUTO_CATEGORIZE_HOURS_OPTIONS: [u32; 6] = [3, 6, 12, 24, 72, 168];

/// The models AI categorization runs on, and how often it runs on its own. The embedding model is
/// always local (an Ollama name or a `.gguf` path); the LLM is local too unless `llm_provider`
/// names one of `providers`, in which case `llm_model` is that provider's model id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub llm_provider: Option<String>,
    pub llm_model: String,
    pub embedding_model: String,
    /// Hours between background runs; `0` turns them off.
    pub auto_categorize_hours: u32,
    pub providers: Vec<ProviderConfig>,
}

/// How a remote provider is spoken to: the OpenAI chat-completions API, which most providers and
/// local servers also offer, or Anthropic's messages API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
}

/// A remote LLM provider. Its API key, if it needs one, is kept in the OS keychain under `id`,
/// never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
}

impl AiSettings {
    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|provider| provider.id == id)
    }
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            llm_provider: None,
            llm_model: llm::DEFAULT_MODEL.to_string(),
            embedding_model: embedding::DEFAULT_MODEL.to_string(),
            auto_categorize_hours: DEFAULT_AUTO_CATEGORIZE_HOURS,
            providers: Vec::new(),
        }
    }
}

pub trait SettingsStore: Send + Sync {
    fn get(&self) -> Settings;
    fn set_min_session_seconds(&self, seconds: i64) -> Result<Settings>;
    fn add_exclusion(&self, exclusion: Exclusion) -> Result<Settings>;
    fn remove_exclusion(&self, exclusion: &Exclusion) -> Result<Settings>;
    /// Sets the LLM to `model` on `provider`, or on the local runtime when it is `None`.
    fn set_llm(&self, provider: Option<&str>, model: &str) -> Result<Settings>;
    fn set_embedding_model(&self, model: &str) -> Result<Settings>;
    fn set_auto_categorize_hours(&self, hours: u32) -> Result<Settings>;
    /// Adds `provider`, or replaces the one with its id.
    fn save_provider(&self, provider: ProviderConfig) -> Result<Settings>;
    /// Removes the provider with `id`. If the LLM was on it, it goes back to the local default.
    fn remove_provider(&self, id: &str) -> Result<Settings>;
}

pub struct TomlSettings {
    path: PathBuf,
    current: RwLock<Settings>,
}

impl TomlSettings {
    /// Reads the file at `path`. A missing file is created with the defaults so there is
    /// something to find and edit. One that cannot be read or parsed is left exactly as it is
    /// and the defaults are used, so a typo never costs the person the rest of their file.
    pub fn load(path: PathBuf) -> Self {
        let mut settings = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                warn!(path = %path.display(), error = %err, "settings file is malformed; using defaults");
                Settings::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let defaults = Settings::default();
                match write_atomically(&path, &defaults) {
                    Ok(()) => info!(path = %path.display(), "created settings file"),
                    Err(err) => warn!(path = %path.display(), error = format!("{err:#}"), "could not create settings file"),
                }
                defaults
            }
            Err(err) => {
                warn!(path = %path.display(), error = %err, "could not read settings file; using defaults");
                Settings::default()
            }
        };

        let seconds = settings.tracking.min_session_seconds;
        if !MIN_SESSION_SECONDS_RANGE.contains(&seconds) {
            let clamped = seconds.clamp(*MIN_SESSION_SECONDS_RANGE.start(), *MIN_SESSION_SECONDS_RANGE.end());
            warn!(seconds, clamped, "tracking.min_session_seconds is out of range; clamping");
            settings.tracking.min_session_seconds = clamped;
        }

        settings.tracking.excluded = normalized_exclusions(std::mem::take(&mut settings.tracking.excluded));

        settings.ai.providers = normalized_providers(std::mem::take(&mut settings.ai.providers));
        if let Some(id) = settings.ai.llm_provider.as_deref() {
            if settings.ai.provider(id).is_none() {
                warn!(provider = id, "ai.llm_provider is not one of ai.providers; using the local model");
                settings.ai.llm_provider = None;
                settings.ai.llm_model = llm::DEFAULT_MODEL.to_string();
            }
        }

        let defaults = AiSettings::default();
        if settings.ai.llm_model.trim().is_empty() {
            settings.ai.llm_model = defaults.llm_model;
        }
        if settings.ai.embedding_model.trim().is_empty() {
            settings.ai.embedding_model = defaults.embedding_model;
        }

        let hours = settings.ai.auto_categorize_hours;
        if hours != 0 && !AUTO_CATEGORIZE_HOURS_OPTIONS.contains(&hours) {
            let nearest = AUTO_CATEGORIZE_HOURS_OPTIONS
                .into_iter()
                .min_by_key(|option| option.abs_diff(hours))
                .unwrap_or(DEFAULT_AUTO_CATEGORIZE_HOURS);
            warn!(hours, nearest, "ai.auto_categorize_hours is not one of the allowed periods; using the nearest");
            settings.ai.auto_categorize_hours = nearest;
        }

        Self {
            path,
            current: RwLock::new(settings),
        }
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings> {
        // Held across the write so two saves cannot reach the file out of order.
        let mut current = self.current.write().unwrap();
        let mut next = current.clone();
        change(&mut next);
        write_atomically(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

/// Puts each entry in its normalized form and drops the ones that cannot be read or repeat an
/// earlier one, so what the page lists is exactly what is matched.
fn normalized_exclusions(entries: Vec<Exclusion>) -> Vec<Exclusion> {
    let mut kept: Vec<Exclusion> = Vec::with_capacity(entries.len());
    for entry in entries {
        match Exclusion::parse(entry.kind, entry.field, &entry.value) {
            Ok(exclusion) if !kept.contains(&exclusion) => kept.push(exclusion),
            Ok(_) => {}
            Err(err) => warn!(value = %entry.value, error = %err, "tracking.excluded entry is not valid; ignoring"),
        }
    }
    kept
}

/// Drops the providers that cannot be used or repeat an earlier id.
fn normalized_providers(entries: Vec<ProviderConfig>) -> Vec<ProviderConfig> {
    let mut kept: Vec<ProviderConfig> = Vec::with_capacity(entries.len());
    for entry in entries {
        match validated_provider(entry) {
            Ok(provider) if kept.iter().all(|other| other.id != provider.id) => kept.push(provider),
            Ok(provider) => warn!(id = %provider.id, "ai.providers repeats an id; ignoring"),
            Err(err) => warn!(error = %err, "ai.providers entry is not valid; ignoring"),
        }
    }
    kept
}

fn validated_provider(provider: ProviderConfig) -> Result<ProviderConfig> {
    let id = provider.id.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
        bail!("a provider id must be letters, digits, '-' or '_' (got \"{id}\")");
    }
    let name = provider.name.trim();
    if name.is_empty() {
        bail!("a provider needs a name");
    }
    let base_url = provider.base_url.trim().trim_end_matches('/');
    match url::Url::parse(base_url) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => {}
        _ => bail!("\"{base_url}\" is not an http(s) URL"),
    }
    Ok(ProviderConfig {
        id: id.to_string(),
        name: name.to_string(),
        kind: provider.kind,
        base_url: base_url.to_string(),
    })
}

fn model_name(model: &str) -> Result<String> {
    let model = model.trim();
    if model.is_empty() {
        bail!("a model needs a name");
    }
    Ok(model.to_string())
}

impl SettingsStore for TomlSettings {
    fn get(&self) -> Settings {
        self.current.read().unwrap().clone()
    }

    fn set_min_session_seconds(&self, seconds: i64) -> Result<Settings> {
        if !MIN_SESSION_SECONDS_RANGE.contains(&seconds) {
            bail!(
                "minimum session length must be between {} and {} seconds",
                MIN_SESSION_SECONDS_RANGE.start(),
                MIN_SESSION_SECONDS_RANGE.end()
            );
        }
        self.update(|settings| settings.tracking.min_session_seconds = seconds)
    }

    fn add_exclusion(&self, exclusion: Exclusion) -> Result<Settings> {
        self.update(|settings| {
            if !settings.tracking.excluded.contains(&exclusion) {
                settings.tracking.excluded.push(exclusion);
            }
        })
    }

    fn remove_exclusion(&self, exclusion: &Exclusion) -> Result<Settings> {
        self.update(|settings| settings.tracking.excluded.retain(|entry| entry != exclusion))
    }

    fn set_llm(&self, provider: Option<&str>, model: &str) -> Result<Settings> {
        let model = model_name(model)?;
        if let Some(id) = provider {
            if self.get().ai.provider(id).is_none() {
                bail!("there is no provider \"{id}\"");
            }
        }
        self.update(|settings| {
            settings.ai.llm_provider = provider.map(str::to_string);
            settings.ai.llm_model = model;
        })
    }

    fn set_embedding_model(&self, model: &str) -> Result<Settings> {
        let model = model_name(model)?;
        self.update(|settings| settings.ai.embedding_model = model)
    }

    fn set_auto_categorize_hours(&self, hours: u32) -> Result<Settings> {
        if hours != 0 && !AUTO_CATEGORIZE_HOURS_OPTIONS.contains(&hours) {
            bail!("automatic categorization must run every one of {AUTO_CATEGORIZE_HOURS_OPTIONS:?} hours, or 0 to turn it off");
        }
        self.update(|settings| settings.ai.auto_categorize_hours = hours)
    }

    fn save_provider(&self, provider: ProviderConfig) -> Result<Settings> {
        let provider = validated_provider(provider)?;
        self.update(|settings| match settings.ai.providers.iter_mut().find(|other| other.id == provider.id) {
            Some(existing) => *existing = provider,
            None => settings.ai.providers.push(provider),
        })
    }

    fn remove_provider(&self, id: &str) -> Result<Settings> {
        self.update(|settings| {
            settings.ai.providers.retain(|provider| provider.id != id);
            if settings.ai.llm_provider.as_deref() == Some(id) {
                settings.ai.llm_provider = None;
                settings.ai.llm_model = llm::DEFAULT_MODEL.to_string();
            }
        })
    }
}

fn write_atomically(path: &Path, settings: &Settings) -> Result<()> {
    let text = toml::to_string_pretty(settings).context("serializing settings")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}
