//! The names the OS knows the person by: their login, home folder and full name. Read once.

use std::sync::OnceLock;

/// Names shorter than this match too much ordinary text to be worth hiding.
const MIN_NAME_CHARS: usize = 3;
/// Parts of a full name shorter than this are left to the full name.
const MIN_NAME_PART_CHARS: usize = 4;

pub fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(collect)
}

fn collect() -> Vec<String> {
    let mut candidates: Vec<String> = ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .collect();
    if let Some(home) = dirs::home_dir().and_then(|home| home.file_name().map(|name| name.to_string_lossy().into_owned())) {
        candidates.push(home);
    }
    if let Some(full) = full_name() {
        candidates.extend(
            full.split_whitespace()
                .filter(|part| part.chars().count() >= MIN_NAME_PART_CHARS)
                .map(str::to_string),
        );
        candidates.push(full);
    }
    let mut names: Vec<String> = Vec::new();
    for name in candidates {
        let name = name.trim().to_string();
        if name.chars().count() >= MIN_NAME_CHARS && !names.iter().any(|other| other.to_lowercase() == name.to_lowercase()) {
            names.push(name);
        }
    }
    names
}

#[cfg(target_os = "linux")]
fn full_name() -> Option<String> {
    let user = std::env::var("USER").ok()?;
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? != user {
            return None;
        }
        let gecos = fields.nth(3)?;
        let name = gecos.split(',').next()?.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

#[cfg(target_os = "macos")]
fn full_name() -> Option<String> {
    let output = std::process::Command::new("id").arg("-F").output().ok()?;
    let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !name.is_empty()).then_some(name)
}

#[cfg(windows)]
fn full_name() -> Option<String> {
    use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};

    let mut size: u32 = 256;
    let mut buffer = vec![0u16; size as usize];
    // SAFETY: `buffer` holds `size` UTF-16 units, and the call writes at most that many.
    let _ = unsafe { GetUserNameExW(NameDisplay, buffer.as_mut_ptr(), &mut size) };
    let len = buffer.iter().position(|&unit| unit == 0).unwrap_or(0);
    let name = String::from_utf16_lossy(&buffer[..len]).trim().to_string();
    (!name.is_empty()).then_some(name)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn full_name() -> Option<String> {
    None
}
