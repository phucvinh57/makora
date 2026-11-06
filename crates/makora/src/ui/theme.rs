//! The web UI's palette (`src/styles.css`, converted from OKLCH) as a gpui-component theme, light
//! and dark, following the system appearance unless the user picked one.

use std::rc::Rc;

use anyhow::{Context, Result};
use gpui_kit::component::{ActiveTheme, Theme, ThemeConfig, ThemeMode, ThemeSet};
use gpui_kit::{App, Hsla, Window};
use tracing::warn;

use crate::ui::prefs::{Prefs, ThemePreference};

const THEMES: &str = include_str!("../../assets/themes/makora.json");

pub fn init(cx: &mut App) {
    match themes() {
        Ok((light, dark)) => {
            let theme = Theme::global_mut(cx);
            theme.light_theme = Rc::new(light);
            theme.dark_theme = Rc::new(dark);
        }
        Err(err) => warn!(
            error = format!("{err:#}"),
            "the Makora theme failed to load; using the default one"
        ),
    }
    let mode = mode_for(Prefs::get(cx).theme, None, cx);
    Theme::change(mode, None, cx);
}

/// GPUI draws `.SystemUIFont`, its default family, as IBM Plex Sans on Linux, which most
/// desktops lack. Bundling it keeps every text run on that family, including those that render
/// outside gpui-component's root, instead of warning and falling back per distro.
#[cfg(target_os = "linux")]
pub fn register_ui_font(cx: &mut App) {
    const FONTS: [&[u8]; 4] = [
        include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf"),
        include_bytes!("../../assets/fonts/IBMPlexSans-Medium.ttf"),
        include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf"),
        include_bytes!("../../assets/fonts/IBMPlexSans-Bold.ttf"),
    ];
    let fonts = FONTS.into_iter().map(std::borrow::Cow::Borrowed).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        warn!(error = format!("{err:#}"), "the bundled UI font failed to load");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn register_ui_font(_cx: &mut App) {}

pub fn themes() -> Result<(ThemeConfig, ThemeConfig)> {
    let set: ThemeSet = serde_json::from_str(THEMES).context("parsing makora.json")?;
    let pick = |mode: ThemeMode| {
        set.themes
            .iter()
            .find(|theme| theme.mode == mode)
            .cloned()
            .with_context(|| format!("makora.json has no {} theme", mode.name()))
    };
    Ok((pick(ThemeMode::Light)?, pick(ThemeMode::Dark)?))
}

/// The mode a preference resolves to; the window's appearance is the reliable one on Linux.
pub fn mode_for(preference: ThemePreference, window: Option<&Window>, cx: &App) -> ThemeMode {
    match preference {
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
        ThemePreference::System => window
            .map_or_else(|| cx.window_appearance(), |window| window.appearance())
            .into(),
    }
}

pub fn apply(preference: ThemePreference, window: Option<&Window>, cx: &mut App) {
    let mode = mode_for(preference, window, cx);
    if cx.theme().mode != mode {
        Theme::change(mode, None, cx);
    }
}

/// Picks a theme and remembers it.
pub fn set_preference(preference: ThemePreference, window: Option<&Window>, cx: &mut App) {
    Prefs::update(cx, |prefs| prefs.theme = preference);
    apply(preference, window, cx);
}

/// Follows a change of the system appearance when no theme was picked.
pub fn follow_system(window: &Window, cx: &mut App) {
    let preference = Prefs::get(cx).theme;
    if preference == ThemePreference::System {
        apply(preference, Some(window), cx);
    }
}

pub fn is_dark(cx: &App) -> bool {
    cx.theme().mode.is_dark()
}

/// The raised surface cards sit on (`--card`).
pub fn card(cx: &App) -> Hsla {
    cx.theme().group_box
}

pub fn card_foreground(cx: &App) -> Hsla {
    cx.theme().group_box_foreground
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_modes_parse_with_the_indigo_primary() {
        let (light, dark) = themes().unwrap();
        assert_eq!(light.mode, ThemeMode::Light);
        assert_eq!(dark.mode, ThemeMode::Dark);
        assert_eq!(light.colors.primary.as_deref(), Some("#4f39f6"));
        assert_eq!(dark.colors.primary.as_deref(), Some("#615fff"));
    }

    #[test]
    fn every_color_is_valid_hex() {
        let json: serde_json::Value = serde_json::from_str(THEMES).unwrap();
        for theme in json["themes"].as_array().unwrap() {
            for (key, value) in theme["colors"].as_object().unwrap() {
                let hex = value.as_str().unwrap();
                assert!(
                    hex.starts_with('#')
                        && matches!(hex.len(), 7 | 9)
                        && hex[1..].chars().all(|c| c.is_ascii_hexdigit()),
                    "{key} = {hex}"
                );
            }
        }
    }
}
