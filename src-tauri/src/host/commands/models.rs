use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::llm::{self, ModelCache};
use crate::ai::model_download::{ModelDownloadState, ModelDownloadStatus};
use crate::host::error::IpcError;

#[derive(Serialize, Clone)]
struct ModelDownloadProgress {
    downloaded: u64,
    total: u64,
}

/// Whether the model is currently held in memory.
#[tauri::command]
pub fn ai_model_loaded(models: State<ModelCache>) -> bool {
    models.is_loaded()
}

/// Releases the cached model's memory.
#[tauri::command]
pub fn unload_ai_model(models: State<ModelCache>) {
    models.unload();
}

/// Whether `model` can be loaded right now (downloaded here, pulled by Ollama, or a file).
#[tauri::command]
pub fn ai_model_installed(model: String) -> bool {
    llm::resolve_model(model.trim()).is_ok()
}

/// Lists models available to categorize with right now: those downloaded into Makora or pulled
/// by Ollama.
#[tauri::command]
pub fn list_installed_ai_models() -> Vec<String> {
    llm::installed_models()
}

/// Lets a page that reloaded mid-download find out it is still going and resync its progress
/// bar, instead of just losing track of it.
#[tauri::command]
pub fn model_download_status(state: State<ModelDownloadState>) -> ModelDownloadStatus {
    state.status()
}

/// Downloads an Ollama-library model (`llama3.2:3b`) and reports `ai-model-download-progress`
/// events. Returns `false` when the user stopped it; the partial file is kept for resuming.
#[tauri::command]
pub async fn download_ai_model(app: AppHandle, model: String) -> Result<bool, IpcError> {
    let model = model.trim().to_string();
    let root = llm::download_root().ok_or_else(|| IpcError::internal("model directory is not ready"))?;

    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ModelDownloadState>();
        let mut last_emit = std::time::Instant::now();
        state.run(root, &model, &mut |downloaded, total| {
            // ~10 events a second is plenty for a progress bar.
            if last_emit.elapsed() >= std::time::Duration::from_millis(100) || downloaded == total {
                last_emit = std::time::Instant::now();
                let _ = app.emit(
                    "ai-model-download-progress",
                    ModelDownloadProgress { downloaded, total },
                );
            }
        })
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
    .map_err(IpcError::from)
}

/// Asks a running model download to stop.
#[tauri::command]
pub fn cancel_model_download(state: State<ModelDownloadState>) {
    state.cancel();
}
