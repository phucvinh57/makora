use std::path::Path;

use crate::core::model::AppInfo;
use crate::core::platform::linux::provider::WindowObservation;

const APP_ID_SCHEME: &str = "wayland-app-id://";

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

fn normalize_exe_path(raw: &str, flatpak_id: impl FnOnce() -> Option<String>) -> (String, String) {
    let base = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(raw)
        .to_owned();

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

fn is_snap_revision(segment: &str) -> bool {
    let digits = segment.strip_prefix('x').unwrap_or(segment);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn synthesize_from_app_id(app_id: &str, wm_class: &str) -> (String, String) {
    let id = [app_id, wm_class]
        .into_iter()
        .map(str::trim)
        .find(|candidate| !candidate.is_empty())
        .unwrap_or("unknown");
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    let app_name = id.rsplit('.').next().unwrap_or(id).to_ascii_lowercase();
    (format!("{APP_ID_SCHEME}{id}"), app_name)
}
