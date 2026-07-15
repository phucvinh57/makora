use std::path::Path;

/// Which kind of pattern a session's [`category_key`] was derived from.
/// Stored alongside the key so it can be matched against rules/overrides of
/// the same kind without re-deriving it (and without the two kinds of
/// pattern ever being confused for one another).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Exe,
    Domain,
}

impl KeyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::Domain => "domain",
        }
    }
}

/// The stable lookup key used to categorize a session: the URL's host for a
/// browser-tab row, or the executable's file name for a plain app-focus row.
/// Computed once when the session is written so that changing a rule or
/// override later re-categorizes historical sessions without needing to
/// re-parse the URL or path at query time.
pub fn category_key(exe_path: &str, url: Option<&str>) -> (KeyKind, String) {
    match url.and_then(domain_of) {
        Some(domain) => (KeyKind::Domain, domain.to_ascii_lowercase()),
        None => (KeyKind::Exe, exe_file_name(exe_path).to_ascii_lowercase()),
    }
}

fn exe_file_name(exe_path: &str) -> &str {
    Path::new(exe_path).file_name().and_then(|s| s.to_str()).unwrap_or(exe_path)
}

/// Extracts the host from a URL that's already been normalized to
/// origin + path (no query string or fragment) — see `TabEvent::url`. Falls
/// back gracefully on anything else since this only ever feeds a lookup key,
/// never a security decision.
fn domain_of(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, rest)| rest);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_focus_row_keys_on_lowercased_exe_file_name() {
        let (kind, key) = category_key(r"C:\Program Files\Code\Code.exe", None);
        assert_eq!(kind, KeyKind::Exe);
        assert_eq!(key, "code.exe");
    }

    #[test]
    fn browser_tab_row_keys_on_lowercased_domain() {
        let (kind, key) = category_key("chrome.exe", Some("https://GitHub.com/foo/bar"));
        assert_eq!(kind, KeyKind::Domain);
        assert_eq!(key, "github.com");
    }

    #[test]
    fn domain_extraction_strips_port_and_userinfo() {
        assert_eq!(domain_of("https://user@example.com:8080/path"), Some("example.com"));
    }

    #[test]
    fn domain_extraction_handles_missing_scheme() {
        assert_eq!(domain_of("example.com/path"), Some("example.com"));
    }
}
