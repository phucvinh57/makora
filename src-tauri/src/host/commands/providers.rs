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
    let saved = settings
        .save_provider(ProviderConfig {
            id: id.clone(),
            name: provider.name,
            kind: provider.kind,
            base_url: provider.base_url,
        })
        .map_err(|err| IpcError::invalid_input(err.to_string()))?;
    match api_key.as_deref().map(str::trim) {
        Some("") => secrets.delete(&id).map_err(|err| IpcError::internal(format!("{err:#}")))?,
        Some(key) => secrets.set(&id, key).map_err(|err| IpcError::internal(format!("{err:#}")))?,
        None => {}
    }
    Ok(saved)
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
