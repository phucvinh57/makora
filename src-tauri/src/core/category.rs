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

/// How a [`crate::core::store::CategoryRule`]'s `pattern` is matched against
/// a session's `category_key`. Unlike a key-wide override (always an exact
/// key, since it's derived from one session's own key), a rule can also
/// match a case-insensitive substring or a case-insensitive regex — e.g. a
/// `Substring` rule of `"steam"` also catches `steamwebhelper.exe`, and a
/// `Regex` rule can express things like "any `*.corp.example.com` subdomain"
/// that neither `Exact` nor `Substring` can.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    Exact,
    Substring,
    Regex,
}

impl MatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Substring => "substring",
            Self::Regex => "regex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exact" => Some(Self::Exact),
            "substring" => Some(Self::Substring),
            "regex" => Some(Self::Regex),
            _ => None,
        }
    }
}

/// A `LEFT JOIN ... ON ...` clause resolving `sessions_alias`'s
/// `(category_key_kind, category_key)` against `category_rules`, honoring
/// each rule's `match_kind`: exact equality, a case-insensitive substring
/// (via `instr`/`lower`, same convention as `ignore_rules`), or a
/// case-insensitive regex (via the `regexp` SQLite function registered in
/// `store::sqlite::open_connection`, backing the `REGEXP` operator).
///
/// When more than one rule matches the same key (e.g. an `Exact` rule in one
/// category and a broader `Substring` rule in another both apply), the
/// subquery picks a single deterministic winner — the rule whose category
/// has the lowest `priority`, then lowest rule id as a final tiebreak — so
/// this join can never fan a session row out into duplicates the way a
/// plain multi-match `ON` condition would. Category `priority` is manually
/// ordered by the user from the Categories page (see
/// `CategoryStore::reorder_categories`) rather than derived from
/// `match_kind`, so a `Substring` rule can be made to win over a more
/// specific `Exact` rule in a lower-priority category. The rule-id tiebreak
/// only matters when two rules in the *same* category both match — which
/// specific rule "wins" is immaterial there, since they resolve to the same
/// category either way.
///
/// `rule_alias` names the joined `category_rules` row; `sessions_alias`
/// names the table/subquery being joined against (must expose
/// `category_key_kind`/`category_key` columns).
pub fn rule_join_sql(rule_alias: &str, sessions_alias: &str) -> String {
    format!(
        "LEFT JOIN category_rules {rule_alias} ON {rule_alias}.id = (
            SELECT r2.id FROM category_rules r2
            JOIN categories c2 ON c2.id = r2.category_id
            WHERE r2.key_kind = {sessions_alias}.category_key_kind AND (
                (r2.match_kind = 'exact' AND r2.pattern = {sessions_alias}.category_key)
                OR (r2.match_kind = 'substring' AND instr(lower({sessions_alias}.category_key), lower(r2.pattern)) > 0)
                OR (r2.match_kind = 'regex' AND {sessions_alias}.category_key REGEXP r2.pattern)
            )
            ORDER BY c2.priority, r2.id
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
    domain_of(url).map(|domain| {
        let domain = domain.to_ascii_lowercase();
        domain.strip_prefix("www.").unwrap_or(&domain).to_string()
    })
}

fn exe_file_name(exe_path: &str) -> &str {
    Path::new(exe_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(exe_path)
}

/// Extracts the host from a URL that's already been normalized to
/// origin + path (no query string or fragment) — see `TabEvent::url`. Falls
/// back gracefully on anything else since this only ever feeds a lookup key,
/// never a security decision.
pub(crate) fn domain_of(url: &str) -> Option<&str> {
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
    fn browser_tab_row_keys_strip_www_prefix_so_it_matches_bare_domain_rules() {
        let (kind, key) = category_key("chrome.exe", Some("https://www.Facebook.com/foo"));
        assert_eq!(kind, KeyKind::Domain);
        assert_eq!(key, "facebook.com");
    }

    #[test]
    fn domain_extraction_strips_port_and_userinfo() {
        assert_eq!(
            domain_of("https://user@example.com:8080/path"),
            Some("example.com")
        );
    }

    #[test]
    fn domain_extraction_handles_missing_scheme() {
        assert_eq!(domain_of("example.com/path"), Some("example.com"));
    }
}
