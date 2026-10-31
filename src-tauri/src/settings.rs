//! App-wide preferences, kept in a TOML file the person can also edit by hand.
//!
//! The file is read once at startup. Changes made through the settings page are applied at once
//! and written back; edits made to the file while the app runs are picked up on the next launch.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::ai::{embedding, llm};
use crate::shared::sync;
use crate::tracking::domain::exclusion::Exclusion;
use crate::tracking::domain::session_policy::{DEFAULT_MIN_SESSION_SECONDS, MIN_SESSION_SECONDS_RANGE};

#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(default)]
pub struct Settings {
    pub tracking: TrackingSettings,
    pub ai: AiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
}

/// A remote LLM provider. Its API key, if it needs one, is kept in the OS keychain under `id`,
/// never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The value asked for is not one the setting can take.
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

macro_rules! invalid {
    ($($arg:tt)*) => {
        return Err(SettingsError::Invalid(format!($($arg)*)))
    };
}

pub type SettingsResult<T = Settings> = std::result::Result<T, SettingsError>;

pub trait SettingsStore: Send + Sync {
    fn get(&self) -> Settings;
    fn set_min_session_seconds(&self, seconds: i64) -> SettingsResult;
    fn add_exclusion(&self, exclusion: Exclusion) -> SettingsResult;
    fn remove_exclusion(&self, exclusion: &Exclusion) -> SettingsResult;
    /// Sets the LLM to `model` on `provider`, or on the local runtime when it is `None`.
    fn set_llm(&self, provider: Option<&str>, model: &str) -> SettingsResult;
    fn set_embedding_model(&self, model: &str) -> SettingsResult;
    fn set_auto_categorize_hours(&self, hours: u32) -> SettingsResult;
    /// Adds `provider`, or replaces the one with its id.
    fn save_provider(&self, provider: ProviderConfig) -> SettingsResult;
    /// Removes the provider with `id`. If the LLM was on it, it goes back to the local default.
    fn remove_provider(&self, id: &str) -> SettingsResult;
    /// What went wrong reading the settings file at startup, once: later calls get `None`.
    fn take_load_issue(&self) -> Option<String>;
}

pub struct TomlSettings {
    path: PathBuf,
    current: RwLock<Settings>,
    /// The file could not be read and is still in place, so saving would overwrite it.
    read_only: bool,
    load_issue: Mutex<Option<String>>,
}

impl TomlSettings {
    /// Reads the file at `path`. A missing file is created with the defaults so there is
    /// something to find and edit. One that cannot be parsed is moved aside to
    /// `settings.toml.corrupt-<time>` and the defaults are used, so a typo never costs the person
    /// the rest of their file; one that cannot be read at all is left in place, and nothing is
    /// saved over it.
    pub fn load(path: PathBuf) -> Self {
        let mut read_only = false;
        let mut load_issue = None;
        let mut settings = match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(settings) => settings,
                Err(err) => {
                    let backup = corrupt_backup_path(&path, Utc::now());
                    match std::fs::rename(&path, &backup) {
                        Ok(()) => {
                            error!(path = %path.display(), backup = %backup.display(), error = %err, "settings file is malformed; moved it aside and using defaults");
                            load_issue = Some(format!(
                                "Your settings file could not be read, so Makora started with the default settings. The old file was kept as {}.",
                                backup.display()
                            ));
                            create_defaults(&path);
                        }
                        Err(rename_err) => {
                            error!(path = %path.display(), error = %err, rename_error = %rename_err, "settings file is malformed and could not be moved aside; using defaults without saving");
                            read_only = true;
                        }
                    }
                    Settings::default()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                create_defaults(&path);
                Settings::default()
            }
            Err(err) => {
                error!(path = %path.display(), error = %err, "could not read settings file; using defaults without saving");
                read_only = true;
                Settings::default()
            }
        };
        if read_only {
            load_issue = Some(format!(
                "Your settings file {} could not be read, so Makora is using the default settings and will not save changes until the file is fixed or removed.",
                path.display()
            ));
        }

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
            read_only,
            load_issue: Mutex::new(load_issue),
        }
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) -> SettingsResult {
        self.try_update(|settings| {
            change(settings);
            Ok(())
        })
    }

    /// Applies `change` and saves the result, or nothing when `change` fails. The lock is held
    /// across the check and the write, so two saves cannot interleave or reach the file out of
    /// order.
    fn try_update(&self, change: impl FnOnce(&mut Settings) -> SettingsResult<()>) -> SettingsResult {
        if self.read_only {
            return Err(anyhow!("{} could not be read at startup; fix or remove it and restart to save settings", self.path.display()).into());
        }
        let mut current = sync::write(&self.current);
        let mut next = current.clone();
        change(&mut next)?;
        write_atomically(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

fn create_defaults(path: &Path) {
    match write_atomically(path, &Settings::default()) {
        Ok(()) => info!(path = %path.display(), "created settings file"),
        Err(err) => warn!(path = %path.display(), error = format!("{err:#}"), "could not create settings file"),
    }
}

/// `settings.toml.corrupt-20261031T142500Z` next to `path`.
fn corrupt_backup_path(path: &Path, now: DateTime<Utc>) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".corrupt-{}", now.format("%Y%m%dT%H%M%SZ")));
    path.with_file_name(name)
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

