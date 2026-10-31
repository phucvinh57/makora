use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::llm::{self, ModelCache};
use crate::ai::model_download::{ModelDownloadState, ModelDownloadStatus};
use crate::categorization::ActivityIndexer;
use crate::host::error::IpcError;

#[derive(Serialize, Clone)]
struct ModelDownloadProgress {
    downloaded: u64,
    total: u64,
}

#[tauri::command(async)]
pub fn ai_model_is_loaded(models: State<ModelCache>) -> bool {
    models.is_loaded()
}

#[tauri::command(async)]
pub fn ai_model_unload(models: State<ModelCache>) {
    models.unload();
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
pub fn ai_model_download_status(state: State<ModelDownloadState>) -> ModelDownloadStatus {
    state.status()
}

/// Downloads an Ollama-library model (`llama3.2:3b`) and reports `ai-model-download-progress`
/// events. Returns `false` when the user stopped it; the partial file is kept for resuming.
#[tauri::command]
pub async fn ai_model_download(app: AppHandle, model: String) -> Result<bool, IpcError> {
    let model = model.trim().to_string();
    let root = llm::download_root().ok_or_else(|| IpcError::internal("model directory is not ready"))?;

    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ModelDownloadState>();
        let mut last_emit = std::time::Instant::now();
        let downloaded = state.run(root, &model, &mut |downloaded, total| {
            // ~10 events a second is plenty for a progress bar.
            if last_emit.elapsed() >= std::time::Duration::from_millis(100) || downloaded == total {
                last_emit = std::time::Instant::now();
                let _ = app.emit(
                    "ai-model-download-progress",
                    ModelDownloadProgress { downloaded, total },
                );
            }
        });
        // The model may be the embedding one, and activities may be waiting for it.
        if matches!(downloaded, Ok(true)) {
            app.state::<Arc<dyn ActivityIndexer>>().refresh();
        }
        downloaded
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
    .map_err(IpcError::from)
}

#[tauri::command]
pub fn ai_model_cancel_download(state: State<ModelDownloadState>) {
    state.cancel();
}
