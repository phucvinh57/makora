//! Turning a [`WindowObservation`] into the [`AppInfo`] the tracker keys
//! sessions on.
//!
//! `exe_path` is the session identity (see [`AppInfo`]'s docs), so this
//! module's whole job is producing a path that is *stable across restarts
//! and package updates* — otherwise history silently splits in two the day
//! a snap refreshes. `app_name` is set explicitly rather than derived from
//! the path by the caller, because the app-id fallback paths below are not
//! filesystem paths and `Path::file_stem` mangles them.

use std::path::Path;

use crate::core::model::AppInfo;
use crate::core::platform::linux::provider::WindowObservation;

/// Scheme used for the `exe_path` of a window whose owning process is
/// unknown — the `wlr-foreign-toplevel-management-v1` protocol carries an
/// `app_id` but no pid, so there is nothing under `/proc` to read. Not a
/// real path, and deliberately not shaped like one: anything comparing
/// `exe_path`s (session identity, ignore rules, `exe`-kind category rules)
/// treats it as an opaque string, and the scheme keeps it from ever
/// colliding with a genuine executable.
const APP_ID_SCHEME: &str = "wayland-app-id://";

/// Resolves the app behind a focused window.
///
/// Never fails: a window with neither a readable `/proc` entry nor an app
/// id still yields a usable (if coarse) identity, because dropping the
/// event would silently stop tracking instead of tracking imprecisely.
pub fn resolve_app_info(observation: &WindowObservation) -> AppInfo {
    if let Some(pid) = observation.pid {
        if let Some(raw) = read_exe_link(pid) {
            let (exe_path, app_name) = normalize_exe_path(&raw, || flatpak_app_id(pid));
            return AppInfo {
                process_id: pid,
                exe_path,
                app_name,
            };
        }
    }

    let (exe_path, app_name) = synthesize_from_app_id(&observation.app_id, &observation.wm_class);
    AppInfo {
        process_id: observation.pid.unwrap_or(0),
        exe_path,
        app_name,
    }
}

fn read_exe_link(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()?
        .to_str()
        .map(str::to_owned)
}

/// Reads the flatpak application id out of a sandboxed process's
/// `.flatpak-info`, which the runtime mounts at the root of every flatpak's
/// namespace. Visible from outside via `/proc/<pid>/root`.
fn flatpak_app_id(pid: u32) -> Option<String> {
    let info = std::fs::read_to_string(format!("/proc/{pid}/root/.flatpak-info")).ok()?;
    parse_flatpak_app_id(&info)
}

fn parse_flatpak_app_id(info: &str) -> Option<String> {
    let mut in_application = false;
    for line in info.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_application = line == "[Application]";
            continue;
        }
        if in_application {
            if let Some(name) = line.strip_prefix("name=") {
                let name = name.trim();
                if !name.is_empty() {
                    return Some(name.to_owned());
                }
            }
        }
    }
    None
}

/// Maps a raw `/proc/<pid>/exe` target to a stable `(exe_path, app_name)`.
///
/// `flatpak_id` is a closure rather than a value because resolving it means
/// another filesystem read, and the overwhelming majority of windows are not
/// flatpaks.
fn normalize_exe_path(raw: &str, flatpak_id: impl FnOnce() -> Option<String>) -> (String, String) {
    let base = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(raw)
        .to_owned();

    // A flatpak's `/proc/<pid>/exe` resolves inside its own mount namespace,
    // so it's almost always `/app/bin/<something>` — identical across
    // unrelated flatpaks, and meaningless on this side of the sandbox.
    // The application id is the stable identity instead.
    if raw.starts_with("/app/") {
        if let Some(id) = flatpak_id() {
            return (format!("/flatpak/{id}/{base}"), base);
        }
    }

    let stem = Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&base)
        .to_owned();
    (normalize_snap_revision(raw), stem)
}

/// Replaces a snap's revision with `current`.
///
/// Snaps are installed at `/snap/<name>/<revision>/…` and the revision
/// changes on *every* update, so without this a Firefox refresh would look
/// like a brand-new application and split its history. `/snap/<name>/current`
/// is a real symlink snapd maintains, so the rewritten path stays valid.
fn normalize_snap_revision(raw: &str) -> String {
    let Some(rest) = raw.strip_prefix("/snap/") else {
        return raw.to_owned();
    };
    let mut parts = rest.splitn(3, '/');
    let (Some(name), Some(revision), Some(tail)) = (parts.next(), parts.next(), parts.next())
    else {
        return raw.to_owned();
    };
    if !is_snap_revision(revision) {
        return raw.to_owned();
    }
    format!("/snap/{name}/current/{tail}")
}

