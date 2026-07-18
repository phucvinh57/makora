use std::path::Path;

use url::Url;

/// Which kind of pattern a session's [`category_key`] was derived from, or
/// (for [`Url`](KeyKind::Url)) a rule-only target matched against a
/// session's `url_key` instead. Stored on `category_key_kind` alongside the
/// key so it can be matched against rules/overrides of the same kind without
/// re-deriving it (and without the different kinds of pattern ever being
/// confused for one another).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Exe,
    Domain,
    /// Rule-only: [`category_key`] never produces this kind. Matches a
    /// session's [`url_key`] (host + path + whitelisted query) instead of
    /// its `category_key`, so two pages on the same domain — e.g. two
    /// Google Docs documents — can resolve to different categories.
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

/// How a [`crate::core::store::CategoryRule`]'s `pattern` is matched against
/// a session's key (`category_key`, or `url_key` for a [`KeyKind::Url`]
/// rule). Also doubles as this system's specificity ranking, most to least
/// specific: [`Exact`](MatchKind::Exact) > [`Prefix`](MatchKind::Prefix) >
/// [`Substring`](MatchKind::Substring) > [`Regex`](MatchKind::Regex) — see
/// [`rule_join_sql`]. A `Substring` rule of `"steam"` also catches
/// `steamwebhelper.exe`; a `Prefix` rule of `"docs.google.com/document/d/ABC"`
/// catches that one document (and its sub-paths) but not other documents on
/// the same domain; a `Regex` rule can express things like "any
/// `*.corp.example.com` subdomain" that none of the others can.
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

/// A `LEFT JOIN ... ON ...` clause resolving `sessions_alias` against
/// `category_rules`, honoring each rule's `match_kind` (case-insensitive
/// exact equality, prefix, substring — all via `lower`/`instr`, same
/// convention as `ignore_rules` — or a case-insensitive regex via the
/// `regexp` SQLite function registered in `store::sqlite::open_connection`,
/// backing the `REGEXP` operator). A [`KeyKind::Url`] rule matches
/// `sessions_alias.url_key`; an `Exe`/`Domain` rule matches
/// `(category_key_kind, category_key)` as before.
///
/// When more than one rule matches the same session (e.g. a `url` rule for
/// one Google Docs document and a broader `domain` rule for
/// `docs.google.com` both apply), the subquery picks a single deterministic
/// winner by **specificity** — there is no user-managed ordering (categories
/// have no priority) — so this join can never fan a session row out into
/// duplicates the way a plain multi-match `ON` condition would:
///
/// 1. Target: a `url` rule beats an `exe`/`domain` rule.
/// 2. Match kind, most to least specific: `exact` > `prefix` > `substring` >
///    `regex` (see [`MatchKind`]'s doc comment).
/// 3. Longer pattern wins (a rough proxy for "more specific").
/// 4. Lowest rule id, as a final tiebreak — only matters when two rules in
///    the *same* category both match, where the winner is immaterial since
///    they resolve to the same category either way.
///
/// `rule_alias` names the joined `category_rules` row; `sessions_alias`
/// names the table/subquery being joined against (must expose
/// `category_key_kind`/`category_key`/`url_key` columns).
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

/// The stable lookup key used to categorize a session: the URL's host for a
/// browser-tab row, or the executable's file name for a plain app-focus row.
/// Computed once when the session is written so that changing a rule or
/// override later re-categorizes historical sessions without needing to
/// re-parse the URL or path at query time.
pub fn category_key(exe_path: &str, url: Option<&str>) -> (KeyKind, String) {
    match url.and_then(normalized_domain) {
        Some(domain) => (KeyKind::Domain, domain),
        None => (KeyKind::Exe, exe_file_name(exe_path).to_ascii_lowercase()),
    }
}

/// The lowercased, `www.`-stripped host of `url` — the same normalization
/// [`category_key`] applies to a browser-tab row's key, exposed separately so
/// other lookups against a URL (e.g. [`crate::core::store::IgnoreStore`]) key
/// on it identically instead of re-deriving their own notion of "domain".
pub fn normalized_domain(url: &str) -> Option<String> {
    let host = domain_of(url)?.to_ascii_lowercase();
    Some(host.strip_prefix("www.").map(str::to_string).unwrap_or(host))
}

/// The normalized match target for a [`KeyKind::Url`] rule: lowercased,
/// `www.`-stripped host (same as [`normalized_domain`]) plus the path
/// (trailing `/` trimmed) plus `?query` when the query is non-empty. `None`
/// for anything [`domain_of`] can't extract a host from.
///
/// `url` is expected to already be query-filtered (see
/// [`filter_query_params`]) — this function doesn't filter it itself, so
/// that the same normalized form always reflects whatever was actually
/// stored on the session.
///
/// Examples: `https://www.YouTube.com/watch?v=XYZ` → `youtube.com/watch?v=XYZ`;
/// `https://docs.google.com/document/d/ABC/edit` →
/// `docs.google.com/document/d/ABC/edit`; `https://example.com/` → `example.com`.
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

/// Filters `url`'s query string down to only the params in `allowed`
/// (matched by the raw param name, i.e. the text before `=`, case-sensitive),
/// re-emitted sorted by name for a canonical form. The fragment is always
/// dropped. An empty `allowed` list drops the whole query string.
///
/// This is the privacy boundary for query params (see
/// `store::QueryWhitelistStore`) — everything not explicitly whitelisted for
/// the URL's domain is stripped before the URL is ever stored.
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

fn exe_file_name(exe_path: &str) -> &str {
    Path::new(exe_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(exe_path)
}

/// Parses `input` as an absolute URL via the `url` crate, retrying with an
/// assumed `http://` scheme when it has none — covers a bare domain
/// (`example.com`) or a rule pattern pasted without a scheme
/// (`docs.google.com/document/d/ABC`), neither of which `Url::parse` accepts
/// on its own.
fn parse_lenient(input: &str) -> Option<Url> {
    Url::parse(input)
        .or_else(|_| Url::parse(&format!("http://{input}")))
        .ok()
}

/// Extracts the host from a URL — see `TabEvent::url`. Falls back gracefully
/// on anything else since this only ever feeds a lookup key, never a
/// security decision.
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
        assert_eq!(domain_of("example.com/path"), Some("example.com".to_string()));
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
        assert_eq!(url_key("https://example.com/"), Some("example.com".to_string()));
        assert_eq!(url_key("https://example.com"), Some("example.com".to_string()));
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
    fn filter_query_params_keeps_only_whitelisted_names() {
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
