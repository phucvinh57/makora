use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Exe,
    Domain,
    Url,
}

impl KeyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::Domain => "domain",
            Self::Url => "url",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exe" => Some(Self::Exe),
            "domain" => Some(Self::Domain),
            "url" => Some(Self::Url),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    Exact,
    Prefix,
    Substring,
    Regex,
}

impl MatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Prefix => "prefix",
            Self::Substring => "substring",
            Self::Regex => "regex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exact" => Some(Self::Exact),
            "prefix" => Some(Self::Prefix),
            "substring" => Some(Self::Substring),
            "regex" => Some(Self::Regex),
            _ => None,
        }
    }
}

pub fn rule_join_sql(rule_alias: &str, sessions_alias: &str) -> String {
    format!(
        "LEFT JOIN category_rules {rule_alias} ON {rule_alias}.id = (
            SELECT r2.id FROM category_rules r2
            WHERE
                (r2.key_kind = {sessions_alias}.category_key_kind AND (
                    (r2.match_kind = 'exact' AND lower(r2.pattern) = lower({sessions_alias}.category_key))
                    OR (r2.match_kind = 'prefix' AND instr(lower({sessions_alias}.category_key), lower(r2.pattern)) = 1)
                    OR (r2.match_kind = 'substring' AND instr(lower({sessions_alias}.category_key), lower(r2.pattern)) > 0)
                    OR (r2.match_kind = 'regex' AND {sessions_alias}.category_key REGEXP r2.pattern)
                ))
                OR (r2.key_kind = 'url' AND {sessions_alias}.url_key IS NOT NULL AND (
                    (r2.match_kind = 'exact' AND lower(r2.pattern) = lower({sessions_alias}.url_key))
                    OR (r2.match_kind = 'prefix' AND instr(lower({sessions_alias}.url_key), lower(r2.pattern)) = 1)
                    OR (r2.match_kind = 'substring' AND instr(lower({sessions_alias}.url_key), lower(r2.pattern)) > 0)
                    OR (r2.match_kind = 'regex' AND {sessions_alias}.url_key REGEXP r2.pattern)
                ))
            ORDER BY
                CASE r2.key_kind WHEN 'url' THEN 0 ELSE 1 END,
                CASE r2.match_kind
                    WHEN 'exact' THEN 0
                    WHEN 'prefix' THEN 1
                    WHEN 'substring' THEN 2
                    ELSE 3
                END,
                length(r2.pattern) DESC,
                r2.id
            LIMIT 1
        )"
    )
}

pub fn category_key(exe_path: &str, url: Option<&str>) -> (KeyKind, String) {
    match url.and_then(normalized_domain) {
        Some(domain) => (KeyKind::Domain, domain),
        None => (KeyKind::Exe, exe_file_name(exe_path).to_ascii_lowercase()),
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

pub fn filter_query_params(url: &str, allowed: &[String]) -> String {
    let Some(mut parsed) = parse_lenient(url) else {
        return url.split('#').next().unwrap_or(url).to_string();
    };
    parsed.set_fragment(None);
    let kept = parsed.query().map(|query| {
        let mut kept: Vec<&str> = query
            .split('&')
            .filter(|param| {
                let name = param.split('=').next().unwrap_or("");
                allowed.iter().any(|a| a == name)
            })
            .collect();
        kept.sort_unstable();
        kept.join("&")
    });
    parsed.set_query(kept.as_deref().filter(|q| !q.is_empty()));
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
    fn exe_keys_are_identical_whichever_platform_reads_the_database() {
        assert_eq!(exe_file_name(r"C:\Program Files\Code\Code.exe"), "Code.exe");
        assert_eq!(exe_file_name("/opt/google/chrome/chrome"), "chrome");
        assert_eq!(exe_file_name("chrome.exe"), "chrome.exe");
        assert_eq!(
            exe_file_name("wayland-app-id://org.gnome.Nautilus"),
            "org.gnome.Nautilus"
        );
    }

    #[test]
    fn browser_tab_row_keys_on_lowercased_domain() {
        let (kind, key) = category_key("chrome.exe", Some("https://GitHub.com/foo/bar"));
        assert_eq!(kind, KeyKind::Domain);
        assert_eq!(key, "github.com");
    }

    #[test]
    fn browser_tab_row_keys_strip_www_prefix_so_it_matches_bare_domain_rules() {
        let (kind, key) = category_key("chrome.exe", Some("https://www.Facebook.com/foo"));
        assert_eq!(kind, KeyKind::Domain);
        assert_eq!(key, "facebook.com");
    }

    #[test]
    fn domain_extraction_strips_port_and_userinfo() {
        assert_eq!(
            domain_of("https://user@example.com:8080/path"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn domain_extraction_handles_missing_scheme() {
        assert_eq!(
            domain_of("example.com/path"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn url_key_combines_domain_path_and_query() {
        assert_eq!(
            url_key("https://www.YouTube.com/watch?v=XYZ"),
            Some("youtube.com/watch?v=XYZ".to_string())
        );
    }

    #[test]
    fn url_key_trims_trailing_slash_and_omits_empty_query() {
        assert_eq!(
            url_key("https://docs.google.com/document/d/ABC/edit"),
            Some("docs.google.com/document/d/ABC/edit".to_string())
        );
        assert_eq!(
            url_key("https://example.com/"),
            Some("example.com".to_string())
        );
        assert_eq!(
            url_key("https://example.com"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn url_key_strips_port_and_userinfo_like_normalized_domain() {
        assert_eq!(
            url_key("https://user@example.com:8080/path"),
            Some("example.com/path".to_string())
        );
    }

    #[test]
    fn url_key_is_none_when_no_host_can_be_extracted() {
        assert_eq!(url_key(""), None);
    }

    #[test]
    fn filter_query_params_keeps_only_allowed_names() {
        let allowed = vec!["v".to_string()];
        assert_eq!(
            filter_query_params("https://youtube.com/watch?v=XYZ&list=abc", &allowed),
            "https://youtube.com/watch?v=XYZ"
        );
    }

    #[test]
    fn filter_query_params_sorts_kept_params_canonically() {
        let allowed = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            filter_query_params("https://example.com/p?b=2&a=1", &allowed),
            "https://example.com/p?a=1&b=2"
        );
    }

    #[test]
    fn filter_query_params_empty_allowlist_strips_entire_query() {
        assert_eq!(
            filter_query_params("https://example.com/p?a=1", &[]),
            "https://example.com/p"
        );
    }

    #[test]
    fn filter_query_params_drops_fragment() {
        assert_eq!(
            filter_query_params("https://example.com/p#section", &[]),
            "https://example.com/p"
        );
    }

    #[test]
    fn filter_query_params_passes_through_url_with_no_query() {
        assert_eq!(
            filter_query_params("https://example.com/p", &["v".to_string()]),
            "https://example.com/p"
        );
    }
}
