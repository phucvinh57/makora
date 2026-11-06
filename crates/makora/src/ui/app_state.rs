//! What every screen shares: the core's services, the settings and categories, the background
//! tasks and the revision counters screens watch to know when to reload.

use std::sync::Arc;

use async_channel::Receiver;
use gpui_kit::{App, AppContext, Context, Entity, Global, Task};
use makora_lib::ai::model_download::ModelDownloadStatus;
use makora_lib::app_core::ClientCore;
use makora_lib::categorization::application::ai_run::{AiRunStatus, RunMode, RunRequest};
use makora_lib::categorization::domain::usage::RunTrigger;
use makora_lib::domain::types::Category;
use makora_lib::events::CoreEvent;
use makora_lib::settings::Settings;
use tracing::warn;

use crate::runtime::commands::{self, UiCommand};
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::bytes::format_bytes;
use crate::ui::screens::categories::ai_section::run_summary;
use crate::ui::widgets::toast::Toast;

/// The core, reachable from any view: `Core::get(cx).history.…`.
pub struct Core(pub Arc<ClientCore>);

impl Global for Core {}

impl Core {
    pub fn get(cx: &App) -> Arc<ClientCore> {
        cx.global::<Self>().0.clone()
    }
}

/// Bumped whenever data of that kind may have changed; a screen observes [`AppState`] and
/// reloads when a counter it depends on moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Revisions {
    /// Categories and rules.
    pub categories: u64,
    /// Activity labels: AI suggestions, the review queue and its counts.
    pub labels: u64,
    /// Session rows and the statistics built from them.
    pub sessions: u64,
    pub ai_usage: u64,
    pub installed_models: u64,
    /// AI providers saved or removed: their keys and model lists.
    pub providers: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revision {
    Categories,
    Labels,
    Sessions,
    AiUsage,
    InstalledModels,
    Providers,
}

impl Revisions {
    pub fn bump(&mut self, revision: Revision) {
        let counter = match revision {
            Revision::Categories => &mut self.categories,
            Revision::Labels => &mut self.labels,
            Revision::Sessions => &mut self.sessions,
            Revision::AiUsage => &mut self.ai_usage,
            Revision::InstalledModels => &mut self.installed_models,
            Revision::Providers => &mut self.providers,
        };
        *counter += 1;
    }