/// Snap revisions are a bare number for store installs (`1234`) and `x`
/// followed by a number for locally installed snaps (`x1`).
fn is_snap_revision(segment: &str) -> bool {
    let digits = segment.strip_prefix('x').unwrap_or(segment);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Fallback identity for a window with no readable process: the app id.
///
/// Note `app_name` is taken as the last dot-separated segment rather than
/// via `Path::file_stem`, which would turn `org.mozilla.firefox` into
/// `org.mozilla` — and `app_name` is what
/// [`BrowserKind::from_app_name`](crate::core::model::BrowserKind::from_app_name)
/// matches on, so getting it wrong would quietly disable per-site tracking.
fn synthesize_from_app_id(app_id: &str, wm_class: &str) -> (String, String) {
    let id = [app_id, wm_class]
        .into_iter()
        .map(str::trim)
        .find(|candidate| !candidate.is_empty())
        .unwrap_or("unknown");
    // `Shell.WindowTracker` reports desktop-file ids; Wayland `app_id`s
    // usually don't carry the suffix, so strip it when present.
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    let app_name = id.rsplit('.').next().unwrap_or(id).to_ascii_lowercase();
    (format!("{APP_ID_SCHEME}{id}"), app_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_flatpak() -> Option<String> {
        None
    }

    #[test]
    fn ordinary_exe_keeps_its_path_and_uses_the_file_stem() {
        let (exe_path, app_name) = normalize_exe_path("/opt/google/chrome/chrome", no_flatpak);
        assert_eq!(exe_path, "/opt/google/chrome/chrome");
        // Already matches `BrowserKind::from_app_name`, so per-site
        // tracking works on Linux without a Linux-specific alias.
        assert_eq!(app_name, "chrome");
    }

    #[test]
    fn snap_revision_is_replaced_so_updates_dont_split_history() {
        let before =
            normalize_snap_revision("/snap/firefox/6836/usr/lib/firefox/firefox");
        let after = normalize_snap_revision("/snap/firefox/7001/usr/lib/firefox/firefox");
        assert_eq!(before, "/snap/firefox/current/usr/lib/firefox/firefox");
        assert_eq!(
            before, after,
            "a snap refresh must not change the session identity"
        );
    }

    #[test]
    fn locally_installed_snap_revisions_are_recognized() {
        assert_eq!(
            normalize_snap_revision("/snap/hello/x1/bin/hello"),
            "/snap/hello/current/bin/hello"
        );
    }

    #[test]
    fn snap_paths_that_are_already_current_are_left_alone() {
        // `current` is not a revision, so the path passes through
        // unchanged rather than being rewritten to `/snap/firefox/current`
        // with a segment eaten.
        let path = "/snap/firefox/current/usr/lib/firefox/firefox";
        assert_eq!(normalize_snap_revision(path), path);
    }

    #[test]
    fn non_snap_paths_starting_with_snap_are_left_alone() {
        assert_eq!(normalize_snap_revision("/snapshot/bin/tool"), "/snapshot/bin/tool");
        assert_eq!(normalize_snap_revision("/snap/firefox"), "/snap/firefox");
    }

    #[test]
    fn flatpak_exe_is_replaced_by_the_application_id() {
        let (exe_path, app_name) =
            normalize_exe_path("/app/bin/firefox", || Some("org.mozilla.firefox".into()));
        // `/app/bin/firefox` is identical inside every flatpak sandbox, so
        // the id is what makes it unique.
        assert_eq!(exe_path, "/flatpak/org.mozilla.firefox/firefox");
        assert_eq!(app_name, "firefox");
    }

    #[test]
    fn flatpak_exe_without_readable_info_falls_back_to_the_raw_path() {
        let (exe_path, app_name) = normalize_exe_path("/app/bin/firefox", no_flatpak);
        assert_eq!(exe_path, "/app/bin/firefox");
        assert_eq!(app_name, "firefox");
    }

    #[test]
    fn flatpak_info_name_is_read_from_the_application_section() {
        let info = "\
[Context]
name=not-this-one

[Application]
name=org.gnome.Boxes
runtime=runtime/org.gnome.Platform/x86_64/47
";
        assert_eq!(parse_flatpak_app_id(info).as_deref(), Some("org.gnome.Boxes"));
    }

    #[test]
    fn flatpak_info_without_an_application_section_yields_nothing() {
        assert_eq!(parse_flatpak_app_id("[Context]\nname=nope\n"), None);
    }

    #[test]
    fn app_id_fallback_does_not_use_file_stem_semantics() {
        let (exe_path, app_name) = synthesize_from_app_id("org.mozilla.firefox", "");
        assert_eq!(exe_path, "wayland-app-id://org.mozilla.firefox");
        // `Path::file_stem` would answer "org.mozilla" here, which would
        // stop `BrowserKind::from_app_name` from ever matching.
        assert_eq!(app_name, "firefox");
    }

    #[test]
    fn desktop_suffix_is_stripped_from_app_ids() {
        let (exe_path, app_name) = synthesize_from_app_id("org.gnome.Nautilus.desktop", "");
        assert_eq!(exe_path, "wayland-app-id://org.gnome.Nautilus");
        assert_eq!(app_name, "nautilus");
    }

    #[test]
    fn wm_class_is_used_when_the_app_id_is_missing() {
        let (exe_path, app_name) = synthesize_from_app_id("", "Alacritty");
        assert_eq!(exe_path, "wayland-app-id://Alacritty");
        assert_eq!(app_name, "alacritty");
    }

    #[test]
    fn a_window_with_no_identity_at_all_still_resolves() {
        let info = resolve_app_info(&WindowObservation::default());
        assert_eq!(info.exe_path, "wayland-app-id://unknown");
        assert_eq!(info.app_name, "unknown");
        assert_eq!(info.process_id, 0);
    }

    #[test]
    fn a_pid_that_no_longer_exists_falls_back_to_the_app_id() {
        // pid 0 is never a real process, so the `/proc` read is guaranteed
        // to fail and exercise the fallback.
        let info = resolve_app_info(&WindowObservation {
            pid: Some(0),
            app_id: "org.gnome.TextEditor.desktop".into(),
            wm_class: "org.gnome.TextEditor".into(),
            title: "Untitled".into(),
        });
        assert_eq!(info.exe_path, "wayland-app-id://org.gnome.TextEditor");
        assert_eq!(info.app_name, "texteditor");
    }

    #[test]
    fn the_current_process_resolves_to_a_real_executable() {
        let info = resolve_app_info(&WindowObservation {
            pid: Some(std::process::id()),
            ..Default::default()
        });
        assert!(
            info.exe_path.starts_with('/'),
            "expected a real path, got {}",
            info.exe_path
        );
        assert!(!info.app_name.is_empty());
    }
}
