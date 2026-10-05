//! Runs AI categorization on its own every `ai.auto_categorize_hours`, so suggestions pile up
//! for review instead of waiting for someone to open the AI tab, and notifies when a run has
//! labeled something.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tracing::{info, warn};

use crate::ai::llm;
use crate::categorization::application::ai_run::{AiRunError, AiRunState};
use crate::domain::LabelingSvc;
use crate::host::commands::categorization::run_categorization;
use crate::settings::SettingsStore;
use crate::shared::time;
use crate::taxonomy::CategorySvc;

/// Left alone after launch so the app's own start-up work finishes first.
const STARTUP_DELAY: Duration = Duration::from_secs(120);
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// When the last labeling run ended, kept in a file so a restart does not start the clock over.
pub struct LastAutoRun {
    path: PathBuf,
}

impl LastAutoRun {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn get(&self) -> Option<DateTime<Utc>> {
        time::parse_db(std::fs::read_to_string(&self.path).ok()?.trim())
    }

    pub fn set(&self, at: DateTime<Utc>) {
        if let Err(err) = std::fs::write(&self.path, time::to_db(at)) {
            warn!(path = %self.path.display(), %err, "could not record the last categorization run");
        }
    }
}

/// When the scheduler is due to label next, or `None` while it is off. A first run, or one that
/// is overdue, is due now: the scheduler picks it up on its next check.
pub fn next_run(last_run: &LastAutoRun, hours: u32, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if hours == 0 {
        return None;
    }
    Some(match last_run.get() {
        Some(last) => (last + chrono::Duration::hours(hours.into())).max(now),
        None => now,
    })
}

#[derive(Serialize, Clone)]
struct Started {
    recheck: bool,
}

pub fn spawn(app: AppHandle) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("makora-ai-scheduler".into())
        .spawn(move || {
            std::thread::sleep(STARTUP_DELAY);
            loop {
                check(&app);
                std::thread::sleep(CHECK_EVERY);
            }
        })?;
    Ok(())
}

fn check(app: &AppHandle) {
    let settings = app.state::<Arc<dyn SettingsStore>>().get();
    let hours = settings.ai.auto_categorize_hours;
    if hours == 0 {
        return;
    }
    let last_run = app.state::<LastAutoRun>();
    if last_run
        .get()
        .is_some_and(|last| Utc::now() - last < chrono::Duration::hours(hours.into()))
    {
        return;
    }
    if app.state::<AiRunState>().status().running {
        return;
    }

    if llm::resolve_model(&settings.ai.llm_model).is_err() || llm::resolve_model(&settings.ai.embedding_model).is_err() {
        return;
    }
    if !matches!(app.state::<Box<dyn CategorySvc>>().get_list(), Ok(list) if !list.is_empty()) {
        return;
    }
    match app.state::<Box<dyn LabelingSvc>>().get_activities_to_label(false) {
        Ok(pending) if pending.is_empty() => {
            last_run.set(Utc::now());
            return;
        }
        Ok(_) => {}
        Err(err) => {
            warn!(error = format!("{err:#}"), "could not look for activities to categorize");
            return;
        }
    }

    info!(hours, "starting scheduled AI categorization");
    let _ = app.emit("ai-categorize-started", Started { recheck: false });
    match run_categorization(app, false) {
        Ok(outcome) => {
            info!(labeled = outcome.labeled, skipped = outcome.skipped, "scheduled AI categorization finished");
            if outcome.labeled > 0 {
                notify(app, outcome.labeled);
            }
        }
        Err(AiRunError::AlreadyRunning) => {}
        Err(err) => {
            warn!(error = %err, "scheduled AI categorization failed");
            last_run.set(Utc::now());
        }
    }
}

fn notify(app: &AppHandle, labeled: usize) {
    let noun = if labeled == 1 { "activity" } else { "activities" };
    let shown = app
        .notification()
        .builder()
        .title("AI categorization finished")
        .body(format!("{labeled} {noun} got a suggested category. Open Categories to review them."))
        .show();
    if let Err(err) = shown {
        warn!(%err, "could not show the notification");
    }
}