    /// Everything a category edit, a pin or an AI label can change.
    pub fn bump_categorization(&mut self) {
        self.bump(Revision::Categories);
        self.bump(Revision::Labels);
        self.bump(Revision::Sessions);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategorizeTask {
    pub running: bool,
    pub mode: RunMode,
    pub done: usize,
    pub total: usize,
}

impl From<AiRunStatus> for CategorizeTask {
    fn from(status: AiRunStatus) -> Self {
        Self {
            running: status.running,
            mode: status.mode,
            done: status.done,
            total: status.total,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadTask {
    pub running: bool,
    pub model: String,
    pub downloaded: u64,
    pub total: u64,
}

impl From<ModelDownloadStatus> for DownloadTask {
    fn from(status: ModelDownloadStatus) -> Self {
        Self {
            running: status.running,
            model: status.model,
            downloaded: status.downloaded,
            total: status.total,
        }
    }
}

/// One running task as the sidebar lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundTask {
    pub id: &'static str,
    pub label: String,
    pub progress: String,
}

pub struct AppState {
    pub settings: Settings,
    pub categories: Vec<Category>,
    /// Whether the categories have loaded once, and why the last load failed.
    pub categories_loaded: bool,
    pub categories_error: Option<String>,
    pub categorize: CategorizeTask,
    pub download: DownloadTask,
    pub revisions: Revisions,
    /// Why the last run started from this window failed.
    pub run_error: Option<String>,
    settings_issue: Option<String>,
    categories_load: Task<()>,
    run: Task<()>,
}

struct AppStateHandle(Entity<AppState>);

impl Global for AppStateHandle {}

impl AppState {
    /// The app-wide state entity. Views `cx.observe(&AppState::entity(cx), …)` to follow it.
    pub fn entity(cx: &App) -> Entity<Self> {
        cx.global::<AppStateHandle>().0.clone()
    }

    pub fn read(cx: &App) -> &Self {
        cx.global::<AppStateHandle>().0.read(cx)
    }

    pub fn update<R>(cx: &mut App, f: impl FnOnce(&mut Self, &mut Context<Self>) -> R) -> R {
        Self::entity(cx).update(cx, f)
    }

    /// Why the settings file could not be read at startup, handed out once.
    pub fn take_settings_issue(&mut self) -> Option<String> {
        self.settings_issue.take()
    }

    pub fn set_settings(&mut self, settings: Settings, cx: &mut Context<Self>) {
        self.settings = settings;
        cx.notify();
    }

    pub fn bump(&mut self, revision: Revision, cx: &mut Context<Self>) {
        self.revisions.bump(revision);
        if revision == Revision::Categories {
            self.reload_categories(cx);
        }
        cx.notify();
    }

    /// The categories loaded and there are none: not just a failed load.
    pub fn has_no_categories(&self) -> bool {
        self.categories_loaded && self.categories_error.is_none() && self.categories.is_empty()
    }

    /// After a category edit, a pin or a label: reloads the categories and tells every screen.
    pub fn invalidate_categorization(&mut self, cx: &mut Context<Self>) {
        self.revisions.bump_categorization();
        self.reload_categories(cx);
        cx.notify();
    }

    pub fn reload_categories(&mut self, cx: &mut Context<Self>) {
        self.categories_load = data::fetch(
            cx,
            |core| core.categories.get_list(),
            |state, result, cx| match result {
                Ok(categories) => {
                    state.categories = categories;
                    state.categories_loaded = true;
                    state.categories_error = None;
                    cx.notify();
                }
                Err(err) => {
                    warn!(error = %err, "failed to load the categories");
                    state.categories_loaded = true;
                    state.categories_error = Some(err.to_string());
                    cx.notify();
                }
            },
        );
    }

    /// A run started here or by the scheduler; the core only reports progress once it runs.
    pub fn mark_categorize_started(&mut self, mode: RunMode, cx: &mut Context<Self>) {
        self.categorize = CategorizeTask {
            running: true,
            mode,
            done: 0,
            total: 0,
        };
        cx.notify();
    }

    /// Starts `request` in the background, unless a run is going, and says how it went in a toast.
    pub fn start_run(&mut self, request: RunRequest, cx: &mut Context<Self>) {
        if self.categorize.running {
            Toast::info("A run is already going")
                .description("Try again once it ends.")
                .show_in_main_window(cx);
            return;
        }
        let mode = request.mode;
        self.run_error = None;
        self.mark_categorize_started(mode, cx);
        let job = data::blocking(cx, move |core| {
            let outcome = core.runner.run(&request, RunTrigger::Manual);
            (outcome, core.runner.status())
        });
        self.run = cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |state, cx| {
                let status = match result {
                    Some((Ok(outcome), status)) => {
                        let (title, description) = run_summary(&outcome, mode);
                        Toast::success(title).description(description).show_in_main_window(cx);
                        Some(status)
                    }
                    Some((Err(err), status)) => {
                        let message = UiError::from(err).to_string();
                        Toast::error(message.clone()).show_in_main_window(cx);
                        state.run_error = Some(message);
                        Some(status)
                    }
                    None => {
                        state.run_error = Some("The run stopped unexpectedly.".into());
                        None
                    }
                };
                state.categorize = status.map_or_else(CategorizeTask::default, Into::into);
                state.invalidate_categorization(cx);
            });
        });
    }

    pub fn mark_download_started(&mut self, model: &str, cx: &mut Context<Self>) {
        self.download = DownloadTask {
            running: true,
            model: model.to_string(),
            downloaded: 0,
            total: 0,
        };
        cx.notify();
    }

    pub fn background_tasks(&self) -> Vec<BackgroundTask> {
        background_tasks(&self.categorize, &self.download)
    }

    fn apply(&mut self, event: CoreEvent, core: &ClientCore, cx: &mut Context<Self>) {
        match event {
            CoreEvent::CategorizeStarted { mode } => self.mark_categorize_started(mode, cx),
            CoreEvent::CategorizeProgress { done, total } => {
                if !self.categorize.running {
                    self.categorize = core.runner.status().into();
                }
                self.categorize.running = true;
                self.categorize.done = done;
                self.categorize.total = total;
            }
            CoreEvent::CategorizeFinished => {
                self.categorize.running = false;
                self.revisions.bump_categorization();
                self.revisions.bump(Revision::AiUsage);
                self.reload_categories(cx);
            }
            CoreEvent::DownloadProgress { downloaded, total } => {
                if !self.download.running {
                    self.download = core.downloads.status().into();
                }
                self.download.running = true;
                self.download.downloaded = downloaded;
                self.download.total = total;
            }
            CoreEvent::DownloadFinished => {
                self.download.running = false;
                self.revisions.bump(Revision::InstalledModels);
            }
            CoreEvent::Navigate(target) => {
                let command = UiCommand::from(target);
                cx.defer(move |cx| commands::handle(command, cx));
            }
        }
        cx.notify();
    }
}

pub fn background_tasks(
    categorize: &CategorizeTask,
    download: &DownloadTask,
) -> Vec<BackgroundTask> {
    let mut tasks = Vec::new();
    if categorize.running {
        tasks.push(BackgroundTask {
            id: "categorize",
            label: match categorize.mode {
                RunMode::Recheck => "Re-checking AI labels",
                RunMode::Label => "Categorizing activities",
                RunMode::FindCategories => "Looking for new categories",
            }
            .into(),
            progress: if categorize.total > 0 {
                format!("{}/{}", categorize.done, categorize.total)
            } else {
                "Starting…".into()
            },
        });
    }
    if download.running {
        tasks.push(BackgroundTask {
            id: "download",
            label: format!("Downloading {}", download.model),
            progress: if download.total > 0 {
                format!(
                    "{} of {}",
                    format_bytes(download.downloaded),
                    format_bytes(download.total)
                )
            } else {
                "Starting…".into()
            },
        });
    }
    tasks
}

/// Builds the state from the core and starts applying its events.
pub fn init(events: Receiver<CoreEvent>, cx: &mut App) {
    let core = Core::get(cx);
    let state = cx.new(|cx| {
        let mut state = AppState {
            settings: core.settings.get(),
            categories: Vec::new(),
            categories_loaded: false,
            categories_error: None,
            categorize: core.runner.status().into(),
            download: core.downloads.status().into(),
            revisions: Revisions::default(),
            run_error: None,
            settings_issue: core.settings.take_load_issue(),
            categories_load: Task::ready(()),
            run: Task::ready(()),
        };
        state.reload_categories(cx);
        state
    });
    cx.set_global(AppStateHandle(state.clone()));

    cx.spawn(async move |cx| {
        while let Ok(event) = events.recv().await {
            let core = core.clone();
            cx.update(|cx| state.update(cx, |state, cx| state.apply(event, &core, cx)));
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tasks_while_idle() {
        assert!(background_tasks(&CategorizeTask::default(), &DownloadTask::default()).is_empty());
    }

    #[test]
    fn a_task_without_a_total_is_starting() {
        let categorize = CategorizeTask {
            running: true,
            mode: RunMode::Recheck,
            ..Default::default()
        };
        let tasks = background_tasks(&categorize, &DownloadTask::default());
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].label, "Re-checking AI labels");
        assert_eq!(tasks[0].progress, "Starting…");
    }

    #[test]
    fn progress_reads_done_over_total_and_bytes() {
        let categorize = CategorizeTask {
            running: true,
            mode: RunMode::Label,
            done: 3,
            total: 10,
        };
        let download = DownloadTask {
            running: true,
            model: "llama3.2:3b".into(),
            downloaded: 512 * 1024 * 1024,
            total: 2 * 1024 * 1024 * 1024,
        };
        let tasks = background_tasks(&categorize, &download);
        assert_eq!(tasks[0].label, "Categorizing activities");
        assert_eq!(tasks[0].progress, "3/10");
        assert_eq!(tasks[1].label, "Downloading llama3.2:3b");
        assert_eq!(
            tasks[1].progress,
            format!(
                "{} of {}",
                format_bytes(512 * 1024 * 1024),
                format_bytes(2 << 30)
            )
        );
    }

    #[test]
    fn categorization_bumps_what_a_label_can_change() {
        let mut revisions = Revisions::default();
        revisions.bump_categorization();
        assert_eq!(
            (revisions.categories, revisions.labels, revisions.sessions),
            (1, 1, 1)
        );
        assert_eq!((revisions.ai_usage, revisions.installed_models), (0, 0));
    }
}
