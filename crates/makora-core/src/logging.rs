//! Process-wide `tracing` setup.
//!
//! Two layers share one registry. The developer log goes to stderr in debug builds, so
//! `cargo run -p makora` shows events in the terminal; release builds have no terminal at all
//! (`windows_subsystem = "windows"`), so it goes to `<app-data>/logs/makora.<date>.log` instead.
//! The network log ([`NetworkLogLayer`]) always goes to `<app-data>/logs/events.<date>.jsonl`
//! for the Monitoring page. Both rotate daily and keep the last [`MAX_LOG_FILES`] days.
//!
//! Must be called in `main` after the single-instance check, like every other process-wide
//! initializer: a second launch exits at that check and must not touch the log directory the
//! live instance is writing to.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::{EnvFilter, Layer};

use crate::network_log::{NetworkLogLayer, EVENTS_FILE_PREFIX, EVENTS_FILE_SUFFIX, NETWORK_TARGET};
use crate::shared::sync::lock;

pub const LOG_DIR_NAME: &str = "logs";

const LOG_FILE_PREFIX: &str = "makora";
const LOG_FILE_SUFFIX: &str = "log";
/// A day more than the Monitoring page's longest period, which can span 31 dates.
const MAX_LOG_FILES: usize = 31;

const DEV_FILTER: &str = "makora=debug,makora_lib=debug,warn";
const PROD_FILTER: &str = "makora=info,makora_lib=info,warn";

/// The non-blocking writers' flush guards. Dropping one flushes and stops its background
/// writer thread, so they live until [`flush`] at exit.
static GUARDS: Mutex<Vec<WorkerGuard>> = Mutex::new(Vec::new());

pub fn log_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(LOG_DIR_NAME)
}

/// Installs the global subscriber.
pub fn init(app_data_dir: &Path) -> Result<()> {
    let dir = log_dir(app_data_dir);
    let (events, events_guard) = daily_writer(&dir, EVENTS_FILE_PREFIX, EVENTS_FILE_SUFFIX)?;
    let network_log = NetworkLogLayer::new(events).with_filter(Targets::new().with_target(NETWORK_TARGET, tracing::Level::INFO));

    let registry = tracing_subscriber::registry().with(network_log);
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
        return Ok(());
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

    Ok(())
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
