//! Where Makora keeps its files: `<data dir>/<APP_ID>` and `<config dir>/<APP_ID>`, the same
//! directories Tauri resolved from the bundle identifier, so existing data stays where it is.

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
    fn the_app_id_is_the_bundle_identifier() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], APP_ID);
        assert!(config["app"].get("appDirectoriesOverride").is_none());
    }
}
