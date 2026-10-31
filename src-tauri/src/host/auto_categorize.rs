//! Runs AI categorization on its own every `ai.auto_categorize_hours`, on the hour, so suggestions
//! pile up for review instead of waiting for someone to open the AI tab, and notifies when a run
//! has labeled something.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, DurationRound, Local, Utc};
use notify_rust::{Notification, NotificationResponse};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::ai::llm;
use crate::categorization::application::ai_run::{AiRunError, AiRunState};
use crate::categorization::domain::usage::RunTrigger;
use crate::domain::LabelingSvc;
use crate::host::commands::categorization::run_categorization;
use crate::host::tray;
use crate::settings::SettingsStore;
use crate::shared::time;
use crate::taxonomy::CategorySvc;

/// Left alone after launch so the app's own start-up work finishes first.
const STARTUP_DELAY: Duration = Duration::from_secs(120);
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// Where clicking the notification lands: the AI review tab.
const REVIEW_ROUTE: &str = "/categories?tab=review";

/// What the schedule counts from: the end of the last labeling run, or when the period was last
/// set. Kept in a file so a restart does not start the clock over.
pub struct ScheduleAnchor {
    path: PathBuf,
}

impl ScheduleAnchor {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn get(&self) -> Option<DateTime<Utc>> {
        time::parse_db(std::fs::read_to_string(&self.path).ok()?.trim())
    }

    pub fn set(&self, at: DateTime<Utc>) {
        if let Err(err) = std::fs::write(&self.path, time::to_db(at)) {
            warn!(path = %self.path.display(), %err, "could not record where the categorization schedule starts");
        }
    }
}

/// When the scheduler is due to label next, or `None` while it is off: one period after the
/// anchor, rounded down to the local hour, so a 3-hour period set at 11:30 first runs at 14:00.
/// A run that is overdue is due now: the scheduler picks it up on its next check.
pub fn next_run(anchor: &ScheduleAnchor, hours: u32, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if hours == 0 {
        return None;
    }
    let due = anchor.get().unwrap_or(now) + chrono::Duration::hours(hours.into());
    let on_the_hour = due
        .with_timezone(&Local)
        .duration_trunc(chrono::Duration::hours(1))
        .map_or(due, |hour| hour.with_timezone(&Utc));
    Some(on_the_hour.max(now))
}

#[derive(Serialize, Clone)]
struct Started {
    recheck: bool,
}

pub fn spawn(app: AppHandle) -> std::io::Result<()> {
    // With nothing to count from, the first run waits a period instead of starting right away.
    let anchor = app.state::<ScheduleAnchor>();
    if anchor.get().is_none() {
        anchor.set(Utc::now());
    }
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
    let anchor = app.state::<ScheduleAnchor>();
    let now = Utc::now();
    if next_run(&anchor, hours, now).is_some_and(|due| due > now) {
        return;
    }
    if app.state::<AiRunState>().status().running {
        return;
    }

    let llm_ready = match settings.ai.llm_provider.as_deref() {
        Some(id) => settings.ai.provider(id).is_some(),
        None => llm::resolve_model(&settings.ai.llm_model).is_ok(),
    };
    if !llm_ready || llm::resolve_model(&settings.ai.embedding_model).is_err() {
        return;
    }
    if !matches!(app.state::<Box<dyn CategorySvc>>().get_list(), Ok(list) if !list.is_empty()) {
        return;
    }
    match app.state::<Box<dyn LabelingSvc>>().get_activities_to_label(false) {
        Ok(pending) if pending.is_empty() => {
            anchor.set(Utc::now());
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
    match run_categorization(app, false, RunTrigger::Scheduled) {
        Ok(outcome) => {
            info!(labeled = outcome.labeled, skipped = outcome.skipped, "scheduled AI categorization finished");
            if outcome.labeled > 0 {
                notify(app, outcome.labeled);
            }
        }
        Err(AiRunError::AlreadyRunning) => {}
        Err(err) => {
            warn!(error = %err, "scheduled AI categorization failed");
            anchor.set(Utc::now());
        }
    }
}

fn notify(app: &AppHandle, labeled: usize) {
    let noun = if labeled == 1 { "activity" } else { "activities" };
    let mut notification = Notification::new();
    notification
        .summary("AI categorization finished")
        .body(&format!("{labeled} {noun} got a suggested category. Click to review them."))
        .auto_icon();
    // Freedesktop servers only report a click on the body when the "default" action is offered.
    #[cfg(all(unix, not(target_os = "macos")))]
    notification.action("default", "Review");
    // Windows only knows the app's ID once it is installed; a build run from `target/` has none.
    #[cfg(windows)]
    {
        let unbundled = std::env::current_exe().is_ok_and(|exe| {
            exe.parent()
                .is_some_and(|dir| dir.ends_with("target/debug") || dir.ends_with("target/release"))
        });
        if !unbundled {
            notification.app_id(&app.config().identifier);
        }
    }
    #[cfg(target_os = "macos")]
    let _ = notify_rust::set_application(if tauri::is_dev() {
        "com.apple.Terminal"
    } else {
        &app.config().identifier
    });

    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("makora-notification".into())
        .spawn(move || {
            let clicked = notification.show().and_then(|handle| {
                handle.wait_for_response(|response: &NotificationResponse| {
                    if response.is_default_action() {
                        tray::show_main_window_at(&app, REVIEW_ROUTE);
                    }
                })
            });
            if let Err(err) = clicked {
                warn!(%err, "could not show the notification");
            }
        });
    if let Err(err) = spawned {
        warn!(%err, "could not spawn the notification thread");
    }
}
