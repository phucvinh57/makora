use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
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

    /// Reads a `sources.kind` value; the column's `CHECK` allows nothing but these two.
    pub fn from_db(value: &str) -> Self {
        match value {
            "domain" => Self::Domain,
            _ => Self::Exe,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceKey {
    pub kind: KeyKind,
    pub key: String,
}

impl SourceKey {
    pub fn of(exe_path: &str, url: Option<&str>) -> Self {
        match url.and_then(normalized_domain) {
            Some(domain) => Self {
                kind: KeyKind::Domain,
                key: domain,
            },
            None => Self {
                kind: KeyKind::Exe,
                key: exe_file_name(exe_path).to_ascii_lowercase(),
            },
        }
    }

    pub fn display_name(&self, app_name: &str) -> String {
        match self.kind {
            KeyKind::Domain => self.key.clone(),
            KeyKind::Exe => app_name.to_string(),
        }
    }
}

pub fn normalized_domain(url: &str) -> Option<String> {
    let host = domain_of(url)?.to_ascii_lowercase();
    Some(
        host.strip_prefix("www.")
            .map(str::to_string)
            .unwrap_or(host),
    )
}

pub fn url_key(url: &str) -> Option<String> {
    let domain = normalized_domain(url)?;
    let parsed = parse_lenient(url);
    let path = parsed
        .as_ref()
        .map(|p| p.path())
        .unwrap_or("/")
        .trim_start_matches('/')
        .trim_end_matches('/');
    let query = parsed.as_ref().and_then(|p| p.query()).unwrap_or("");

    let mut key = domain;
    if !path.is_empty() {
        key.push('/');
        key.push_str(path);
    }
    if !query.is_empty() {
        key.push('?');
        key.push_str(query);
    }
    Some(key)
}

/// What `url_key` gives after the domain, so an activity's `path` column can be rebuilt without
/// storing the domain a second time. `""` when the URL has neither path nor query.
pub fn url_path(url: &str) -> String {
    let Some(key) = url_key(url) else {
        return String::new();
    };
    let Some(domain) = normalized_domain(url) else {
        return String::new();
    };
    key.strip_prefix(&domain).unwrap_or("").to_string()
}

pub fn source_url(key: &str, path: &str) -> String {
    format!("https://{key}{path}")
}

/// `domain/path` with the scheme, `www.`, query and fragment dropped — what a page is about
/// without the tracking noise.
pub fn url_location(url: &str) -> Option<String> {
    let domain = normalized_domain(url)?;
    let parsed = parse_lenient(url);
    let path = parsed
        .as_ref()
        .map(|p| p.path())
        .unwrap_or("/")
        .trim_end_matches('/');
    Some(format!("{domain}{path}"))
}

pub fn strip_query_and_fragment(url: &str) -> String {
    let Some(mut parsed) = parse_lenient(url) else {
        return url.split('#').next().unwrap_or(url).to_string();
    };
    parsed.set_fragment(None);
    parsed.set_query(None);
    parsed.to_string()
}

pub(crate) fn exe_file_name(exe_path: &str) -> &str {
    exe_path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(exe_path)
}

fn parse_lenient(input: &str) -> Option<Url> {
    Url::parse(input)
        .or_else(|_| Url::parse(&format!("http://{input}")))
        .ok()
}

pub(crate) fn domain_of(url: &str) -> Option<String> {
    let host = parse_lenient(url)?.host_str()?.to_string();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}