fn validated_provider(provider: ProviderConfig) -> SettingsResult<ProviderConfig> {
    let id = provider.id.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
        invalid!("a provider id must be letters, digits, '-' or '_' (got \"{id}\")");
    }
    let name = provider.name.trim();
    if name.is_empty() {
        invalid!("a provider needs a name");
    }
    let base_url = provider.base_url.trim().trim_end_matches('/');
    match url::Url::parse(base_url) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => {}
        _ => invalid!("\"{base_url}\" is not an http(s) URL"),
    }
    Ok(ProviderConfig {
        id: id.to_string(),
        name: name.to_string(),
        kind: provider.kind,
        base_url: base_url.to_string(),
    })
}

/// A key sent over plain HTTP can be read by anyone on the path, so a provider outside this machine
/// or its local network must use HTTPS.
fn require_https_unless_local(base_url: &str) -> SettingsResult<()> {
    let Ok(url) = url::Url::parse(base_url) else {
        invalid!("\"{base_url}\" is not a URL");
    };
    if url.scheme() == "https" || url.host().is_some_and(|host| is_local(&host)) {
        return Ok(());
    }
    invalid!("\"{base_url}\" must use https:// unless it is on this machine or the local network")
}

impl ProviderConfig {
    /// The provider in its saved form, or why it cannot be saved.
    pub fn validated(self) -> SettingsResult<Self> {
        let provider = validated_provider(self)?;
        require_https_unless_local(&provider.base_url)?;
        Ok(provider)
    }
}

fn is_local(host: &url::Host<&str>) -> bool {
    match host {
        // 100.64.0.0/10 is carrier-grade NAT space, where VPNs such as Tailscale put their peers.
        url::Host::Ipv4(ip) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || (a == 100 && b & 0xc0 == 64)
        }
        url::Host::Ipv6(ip) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback() || ip.is_private())
        }
        url::Host::Domain(name) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            !name.contains('.')
                || [".localhost", ".local", ".lan", ".internal", ".home.arpa"]
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
        }
    }
}

fn model_name(model: &str) -> SettingsResult<String> {
    let model = model.trim();
    if model.is_empty() {
        invalid!("a model needs a name");
    }
    Ok(model.to_string())
}

impl SettingsStore for TomlSettings {
    fn get(&self) -> Settings {
        sync::read(&self.current).clone()
    }

    fn set_min_session_seconds(&self, seconds: i64) -> SettingsResult {
        if !MIN_SESSION_SECONDS_RANGE.contains(&seconds) {
            invalid!(
                "minimum session length must be between {} and {} seconds",
                MIN_SESSION_SECONDS_RANGE.start(),
                MIN_SESSION_SECONDS_RANGE.end()
            );
        }
        self.update(|settings| settings.tracking.min_session_seconds = seconds)
    }

    fn add_exclusion(&self, exclusion: Exclusion) -> SettingsResult {
        self.update(|settings| {
            if !settings.tracking.excluded.contains(&exclusion) {
                settings.tracking.excluded.push(exclusion);
            }
        })
    }

    fn remove_exclusion(&self, exclusion: &Exclusion) -> SettingsResult {
        self.update(|settings| settings.tracking.excluded.retain(|entry| entry != exclusion))
    }

