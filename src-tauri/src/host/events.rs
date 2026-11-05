//! The core's events as the webview hears them.

use makora_lib::events::{CoreEvent, EventSink, NavTarget};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::host::tray;

pub struct TauriEvents {
    app: AppHandle,
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[derive(Serialize, Clone)]
struct Started {
    recheck: bool,
}

#[derive(Serialize, Clone)]
struct CategorizeProgress {
    done: usize,
    total: usize,
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

impl EventSink for TauriEvents {
    fn emit(&self, event: CoreEvent) {
        let _ = match event {
            CoreEvent::CategorizeStarted { recheck } => self.app.emit("ai-categorize-started", Started { recheck }),
            CoreEvent::CategorizeProgress { done, total } => {
                self.app.emit("ai-categorize-progress", CategorizeProgress { done, total })
            }
            CoreEvent::DownloadProgress { downloaded, total } => {
                self.app.emit("ai-model-download-progress", DownloadProgress { downloaded, total })
            }
            CoreEvent::Navigate(NavTarget::CategoriesReview) => {
                tray::show_main_window_at(&self.app, "/categories?tab=review");
                Ok(())
            }
            CoreEvent::CategorizeFinished | CoreEvent::DownloadFinished => Ok(()),
        };
    }
}
