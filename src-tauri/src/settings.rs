//! App-wide preferences, kept in a TOML file the person can also edit by hand.
//!
//! The file is read once at startup. Changes made through the settings page are applied at once
//! and written back; edits made to the file while the app runs are picked up on the next launch.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::tracking::domain::session_policy::{DEFAULT_MIN_SESSION_SECONDS, MIN_SESSION_SECONDS_RANGE};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub tracking: TrackingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackingSettings {
    /// Activity that ends sooner than this after it began is discarded.
    pub min_session_seconds: i64,
}

impl Default for TrackingSettings {
    fn default() -> Self {
        Self {
            min_session_seconds: DEFAULT_MIN_SESSION_SECONDS,
        }
    }
}

pub trait SettingsStore: Send + Sync {
    fn get(&self) -> Settings;
    /// Rejects a value outside [`MIN_SESSION_SECONDS_RANGE`]; otherwise applies it and saves the
    /// file. Returns the settings as they now stand.
    fn set_min_session_seconds(&self, seconds: i64) -> Result<Settings>;
}

pub struct TomlSettings {
    path: PathBuf,
    current: RwLock<Settings>,
}

impl TomlSettings {
    /// Reads the file at `path`. A missing file is created with the defaults so there is
    /// something to find and edit. One that cannot be read or parsed is left exactly as it is
    /// and the defaults are used, so a typo never costs the person the rest of their file.
    pub fn load(path: PathBuf) -> Self {
        let mut settings = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                warn!(path = %path.display(), error = %err, "settings file is malformed; using defaults");
                Settings::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let defaults = Settings::default();
                match write_atomically(&path, &defaults) {
                    Ok(()) => info!(path = %path.display(), "created settings file"),
                    Err(err) => warn!(path = %path.display(), error = format!("{err:#}"), "could not create settings file"),
                }
                defaults
            }
            Err(err) => {
                warn!(path = %path.display(), error = %err, "could not read settings file; using defaults");
                Settings::default()
            }
        };

        let seconds = settings.tracking.min_session_seconds;
        if !MIN_SESSION_SECONDS_RANGE.contains(&seconds) {
            let clamped = seconds.clamp(*MIN_SESSION_SECONDS_RANGE.start(), *MIN_SESSION_SECONDS_RANGE.end());
            warn!(seconds, clamped, "tracking.min_session_seconds is out of range; clamping");
            settings.tracking.min_session_seconds = clamped;
        }

        Self {
            path,
            current: RwLock::new(settings),
        }
    }
}

impl SettingsStore for TomlSettings {
    fn get(&self) -> Settings {
        self.current.read().unwrap().clone()
    }

    fn set_min_session_seconds(&self, seconds: i64) -> Result<Settings> {
        if !MIN_SESSION_SECONDS_RANGE.contains(&seconds) {
            bail!(
                "minimum session length must be between {} and {} seconds",
                MIN_SESSION_SECONDS_RANGE.start(),
                MIN_SESSION_SECONDS_RANGE.end()
            );
        }
        // Held across the write so two saves cannot reach the file out of order.
        let mut current = self.current.write().unwrap();
        let mut next = current.clone();
        next.tracking.min_session_seconds = seconds;
        write_atomically(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

fn write_atomically(path: &Path, settings: &Settings) -> Result<()> {
    let text = toml::to_string_pretty(settings).context("serializing settings")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}