    fn set_llm(&self, provider: Option<&str>, model: &str) -> SettingsResult {
        let model = model_name(model)?;
        self.try_update(|settings| {
            if let Some(id) = provider {
                if settings.ai.provider(id).is_none() {
                    invalid!("there is no provider \"{id}\"");
                }
            }
            settings.ai.llm_provider = provider.map(str::to_string);
            settings.ai.llm_model = model;
            Ok(())
        })
    }

    fn set_embedding_model(&self, model: &str) -> SettingsResult {
        let model = model_name(model)?;
        self.update(|settings| settings.ai.embedding_model = model)
    }

    fn set_auto_categorize_hours(&self, hours: u32) -> SettingsResult {
        if hours != 0 && !AUTO_CATEGORIZE_HOURS_OPTIONS.contains(&hours) {
            invalid!("automatic categorization must run every one of {AUTO_CATEGORIZE_HOURS_OPTIONS:?} hours, or 0 to turn it off");
        }
        self.update(|settings| settings.ai.auto_categorize_hours = hours)
    }

    fn save_provider(&self, provider: ProviderConfig) -> SettingsResult {
        let provider = provider.validated()?;
        self.update(|settings| match settings.ai.providers.iter_mut().find(|other| other.id == provider.id) {
            Some(existing) => *existing = provider,
            None => settings.ai.providers.push(provider),
        })
    }

    fn remove_provider(&self, id: &str) -> SettingsResult {
        self.update(|settings| {
            settings.ai.providers.retain(|provider| provider.id != id);
            if settings.ai.llm_provider.as_deref() == Some(id) {
                settings.ai.llm_provider = None;
                settings.ai.llm_model = llm::DEFAULT_MODEL.to_string();
            }
        })
    }

    fn take_load_issue(&self) -> Option<String> {
        sync::lock(&self.load_issue).take()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("makora-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_malformed_file_is_moved_aside_and_defaults_are_used() {
        let dir = scratch_dir("malformed");
        let path = dir.join("settings.toml");
        let original = "[tracking]\nmin_session_seconds = = 5\n";
        std::fs::write(&path, original).unwrap();

        let store = TomlSettings::load(path.clone());

        assert_eq!(store.get().tracking.min_session_seconds, DEFAULT_MIN_SESSION_SECONDS);
        let backups: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("settings.toml.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), original);
        assert!(toml::from_str::<Settings>(&std::fs::read_to_string(&path).unwrap()).is_ok());
        assert!(store.take_load_issue().is_some());
        assert!(store.take_load_issue().is_none());
        assert!(store.set_min_session_seconds(30).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_valid_file_loads_without_an_issue() {
        let dir = scratch_dir("valid");
        let path = dir.join("settings.toml");
        std::fs::write(&path, "[tracking]\nmin_session_seconds = 30\n").unwrap();
        let store = TomlSettings::load(path);
        assert_eq!(store.get().tracking.min_session_seconds, 30);
        assert!(store.take_load_issue().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_backup_name_carries_the_time() {
        let now = DateTime::parse_from_rfc3339("2026-10-31T14:25:00Z").unwrap().with_timezone(&Utc);
        assert_eq!(
            corrupt_backup_path(Path::new("/cfg/settings.toml"), now),
            Path::new("/cfg/settings.toml.corrupt-20261031T142500Z")
        );
    }

    #[test]
    fn set_llm_rejects_an_unknown_provider() {
        let dir = scratch_dir("llm");
        let store = TomlSettings::load(dir.join("settings.toml"));
        assert!(matches!(store.set_llm(Some("nope"), "gpt"), Err(SettingsError::Invalid(_))));
        assert_eq!(store.get().ai.llm_provider, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_providers_need_https() {
        assert!(require_https_unless_local("https://api.openai.com/v1").is_ok());
        assert!(require_https_unless_local("http://api.openai.com/v1").is_err());
        assert!(require_https_unless_local("http://8.8.8.8:11434").is_err());
        assert!(require_https_unless_local("http://[2001:db8::1]:8080").is_err());
    }

    #[test]
    fn local_providers_may_use_http() {
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:1234",
            "http://192.168.1.20:1234/v1",
            "http://10.0.0.5",
            "http://100.101.102.103:11434",
            "http://[::1]:8080",
            "http://[fd00::5]:8080",
            "http://gpu-box:1234",
            "http://studio.local:1234",
            "http://ollama.localhost",
        ] {
            assert!(require_https_unless_local(url).is_ok(), "{url}");
        }
    }
}
