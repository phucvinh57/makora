//! The remote LLM providers the user set up: their settings entry and their API key in the
//! keychain, kept in step.

use std::sync::Arc;

use serde::Deserialize;
use tracing::{info, warn};

use crate::ai::remote;
use crate::ai::secrets::SecretStore;
use crate::settings::{ProviderConfig, ProviderKind, Settings, SettingsError, SettingsStore};

/// A provider as the settings page sends it: no `id` when it is new.
#[derive(Debug, Deserialize)]
pub struct ProviderDraft {
    pub id: Option<String>,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("there is no provider \"{0}\"")]
    NotFound(String),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("{0:#}")]
    Keychain(anyhow::Error),
    /// The provider itself refused or could not be reached.
    #[error("{0:#}")]
    Remote(anyhow::Error),
}

pub trait ProviderSvc: Send + Sync {
    /// Adds or updates a provider. `api_key` left out keeps the stored key, an empty one removes it.
    fn save(&self, provider: ProviderDraft, api_key: Option<String>) -> Result<Settings, ProviderError>;
    fn remove(&self, id: &str) -> Result<Settings, ProviderError>;
    fn has_key(&self, id: &str) -> bool;
    /// The models the provider offers, which also shows its address and key work.
    fn list_models(&self, id: &str) -> Result<Vec<String>, ProviderError>;
}

pub struct KeychainProviders {
    settings: Arc<dyn SettingsStore>,
    secrets: Arc<dyn SecretStore>,
}

impl KeychainProviders {
    pub fn new(settings: Arc<dyn SettingsStore>, secrets: Arc<dyn SecretStore>) -> Self {
        Self { settings, secrets }
    }
}

impl ProviderSvc for KeychainProviders {
    fn save(&self, provider: ProviderDraft, api_key: Option<String>) -> Result<Settings, ProviderError> {
        save_provider(&*self.settings, &*self.secrets, provider, api_key)
    }

    fn remove(&self, id: &str) -> Result<Settings, ProviderError> {
        let saved = self.settings.remove_provider(id)?;
        info!(provider = %id, "provider removed");
        match self.secrets.delete(id) {
            Ok(()) => info!(provider = %id, "API key removed"),
            Err(err) => warn!(provider = %id, error = format!("{err:#}"), "could not remove the provider's API key"),
        }
        Ok(saved)
    }

    fn has_key(&self, id: &str) -> bool {
        self.secrets.get(id).ok().flatten().is_some()
    }

    fn list_models(&self, id: &str) -> Result<Vec<String>, ProviderError> {
        let provider = self
            .settings
            .get()
            .ai
            .provider(id)
            .cloned()
            .ok_or_else(|| ProviderError::NotFound(id.to_string()))?;
        let key = self.secrets.get(id).ok().flatten();
        remote::api_for(&provider, key)
            .list_models()
            .map_err(ProviderError::Remote)
    }
}

