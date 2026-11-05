use std::sync::Arc;

use tauri::State;

use crate::ai::providers::ProviderDraft;
use crate::app_core::AppCore;
use crate::host::error::IpcError;
use crate::settings::Settings;

#[tauri::command]
pub async fn ai_provider_save(
    core: State<'_, Arc<AppCore>>,
    provider: ProviderDraft,
    api_key: Option<String>,
) -> Result<Settings, IpcError> {
    let core = Arc::clone(&core);
    blocking(move || Ok(core.providers.save(provider, api_key)?)).await
}

#[tauri::command]
pub async fn ai_provider_remove(core: State<'_, Arc<AppCore>>, id: String) -> Result<Settings, IpcError> {
    let core = Arc::clone(&core);
    blocking(move || Ok(core.providers.remove(&id)?)).await
}

#[tauri::command]
pub async fn ai_provider_has_key(core: State<'_, Arc<AppCore>>, id: String) -> Result<bool, IpcError> {
    let core = Arc::clone(&core);
    blocking(move || Ok(core.providers.has_key(&id))).await
}

#[tauri::command]
pub async fn ai_provider_list_models(core: State<'_, Arc<AppCore>>, id: String) -> Result<Vec<String>, IpcError> {
    let core = Arc::clone(&core);
    blocking(move || Ok(core.providers.list_models(&id)?)).await
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
