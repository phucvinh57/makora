use std::sync::Arc;

use serde::Deserialize;
use tauri::State;
use tracing::warn;

use crate::ai::remote;
use crate::ai::secrets::SecretStore;
use crate::host::error::IpcError;
use crate::settings::{ProviderConfig, ProviderKind, Settings, SettingsStore};

/// A provider as the settings page sends it: no `id` when it is new.
#[derive(Debug, Deserialize)]
pub struct ProviderDraft {
    pub id: Option<String>,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
}

/// Adds or updates a provider. `api_key` left out keeps the stored key, an empty one removes it.
#[tauri::command]
pub async fn ai_provider_save(
    settings: State<'_, Arc<dyn SettingsStore>>,
    secrets: State<'_, Arc<dyn SecretStore>>,
    provider: ProviderDraft,
    api_key: Option<String>,
) -> Result<Settings, IpcError> {
    let (settings, secrets) = (Arc::clone(&settings), Arc::clone(&secrets));
    blocking(move || save_provider(&*settings, &*secrets, provider, api_key)).await
}

/// The key goes to the keychain before the provider goes to the settings, so a keychain that
/// refuses it leaves nothing half saved; if the settings then fail, the key is put back as it was.
fn save_provider(
    settings: &dyn SettingsStore,
    secrets: &dyn SecretStore,
    provider: ProviderDraft,
    api_key: Option<String>,
) -> Result<Settings, IpcError> {
    let current = settings.get();
    let id = match provider.id {
        Some(id) if current.ai.provider(&id).is_some() => id,
        Some(id) => return Err(IpcError::new("not_found", format!("there is no provider \"{id}\""))),
        None => unique_id(&provider.name, &current.ai.providers),
    };
    let provider = ProviderConfig {
        id: id.clone(),
        name: provider.name,
        kind: provider.kind,
        base_url: provider.base_url,
    }
    .validated()?;

    let keychain = |err: anyhow::Error| IpcError::internal(format!("{err:#}"));
    let previous = match api_key.as_deref().map(str::trim) {
        None => None,
        Some(key) => {
            let previous = secrets.get(&id).map_err(keychain)?;
            match key {
                "" => secrets.delete(&id),
                key => secrets.set(&id, key),
            }
            .map_err(keychain)?;
            Some(previous)
        }
    };
    settings.save_provider(provider).inspect_err(|_| {
        let restored = match previous {
            Some(Some(key)) => secrets.set(&id, &key),
            Some(None) => secrets.delete(&id),
            None => Ok(()),
        };
        if let Err(err) = restored {
            warn!(provider = %id, error = format!("{err:#}"), "could not put the provider's API key back after a failed save");
        }
    })
    .map_err(IpcError::from)
}

#[tauri::command]
pub async fn ai_provider_remove(
    settings: State<'_, Arc<dyn SettingsStore>>,
    secrets: State<'_, Arc<dyn SecretStore>>,
    id: String,
) -> Result<Settings, IpcError> {
    let (settings, secrets) = (Arc::clone(&settings), Arc::clone(&secrets));
    blocking(move || {
        let saved = settings.remove_provider(&id)?;
        if let Err(err) = secrets.delete(&id) {
            warn!(provider = %id, error = format!("{err:#}"), "could not remove the provider's API key");
        }
        Ok(saved)
    })
    .await
}

#[tauri::command]
pub async fn ai_provider_has_key(secrets: State<'_, Arc<dyn SecretStore>>, id: String) -> Result<bool, IpcError> {
    let secrets = Arc::clone(&secrets);
    blocking(move || Ok(secrets.get(&id).ok().flatten().is_some())).await
}

/// The models the provider offers, which also shows its address and key work.
#[tauri::command]
pub async fn ai_provider_list_models(
    settings: State<'_, Arc<dyn SettingsStore>>,
    secrets: State<'_, Arc<dyn SecretStore>>,
    id: String,
) -> Result<Vec<String>, IpcError> {
    let provider = settings
        .get()
        .ai
        .provider(&id)
        .cloned()
        .ok_or_else(|| IpcError::new("not_found", format!("there is no provider \"{id}\"")))?;
    let secrets = Arc::clone(&secrets);
    blocking(move || {
        let key = secrets.get(&id).ok().flatten();
        remote::api_for(&provider, key)
            .list_models()
            .map_err(|err| IpcError::new("provider_error", format!("{err:#}")))
    })
    .await
}

/// Runs `work` on the blocking pool: the keychain and provider calls can take seconds, and must
/// hold neither the main thread nor an async worker.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, IpcError> + Send + 'static,
) -> Result<T, IpcError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| IpcError::internal(err.to_string()))?
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
    use crate::settings::{AiSettings, SettingsError, SettingsResult};
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
        assert_eq!(result.unwrap_err().code, "invalid_input");
        assert!(lock(&keychain.0).is_empty());
    }
}