/// The key goes to the keychain before the provider goes to the settings, so a keychain that
/// refuses it leaves nothing half saved; if the settings then fail, the key is put back as it was.
fn save_provider(
    settings: &dyn SettingsStore,
    secrets: &dyn SecretStore,
    provider: ProviderDraft,
    api_key: Option<String>,
) -> Result<Settings, ProviderError> {
    let current = settings.get();
    let is_new = provider.id.is_none();
    let id = match provider.id {
        Some(id) if current.ai.provider(&id).is_some() => id,
        Some(id) => return Err(ProviderError::NotFound(id)),
        None => unique_id(&provider.name, &current.ai.providers),
    };
    let provider = ProviderConfig {
        id: id.clone(),
        name: provider.name,
        kind: provider.kind,
        base_url: provider.base_url,
    }
    .validated()?;
    let (name, kind) = (provider.name.clone(), format!("{:?}", provider.kind));
    let host = url::Url::parse(&provider.base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default();

    let previous = match api_key.as_deref().map(str::trim) {
        None => None,
        Some(key) => {
            let previous = secrets.get(&id).map_err(ProviderError::Keychain)?;
            match key {
                "" => secrets.delete(&id),
                key => secrets.set(&id, key),
            }
            .map_err(ProviderError::Keychain)?;
            Some(previous)
        }
    };
    let saved = settings.save_provider(provider).inspect_err(|_| {
        let restored = match previous {
            Some(Some(key)) => secrets.set(&id, &key),
            Some(None) => secrets.delete(&id),
            None => Ok(()),
        };
        if let Err(err) = restored {
            warn!(provider = %id, error = format!("{err:#}"), "could not put the provider's API key back after a failed save");
        }
    })?;

    let action = if is_new { "provider added" } else { "provider updated" };
    info!(provider = %id, name, kind, host, "{action}");
    match api_key.as_deref().map(str::trim) {
        Some("") => info!(provider = %id, "API key removed"),
        Some(_) => info!(provider = %id, "API key saved"),
        None => {}
    }
    Ok(saved)
}

/// A slug of `name` no other provider has: `OpenAI` becomes `openai`, then `openai-2`, ...
fn unique_id(name: &str, providers: &[ProviderConfig]) -> String {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = match slug.trim_end_matches('-') {
        "" => "provider".to_string(),
        slug => slug.to_string(),
    };
    let taken = |id: &str| providers.iter().any(|provider| provider.id == id);
    if !taken(&slug) {
        return slug;
    }
    (2..)
        .map(|n| format!("{slug}-{n}"))
        .find(|id| !taken(id))
        .expect("some suffix is free")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use anyhow::{anyhow, Result};

    use super::*;
    use crate::ai::privacy::PrivacySettings;
    use crate::settings::{AiSettings, ModelPrice, SettingsResult};
    use crate::shared::sync::lock;
    use crate::tracking::domain::exclusion::Exclusion;

    #[derive(Default)]
    struct Keychain(Mutex<HashMap<String, String>>);

    impl SecretStore for Keychain {
        fn get(&self, id: &str) -> Result<Option<String>> {
            Ok(lock(&self.0).get(id).cloned())
        }
        fn set(&self, id: &str, key: &str) -> Result<()> {
            lock(&self.0).insert(id.into(), key.into());
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<()> {
            lock(&self.0).remove(id);
            Ok(())
        }
    }

    /// Settings whose file cannot be written.
    struct ReadOnly(Settings);

    impl SettingsStore for ReadOnly {
        fn get(&self) -> Settings {
            self.0.clone()
        }
        fn save_provider(&self, _provider: ProviderConfig) -> SettingsResult {
            Err(SettingsError::Failed(anyhow!("disk full")))
        }
        fn set_min_session_seconds(&self, _: i64) -> SettingsResult {
            unreachable!()
        }
        fn add_exclusion(&self, _: Exclusion) -> SettingsResult {
            unreachable!()
        }
        fn remove_exclusion(&self, _: &Exclusion) -> SettingsResult {
            unreachable!()
        }
        fn set_llm(&self, _: Option<&str>, _: &str) -> SettingsResult {
            unreachable!()
        }
        fn set_embedding_model(&self, _: &str) -> SettingsResult {
            unreachable!()
        }
        fn set_auto_categorize_hours(&self, _: u32) -> SettingsResult {
            unreachable!()
        }
        fn remove_provider(&self, _: &str) -> SettingsResult {
            unreachable!()
        }
        fn set_model_price(&self, _: ModelPrice) -> SettingsResult {
            unreachable!()
        }
        fn remove_model_price(&self, _: &str, _: &str) -> SettingsResult {
            unreachable!()
        }
        fn set_privacy(&self, _: PrivacySettings) -> SettingsResult {
            unreachable!()
        }
        fn add_never_send(&self, _: Exclusion) -> SettingsResult {
            unreachable!()
        }
        fn remove_never_send(&self, _: &Exclusion) -> SettingsResult {
            unreachable!()
        }
        fn take_load_issue(&self) -> Option<String> {
            None
        }
    }

    fn existing() -> Settings {
        Settings {
            ai: AiSettings {
                providers: vec![ProviderConfig {
                    id: "openai".into(),
                    name: "OpenAI".into(),
                    kind: ProviderKind::OpenAi,
                    base_url: "https://api.openai.com/v1".into(),
                }],
                ..AiSettings::default()
            },
            ..Settings::default()
        }
    }

    fn draft(id: Option<&str>, base_url: &str) -> ProviderDraft {
        ProviderDraft {
            id: id.map(str::to_string),
            name: "OpenAI".into(),
            kind: ProviderKind::OpenAi,
            base_url: base_url.into(),
        }
    }

    #[test]
    fn a_failed_settings_save_puts_the_old_key_back() {
        let keychain = Keychain::default();
        keychain.set("openai", "old").unwrap();
        let result = save_provider(
            &ReadOnly(existing()),
            &keychain,
            draft(Some("openai"), "https://api.openai.com/v1"),
            Some("new".into()),
        );
        assert!(result.is_err());
        assert_eq!(keychain.get("openai").unwrap().as_deref(), Some("old"));
    }

    #[test]
    fn a_failed_settings_save_removes_a_new_key() {
        let keychain = Keychain::default();
        let result = save_provider(
            &ReadOnly(Settings::default()),
            &keychain,
            draft(None, "https://api.openai.com/v1"),
            Some("new".into()),
        );
        assert!(result.is_err());
        assert!(lock(&keychain.0).is_empty());
    }

    #[test]
    fn an_invalid_provider_never_touches_the_keychain() {
        let keychain = Keychain::default();
        let result = save_provider(
            &ReadOnly(Settings::default()),
            &keychain,
            draft(None, "http://api.example.com"),
            Some("new".into()),
        );
        assert!(matches!(result, Err(ProviderError::Settings(SettingsError::Invalid(_)))));
        assert!(lock(&keychain.0).is_empty());
    }
}
