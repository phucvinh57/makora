use std::path::Path;

use serde_json::json;
use tracing::{info, warn};

use super::bridge_wire::HOST_NAME;

/// Chrome Web Store listing (`vars.CWS_EXTENSION_ID` in the release workflow).
pub const STORE_EXTENSION_ID: &str = "hboianmkjncgkjnapijgaplfdigjmklf";
/// Unpacked dev builds: pinned by the public `key` that `extensions/browser/build.mjs` injects.
pub const DEV_EXTENSION_ID: &str = "bapccffcoeagcmgkjlmdfhinnokhmnab";
/// `browser_specific_settings.gecko.id` in `extensions/browser/manifest.json`.
pub const FIREFOX_EXTENSION_ID: &str = "tab-reporter@makora.dev";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Chromium,
    Firefox,
}

pub fn manifest(family: Family, bridge: &Path) -> String {
    let mut manifest = json!({
        "name": HOST_NAME,
        "description": "Makora browser tab bridge",
        "path": bridge.to_string_lossy(),
        "type": "stdio",
    });
    match family {
        Family::Chromium => {
            manifest["allowed_origins"] = json!([STORE_EXTENSION_ID, DEV_EXTENSION_ID]
                .map(|id| format!("chrome-extension://{id}/")));
        }
        Family::Firefox => manifest["allowed_extensions"] = json!([FIREFOX_EXTENSION_ID]),
    }
    format!("{manifest:#}\n")
}

/// Points each installed browser at the bridge next to this executable. Rewritten on every
/// start, so a moved install heals itself.
pub fn register(data_dir: &Path) {
    let bridge = match std::env::current_exe() {
        Ok(exe) => exe.with_file_name(format!("makora-bridge{}", std::env::consts::EXE_SUFFIX)),
        Err(err) => {
            warn!(error = %err, "browser tab tracking disabled: cannot locate the app executable");
            return;
        }
    };
    if !bridge.is_file() {
        warn!(path = %bridge.display(), "browser tab tracking disabled: the browser bridge is missing");
        return;
    }
    platform::register(&bridge, data_dir);
}

#[cfg(unix)]
mod platform {
    use std::path::PathBuf;

    use super::*;

    pub struct Target {
        pub browser_dir: PathBuf,
        pub hosts_dir: PathBuf,
        pub family: Family,
    }

    pub fn register(bridge: &Path, _data_dir: &Path) {
        let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
            warn!("browser tab tracking disabled: HOME is not set");
            return;
        };
        let config = std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty());
        for target in targets(Path::new(&home), config.as_deref().map(Path::new)) {
            if !target.browser_dir.is_dir() {
                continue;
            }
            let path = target.hosts_dir.join(format!("{HOST_NAME}.json"));
            match write_if_changed(&path, &manifest(target.family, bridge)) {
                Ok(true) => info!(path = %path.display(), "registered the browser bridge"),
                Ok(false) => {}
                Err(err) => warn!(path = %path.display(), error = %err, "failed to register the browser bridge"),
            }
        }
    }

    fn write_if_changed(path: &Path, content: &str) -> std::io::Result<bool> {
        if std::fs::read_to_string(path).is_ok_and(|current| current == content) {
            return Ok(false);
        }
        std::fs::create_dir_all(path.parent().expect("manifest path has a parent"))?;
        std::fs::write(path, content)?;
        Ok(true)
    }

    #[cfg(target_os = "macos")]
    pub fn targets(home: &Path, _config: Option<&Path>) -> Vec<Target> {
        let support = home.join("Library/Application Support");
        let chromium = |dir: &str| Target {
            browser_dir: support.join(dir),
            hosts_dir: support.join(dir).join("NativeMessagingHosts"),
            family: Family::Chromium,
        };
        vec![
            chromium("Google/Chrome"),
            chromium("Chromium"),
            chromium("Microsoft Edge"),
            chromium("BraveSoftware/Brave-Browser"),
            Target {
                browser_dir: support.join("Firefox"),
                hosts_dir: support.join("Mozilla/NativeMessagingHosts"),
                family: Family::Firefox,
            },
        ]
    }

    #[cfg(not(target_os = "macos"))]
    pub fn targets(home: &Path, config: Option<&Path>) -> Vec<Target> {
        let config = config.map(Path::to_path_buf).unwrap_or_else(|| home.join(".config"));
        let chromium = |dir: &str| Target {
            browser_dir: config.join(dir),
            hosts_dir: config.join(dir).join("NativeMessagingHosts"),
            family: Family::Chromium,
        };
        vec![
            chromium("google-chrome"),
            chromium("chromium"),
            chromium("microsoft-edge"),
            chromium("BraveSoftware/Brave-Browser"),
            Target {
                browser_dir: home.join(".mozilla"),
                hosts_dir: home.join(".mozilla/native-messaging-hosts"),
                family: Family::Firefox,
            },
        ]
    }
}

