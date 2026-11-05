//! How the window looks between runs (theme, sidebar, last tabs), kept in `ui.toml` next to
//! `settings.toml`. Writes are debounced, and flushed when the app quits.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Global, Task};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::ui::route::{CategoriesTab, InsightsTab};

pub const FILE_NAME: &str = "ui.toml";
const SAVE_DELAY: Duration = Duration::from_millis(300);

/// The newest change on disk. A debounced save still running when the app quits must neither
/// interleave with the final save nor overwrite it with older preferences.
static WRITTEN: Mutex<u64> = Mutex::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    pub theme: ThemePreference,
    pub sidebar_collapsed: bool,
    pub categories_tab: CategoriesTab,
    pub insights_tab: InsightsTab,
}

pub struct Prefs {
    path: PathBuf,
    value: UiPrefs,
    generation: u64,
    dirty: bool,
    save: Task<()>,
}

impl Global for Prefs {}

impl Prefs {
    pub fn get(cx: &App) -> &UiPrefs {
        &cx.global::<Self>().value
    }

    /// Changes the preferences; the file is written shortly after the last change.
    pub fn update(cx: &mut App, change: impl FnOnce(&mut UiPrefs)) {
        let prefs = cx.global_mut::<Self>();
        let before = prefs.value.clone();
        change(&mut prefs.value);
        if prefs.value == before {
            return;
        }
        prefs.dirty = true;
        prefs.generation += 1;
        let (path, value, generation) = (prefs.path.clone(), prefs.value.clone(), prefs.generation);
        let task = cx.spawn(async move |cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let written = cx
                .background_spawn(async move { save_change(&path, &value, generation) })
                .await;
            match written {
                Ok(()) => cx.update(|cx| cx.global_mut::<Self>().dirty = false),
                Err(err) => warn!(
                    error = format!("{err:#}"),
                    "failed to save the UI preferences"
                ),
            }
        });
        cx.global_mut::<Self>().save = task;
    }

    /// Writes a change still waiting on its delay; for quitting.
    pub fn flush(cx: &mut App) {
        let prefs = cx.global_mut::<Self>();
        if !prefs.dirty {
            return;
        }
        prefs.save = Task::ready(());
        prefs.dirty = false;
        if let Err(err) = save_change(&prefs.path, &prefs.value, prefs.generation) {
            warn!(
                error = format!("{err:#}"),
                "failed to save the UI preferences"
            );
        }
    }
}

pub fn init(config_dir: &Path, cx: &mut App) {
    let path = config_dir.join(FILE_NAME);
    let value = load(&path);
    cx.set_global(Prefs {
        path,
        value,
        generation: 0,
        dirty: false,
        save: Task::ready(()),
    });
}

/// The saved preferences; defaults when the file is missing or unreadable.
pub fn load(path: &Path) -> UiPrefs {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return UiPrefs::default(),
        Err(err) => {
            warn!(error = %err, path = %path.display(), "failed to read the UI preferences");
            return UiPrefs::default();
        }
    };
    toml::from_str(&text).unwrap_or_else(|err| {
        warn!(error = %err, path = %path.display(), "ignoring unreadable UI preferences");
        UiPrefs::default()
    })
}

/// Saves change number `generation`, unless a newer one is already on disk.
fn save_change(path: &Path, prefs: &UiPrefs, generation: u64) -> Result<()> {
    let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    if generation <= *written {
        return Ok(());
    }
    save(path, prefs)?;
    *written = generation;
    Ok(())
}

pub fn save(path: &Path, prefs: &UiPrefs) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let text = toml::to_string(prefs).context("serializing the UI preferences")?;
    let staging = path.with_extension("toml.tmp");
    std::fs::write(&staging, text).with_context(|| format!("writing {}", staging.display()))?;
    std::fs::rename(&staging, path).with_context(|| format!("replacing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("makora-prefs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        assert_eq!(load(&temp_path("missing")), UiPrefs::default());
    }

    #[test]
    fn saved_prefs_load_back() {
        let path = temp_path("roundtrip");
        let prefs = UiPrefs {
            theme: ThemePreference::Dark,
            sidebar_collapsed: true,
            categories_tab: CategoriesTab::Review,
            insights_tab: InsightsTab::Logs,
        };
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path), prefs);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_keys_keep_their_defaults() {
        let prefs: UiPrefs = toml::from_str("theme = \"light\"").unwrap();
        assert_eq!(prefs.theme, ThemePreference::Light);
        assert!(!prefs.sidebar_collapsed);
        assert_eq!(prefs.categories_tab, CategoriesTab::Taxonomy);
    }

    #[test]
    fn an_unreadable_file_gives_defaults() {
        let path = temp_path("garbage");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "theme = 3").unwrap();
        assert_eq!(load(&path), UiPrefs::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
