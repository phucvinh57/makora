//! Where Makora keeps its files: `<data dir>/<APP_ID>` and `<config dir>/<APP_ID>`, the same
//! directories the former Tauri build resolved from its bundle identifier, so existing data stays
//! where it is.

use std::path::PathBuf;

use anyhow::{Context, Result};

pub const APP_ID: &str = "dev.phucvinh57.makora";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl AppPaths {
    /// The platform directories, which debug builds let `MAKORA_DATA_DIR` and
    /// `MAKORA_CONFIG_DIR` replace so they can run on fixture data.
    pub fn resolve() -> Result<Self> {
        let platform = Self::platform()?;
        if !cfg!(debug_assertions) {
            return Ok(platform);
        }
        let from_env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
        Ok(Self {
            data_dir: from_env("MAKORA_DATA_DIR").unwrap_or(platform.data_dir),
            config_dir: from_env("MAKORA_CONFIG_DIR").unwrap_or(platform.config_dir),
        })
    }

    pub fn platform() -> Result<Self> {
        Ok(Self {
            data_dir: dirs::data_dir().context("the platform has no data directory")?.join(APP_ID),
            config_dir: dirs::config_dir().context("the platform has no config directory")?.join(APP_ID),
        })
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir.join("makora.sqlite")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_id_is_the_identifier_existing_installs_store_their_data_under() {
        assert_eq!(APP_ID, "dev.phucvinh57.makora");
    }

    #[test]
    fn platform_paths_sit_under_the_app_id() {
        let paths = AppPaths::platform().unwrap();
        assert_eq!(paths.data_dir, dirs::data_dir().unwrap().join("dev.phucvinh57.makora"));
        assert_eq!(paths.config_dir, dirs::config_dir().unwrap().join("dev.phucvinh57.makora"));
        assert_eq!(paths.database().file_name().unwrap(), "makora.sqlite");
    }
}
