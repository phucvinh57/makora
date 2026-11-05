//! "Run when system starts", through the same login entry the Tauri autostart plugin wrote: a
//! `Makora` entry for the current executable (a LaunchAgent on macOS).

use std::sync::Arc;

use anyhow::{Context, Result};
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use gpui_kit::{App, Global};

pub const APP_NAME: &str = "Makora";

pub trait Autostart: Send + Sync {
    fn is_enabled(&self) -> Result<bool>;
    fn set_enabled(&self, enabled: bool) -> Result<()>;
}

pub struct LoginItem {
    entry: AutoLaunch,
}

impl LoginItem {
    pub fn for_current_exe() -> Result<Self> {
        let exe = std::env::current_exe().context("locating the running executable")?;
        let mut builder = AutoLaunchBuilder::new();
        builder.set_app_name(APP_NAME);

        #[cfg(target_os = "macos")]
        {
            builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
            let exe = exe
                .canonicalize()
                .context("resolving the running executable")?;
            builder.set_app_path(&exe.display().to_string());
        }
        #[cfg(target_os = "linux")]
        match std::env::var("APPIMAGE") {
            Ok(appimage) if !appimage.is_empty() => {
                builder.set_app_path(&appimage);
            }
            _ => {
                builder.set_app_path(&exe.display().to_string());
            }
        }
        #[cfg(windows)]
        builder.set_app_path(&exe.display().to_string());

        Ok(Self {
            entry: builder.build().context("preparing the login entry")?,
        })
    }
}

impl Autostart for LoginItem {
    fn is_enabled(&self) -> Result<bool> {
        self.entry.is_enabled().context("reading the login entry")
    }

    fn set_enabled(&self, enabled: bool) -> Result<()> {
        if enabled {
            self.entry.enable().context("adding the login entry")
        } else {
            self.entry.disable().context("removing the login entry")
        }
    }
}

/// The login entry release builds manage; debug builds have none, so a dev run never registers
/// itself to start at login.
pub fn for_build() -> Result<Option<Arc<dyn Autostart>>> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    Ok(Some(Arc::new(LoginItem::for_current_exe()?)))
}

/// The app's login entry, or `None` in debug builds; the settings screen reads it from here.
pub struct AutostartService(pub Option<Arc<dyn Autostart>>);

impl Global for AutostartService {}

impl AutostartService {
    pub fn get(cx: &App) -> Option<Arc<dyn Autostart>> {
        cx.try_global::<Self>()
            .and_then(|service| service.0.clone())
    }
}
