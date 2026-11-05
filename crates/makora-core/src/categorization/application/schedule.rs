//! Runs AI categorization on its own every `ai.auto_categorize_hours`, on the hour, so suggestions
//! pile up for review instead of waiting for someone to open the AI tab, and notifies when a run
//! has labeled something.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, DurationRound, Local, Utc};
use notify_rust::{Notification, NotificationResponse};
use tracing::{info, warn};

use crate::ai::llm;
use crate::categorization::application::ai_run::AiRunError;
use crate::categorization::application::runner::CategorizationRunner;
use crate::categorization::domain::usage::RunTrigger;
use crate::domain::LabelingSvc;
use crate::events::{CoreEvent, EventSink, NavTarget};
#[cfg(any(windows, target_os = "macos"))]
use crate::paths::APP_ID;
use crate::settings::SettingsStore;
use crate::shared::time;
use crate::taxonomy::CategorySvc;

/// Left alone after launch so the app's own start-up work finishes first.
const STARTUP_DELAY: Duration = Duration::from_secs(120);
const CHECK_EVERY: Duration = Duration::from_secs(60);

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

pub trait AutoCategorizeSchedule: Send + Sync {
    /// When the next scheduled run is due, `None` while automatic categorization is off.
    fn next_run(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>>;
    /// Starts the period over: the next run comes one period from now, on the hour.
    fn restart(&self);
    /// Checks every minute, on a thread of its own, whether a run is due.
    fn start(self: Arc<Self>) -> io::Result<()>;
}

pub struct ScheduledCategorization {
    pub anchor: Arc<ScheduleAnchor>,
    pub settings: Arc<dyn SettingsStore>,
    pub runner: Arc<dyn CategorizationRunner>,
    pub categories: Arc<dyn CategorySvc>,
    pub labeling: Arc<dyn LabelingSvc>,
    pub events: Arc<dyn EventSink>,
}

impl AutoCategorizeSchedule for ScheduledCategorization {
    fn next_run(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        next_run(&self.anchor, self.settings.get().ai.auto_categorize_hours, now)
    }

    fn restart(&self) {
        self.anchor.set(Utc::now());
    }

    fn start(self: Arc<Self>) -> io::Result<()> {
        // With nothing to count from, the first run waits a period instead of starting right away.
        if self.anchor.get().is_none() {
            self.anchor.set(Utc::now());
        }
        std::thread::Builder::new()
            .name("makora-ai-scheduler".into())
            .spawn(move || {
                std::thread::sleep(STARTUP_DELAY);
                loop {
                    self.check();
                    std::thread::sleep(CHECK_EVERY);
                }
            })?;
        Ok(())
    }
}

impl ScheduledCategorization {
    fn check(&self) {
        let settings = self.settings.get();
        let hours = settings.ai.auto_categorize_hours;
        if hours == 0 {
            return;
        }
        let now = Utc::now();
        if next_run(&self.anchor, hours, now).is_some_and(|due| due > now) {
            return;
        }
        if self.runner.status().running {
            return;
        }

        let llm_ready = match settings.ai.llm_provider.as_deref() {
            Some(id) => settings.ai.provider(id).is_some(),
            None => llm::resolve_model(&settings.ai.llm_model).is_ok(),
        };
        if !llm_ready || llm::resolve_model(&settings.ai.embedding_model).is_err() {
            return;
        }
        if !matches!(self.categories.get_list(), Ok(list) if !list.is_empty()) {
            return;
        }
        match self.labeling.get_activities_to_label(false) {
            Ok(pending) if pending.is_empty() => {
                self.anchor.set(Utc::now());
                return;
            }
            Ok(_) => {}
            Err(err) => {
                warn!(error = format!("{err:#}"), "could not look for activities to categorize");
                return;
            }
        }

        info!(hours, "starting scheduled AI categorization");
        self.events.emit(CoreEvent::CategorizeStarted { recheck: false });
        match self.runner.run(false, RunTrigger::Scheduled) {
            Ok(outcome) => {
                if outcome.labeled > 0 {
                    notify(self.events.clone(), outcome.labeled);
                }
            }
            Err(AiRunError::AlreadyRunning) => {}
            Err(err) => {
                warn!(error = %err, "scheduled AI categorization failed");
                self.anchor.set(Utc::now());
            }
        }
    }
}

fn notify(events: Arc<dyn EventSink>, labeled: usize) {
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
            notification.app_id(APP_ID);
        }
    }
    #[cfg(target_os = "macos")]
    let _ = notify_rust::set_application(if cfg!(debug_assertions) {
        "com.apple.Terminal"
    } else {
        APP_ID
    });

    let spawned = std::thread::Builder::new()
        .name("makora-notification".into())
        .spawn(move || {
            let clicked = notification.show().and_then(|handle| {
                handle.wait_for_response(|response: &NotificationResponse| {
                    if response.is_default_action() {
                        events.emit(CoreEvent::Navigate(NavTarget::CategoriesReview));
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