#[cfg(windows)]
mod platform {
    use super::*;

    const REGISTRY_ROOTS: [(&str, Family); 5] = [
        (r"Software\Google\Chrome", Family::Chromium),
        (r"Software\Chromium", Family::Chromium),
        (r"Software\Microsoft\Edge", Family::Chromium),
        (r"Software\BraveSoftware\Brave-Browser", Family::Chromium),
        (r"Software\Mozilla", Family::Firefox),
    ];

    pub fn register(bridge: &Path, data_dir: &Path) {
        let dir = data_dir.join("native-messaging");
        let chromium = dir.join("chromium.json");
        let firefox = dir.join("firefox.json");
        let written = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&chromium, manifest(Family::Chromium, bridge)))
            .and_then(|()| std::fs::write(&firefox, manifest(Family::Firefox, bridge)));
        if let Err(err) = written {
            warn!(path = %dir.display(), error = %err, "failed to register the browser bridge");
            return;
        }
        for (root, family) in REGISTRY_ROOTS {
            let key = format!(r"{root}\NativeMessagingHosts\{HOST_NAME}");
            let manifest_path = match family {
                Family::Chromium => &chromium,
                Family::Firefox => &firefox,
            };
            if let Err(err) = set_default_value(&key, manifest_path) {
                warn!(key, error = %err, "failed to register the browser bridge");
            }
        }
        info!(path = %dir.display(), "registered the browser bridge");
    }

    fn set_default_value(key: &str, value: &Path) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
            REG_OPTION_NON_VOLATILE, REG_SZ,
        };

        let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
        let key = wide(key.as_ref());
        let value = wide(value.as_os_str());
        unsafe {
            let mut handle: HKEY = std::ptr::null_mut();
            let status = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut handle,
                std::ptr::null_mut(),
            );
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32));
            }
            let status = RegSetValueExW(
                handle,
                std::ptr::null(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            );
            RegCloseKey(handle);
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn parsed(family: Family) -> Value {
        serde_json::from_str(&manifest(family, Path::new("/opt/makora/makora-bridge"))).unwrap()
    }

    #[test]
    fn chromium_manifest_allows_the_store_and_dev_extensions() {
        let manifest = parsed(Family::Chromium);

        assert_eq!(manifest["name"], "dev.phucvinh57.makora");
        assert_eq!(manifest["path"], "/opt/makora/makora-bridge");
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(
            manifest["allowed_origins"],
            json!([
                "chrome-extension://hboianmkjncgkjnapijgaplfdigjmklf/",
                "chrome-extension://bapccffcoeagcmgkjlmdfhinnokhmnab/",
            ])
        );
        assert!(manifest.get("allowed_extensions").is_none());
    }

    #[test]
    fn firefox_manifest_allows_the_gecko_id() {
        let manifest = parsed(Family::Firefox);

        assert_eq!(manifest["name"], "dev.phucvinh57.makora");
        assert_eq!(manifest["path"], "/opt/makora/makora-bridge");
        assert_eq!(manifest["type"], "stdio");
        assert_eq!(manifest["allowed_extensions"], json!(["tab-reporter@makora.dev"]));
        assert!(manifest.get("allowed_origins").is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_targets_follow_xdg_config_home() {
        use std::path::PathBuf;

        let targets = platform::targets(Path::new("/home/me"), Some(Path::new("/xdg")));
        let dirs: Vec<_> = targets.iter().map(|t| (t.hosts_dir.clone(), t.family)).collect();

        assert_eq!(
            dirs,
            vec![
                (PathBuf::from("/xdg/google-chrome/NativeMessagingHosts"), Family::Chromium),
                (PathBuf::from("/xdg/chromium/NativeMessagingHosts"), Family::Chromium),
                (PathBuf::from("/xdg/microsoft-edge/NativeMessagingHosts"), Family::Chromium),
                (PathBuf::from("/xdg/BraveSoftware/Brave-Browser/NativeMessagingHosts"), Family::Chromium),
                (PathBuf::from("/home/me/.mozilla/native-messaging-hosts"), Family::Firefox),
            ]
        );
    }
}
