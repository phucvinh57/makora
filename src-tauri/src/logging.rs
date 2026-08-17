//! Process-wide `tracing` setup.
//!
//! Debug builds write to stderr, so `bun tauri dev` shows events in the terminal.
//! Release builds have no terminal at all (`windows_subsystem = "windows"`), so they
//! write to `<app-data>/logs/makora.<date>.log`, rotated daily and pruned to the last
//! [`MAX_LOG_FILES`] days.
//!
//! Must be called from the Tauri `setup` hook like every other process-wide
//! initializer — a second launch dies inside the single-instance plugin, before
//! `setup`, and must not touch the log directory the live instance is writing to.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

/// Directory under the app-data dir that holds the rotated log files.
pub const LOG_DIR_NAME: &str = "logs";

const LOG_FILE_PREFIX: &str = "makora";
const LOG_FILE_SUFFIX: &str = "log";
const MAX_LOG_FILES: usize = 30;

const DEV_FILTER: &str = "makora=debug,makora_lib=debug,warn";
const PROD_FILTER: &str = "makora=info,makora_lib=info,warn";

/// The non-blocking writer's flush guard. Dropping it stops the background writer
/// thread, so it has to outlive every logging call — i.e. live until the process exits.
static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Installs the global subscriber. Returns the log directory in release builds and
/// `None` in debug builds, where events go to stderr instead.
pub fn init(app_data_dir: &Path) -> Result<Option<PathBuf>> {
    if cfg!(debug_assertions) {
        tracing_subscriber::fmt()
            .with_env_filter(filter(DEV_FILTER))
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .try_init()
            .map_err(|err| anyhow::anyhow!("installing the stderr log subscriber: {err}"))?;
        return Ok(None);
    }

    let dir = app_data_dir.join(LOG_DIR_NAME);
    let (writer, guard) = daily_writer(&dir)?;
    let _ = GUARD.set(guard);

    tracing_subscriber::fmt()
        .with_env_filter(filter(PROD_FILTER))
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .map_err(|err| anyhow::anyhow!("installing the file log subscriber: {err}"))?;

    Ok(Some(dir))
}

/// Opens `<dir>/makora.<yyyy-mm-dd>.log`, creating `dir` if needed. The appender rolls
/// over on its own at midnight and deletes everything past the newest [`MAX_LOG_FILES`].
fn daily_writer(dir: &Path) -> Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating log directory {}", dir.display()))?;

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
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
    use std::io::Write;
    use tracing_subscriber::fmt::MakeWriter;

    #[test]
    fn the_file_writer_partitions_by_date() {
        let dir = std::env::temp_dir().join(format!("makora-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let (writer, guard) = daily_writer(&dir).expect("opening the log directory");
        writer
            .make_writer()
            .write_all(b"hello\n")
            .expect("writing a log line");
        drop(guard); // flushes the background writer thread

        let names: Vec<_> = std::fs::read_dir(&dir)
            .expect("reading the log directory")
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "expected one log file, got {names:?}");

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(
            names[0],
            format!("{LOG_FILE_PREFIX}.{today}.{LOG_FILE_SUFFIX}")
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(&names[0])).expect("reading the log file"),
            "hello\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
