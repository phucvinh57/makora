//! Process-wide `tracing` setup.
//!
//! Two layers share one registry. The developer log goes to stderr in debug builds, so
//! `bun tauri dev` shows events in the terminal; release builds have no terminal at all
//! (`windows_subsystem = "windows"`), so it goes to `<app-data>/logs/makora.<date>.log` instead.
//! The app log ([`AppLogLayer`]) always goes to `<app-data>/logs/events.<date>.jsonl`, with the
//! same filter in every build, for the in-app log viewer. Both rotate daily and keep the last
//! [`MAX_LOG_FILES`] days.
//!
//! Must be called from the Tauri `setup` hook like every other process-wide
//! initializer — a second launch dies inside the single-instance plugin, before
//! `setup`, and must not touch the log directory the live instance is writing to.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::logs::{AppLogLayer, EVENTS_FILE_PREFIX, EVENTS_FILE_SUFFIX};
use crate::shared::sync::lock;

pub const LOG_DIR_NAME: &str = "logs";

const LOG_FILE_PREFIX: &str = "makora";
const LOG_FILE_SUFFIX: &str = "log";
const MAX_LOG_FILES: usize = 30;

const DEV_FILTER: &str = "makora=debug,makora_lib=debug,warn";
const PROD_FILTER: &str = "makora=info,makora_lib=info,warn";
const APP_LOG_FILTER: &str = "makora_lib=info,warn";

/// The non-blocking writers' flush guards. Dropping one flushes and stops its background
/// writer thread, so they live until [`flush`] at exit.
static GUARDS: Mutex<Vec<WorkerGuard>> = Mutex::new(Vec::new());

pub fn log_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(LOG_DIR_NAME)
}

/// Installs the global subscriber. Returns the developer log's directory in release builds
/// and `None` in debug builds, where it goes to stderr instead.
pub fn init(app_data_dir: &Path) -> Result<Option<PathBuf>> {
    let dir = log_dir(app_data_dir);
    let (events, events_guard) = daily_writer(&dir, EVENTS_FILE_PREFIX, EVENTS_FILE_SUFFIX)?;
    let app_log = AppLogLayer::new(events).with_filter(EnvFilter::new(APP_LOG_FILTER));

    let registry = tracing_subscriber::registry().with(app_log);
    if cfg!(debug_assertions) {
        lock(&GUARDS).push(events_guard);
        let dev_log = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_filter(filter(DEV_FILTER));
        registry
            .with(dev_log)
            .try_init()
            .map_err(|err| anyhow::anyhow!("installing the stderr log subscriber: {err}"))?;
        return Ok(None);
    }

    let (writer, guard) = daily_writer(&dir, LOG_FILE_PREFIX, LOG_FILE_SUFFIX)?;
    lock(&GUARDS).extend([events_guard, guard]);
    let file_log = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_filter(filter(PROD_FILTER));
    registry
        .with(file_log)
        .try_init()
        .map_err(|err| anyhow::anyhow!("installing the file log subscriber: {err}"))?;

    Ok(Some(dir))
}

/// Writes out every buffered event before the process exits; nothing is logged to a file after.
pub fn flush() {
    drop(std::mem::take(&mut *lock(&GUARDS)));
}

fn daily_writer(dir: &Path, prefix: &str, suffix: &str) -> Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating log directory {}", dir.display()))?;

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(prefix)
        .filename_suffix(suffix)
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .with_context(|| format!("opening a log file in {}", dir.display()))?;

    Ok(tracing_appender::non_blocking(appender))
}

fn filter(default: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::layer::tests::Buffer;

    #[test]
    fn the_app_log_keeps_makora_info_and_anyone_else_s_warnings() {
        let buffer = Buffer::default();
        let app_log = AppLogLayer::new(buffer.clone()).with_filter(EnvFilter::new(APP_LOG_FILTER));
        let subscriber = tracing_subscriber::registry().with(app_log);
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "makora_lib::tracking", "focus changed");
            tracing::info!(target: "makora_lib::settings", "setting changed");
            tracing::info!(target: "ureq::pool", "connection reused");
            tracing::warn!(target: "ureq::pool", "connection dropped");
        });

        let text = buffer.text();
        assert!(text.contains("setting changed") && text.contains("connection dropped"));
        assert!(!text.contains("focus changed") && !text.contains("connection reused"));
    }
}
