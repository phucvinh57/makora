use std::sync::Arc;

use tauri::State;

use crate::ai::llm;
use crate::ai::model_download::ModelDownloadStatus;
use crate::app_core::AppCore;
use crate::host::error::IpcError;

#[tauri::command(async)]
pub fn ai_model_is_loaded(core: State<Arc<AppCore>>) -> bool {
    core.models.is_loaded()
}

#[tauri::command(async)]
pub fn ai_model_unload(core: State<Arc<AppCore>>) {
    core.models.unload();
}

/// Whether `model` can be loaded right now (downloaded here, pulled by Ollama, or a file).
#[tauri::command(async)]
pub fn ai_model_is_installed(model: String) -> bool {
    llm::resolve_model(model.trim()).is_ok()
}

#[tauri::command(async)]
pub fn ai_model_list_installed() -> Vec<llm::InstalledModel> {
    llm::installed_models()
}

#[tauri::command]
pub fn ai_model_download_status(core: State<Arc<AppCore>>) -> ModelDownloadStatus {
    core.downloads.status()
}

/// Reports `ai-model-download-progress` events. Returns `false` when the user stopped it.
#[tauri::command]
pub async fn ai_model_download(core: State<'_, Arc<AppCore>>, model: String) -> Result<bool, IpcError> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || core.downloads.download(&model))
        .await
        .map_err(|e| IpcError::internal(e.to_string()))?
        .map_err(IpcError::from)
}

#[tauri::command]
pub fn ai_model_cancel_download(core: State<Arc<AppCore>>) {
    core.downloads.cancel();
}
