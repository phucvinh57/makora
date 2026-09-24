use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// How a rule's pattern is compared against a session key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    Exact,
    Prefix,
    Substring,
    Glob,
}

impl MatchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Prefix => "prefix",
            Self::Substring => "substring",
            Self::Glob => "glob",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exact" => Some(Self::Exact),
            "prefix" => Some(Self::Prefix),
            "substring" => Some(Self::Substring),
            "glob" => Some(Self::Glob),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
enum PatternMatcher {
    Exact(String),
    Prefix(String),
    Substring(String),
    Glob(GlobPattern),
}

impl PatternMatcher {
    fn compile(kind: MatchKind, pattern: &str) -> Result<Self> {
        let folded = pattern.to_ascii_lowercase();
        Ok(match kind {
            MatchKind::Exact => Self::Exact(folded),
            MatchKind::Prefix => Self::Prefix(folded),
            MatchKind::Substring => Self::Substring(folded),
            MatchKind::Glob => Self::Glob(GlobPattern::compile(&folded)?),
        })
    }

    fn matches(&self, candidate: &str) -> bool {
        let candidate = candidate.to_ascii_lowercase();
        match self {
            Self::Exact(pattern) => candidate == *pattern,
            Self::Prefix(pattern) => candidate.starts_with(pattern),
            Self::Substring(pattern) => candidate.contains(pattern),
            Self::Glob(pattern) => pattern.matches(&candidate),
        }
    }
}

#[derive(Debug, Clone)]
struct GlobPattern {
    tokens: Vec<GlobToken>,
}

#[derive(Debug, Clone)]
enum GlobToken {
    Literal(char),
    AnyChar,
    AnySequence,
    Class(CharClass),
}

#[derive(Debug, Clone)]
struct CharClass {
    negated: bool,
    items: Vec<ClassItem>,
}

#[derive(Debug, Clone)]
enum ClassItem {
    Literal(char),
    Range(char, char),
}

impl GlobPattern {
    fn compile(pattern: &str) -> Result<Self> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut tokens = Vec::new();
        let mut index = 0;
        while index < chars.len() {
            match chars[index] {
                '*' => {
                    if !matches!(tokens.last(), Some(GlobToken::AnySequence)) {
                        tokens.push(GlobToken::AnySequence);
                    }
                    index += 1;
                }
                '?' => {
                    tokens.push(GlobToken::AnyChar);
                    index += 1;
                }
                '[' => {
                    let (class, next) = CharClass::parse(&chars, index)?;
                    tokens.push(GlobToken::Class(class));
                    index = next;
                }
                literal => {
                    tokens.push(GlobToken::Literal(literal));
                    index += 1;
                }
            }
        }
        Ok(Self { tokens })
    }

    fn matches(&self, candidate: &str) -> bool {
        let chars: Vec<char> = candidate.chars().collect();
        let mut memo = HashMap::new();
        self.matches_from(0, 0, &chars, &mut memo)
    }

    fn matches_from(
        &self,
        token_index: usize,
        char_index: usize,
        chars: &[char],
        memo: &mut HashMap<(usize, usize), bool>,
    ) -> bool {
        if let Some(result) = memo.get(&(token_index, char_index)) {
            return *result;
        }
        let result = match self.tokens.get(token_index) {
            None => char_index == chars.len(),
            Some(GlobToken::AnySequence) => {
                self.matches_from(token_index + 1, char_index, chars, memo)
                    || (char_index < chars.len()
                        && self.matches_from(token_index, char_index + 1, chars, memo))
            }
            Some(GlobToken::AnyChar) => {
                char_index < chars.len()
                    && self.matches_from(token_index + 1, char_index + 1, chars, memo)
            }
            Some(GlobToken::Literal(expected)) => {
                chars.get(char_index) == Some(expected)
                    && self.matches_from(token_index + 1, char_index + 1, chars, memo)
            }
            Some(GlobToken::Class(class)) => {
                chars
                    .get(char_index)
                    .is_some_and(|value| class.matches(*value))
                    && self.matches_from(token_index + 1, char_index + 1, chars, memo)
            }
        };
        memo.insert((token_index, char_index), result);
        result
    }
}

impl CharClass {
    fn parse(chars: &[char], opening: usize) -> Result<(Self, usize)> {
        let mut index = opening + 1;
        let negated = chars.get(index) == Some(&'^');
        if negated {
            index += 1;
        }

        let mut members = Vec::new();
        if chars.get(index) == Some(&']') {
            members.push(']');
            index += 1;
        }
        while let Some(value) = chars.get(index).copied() {
            if value == ']' {
                break;
            }
            members.push(value);
            index += 1;
        }
        if chars.get(index) != Some(&']') || members.is_empty() {
            return Err(anyhow!(
                "invalid glob pattern: unterminated or empty character class"
            ));
        }

        let mut items = Vec::new();
        let mut member_index = 0;
        while member_index < members.len() {
            if member_index + 2 < members.len() && members[member_index + 1] == '-' {
                let start = members[member_index];
                let end = members[member_index + 2];
                if start > end {
                    return Err(anyhow!("invalid glob pattern: descending character range"));
                }
                items.push(ClassItem::Range(start, end));
                member_index += 3;
            } else {
                items.push(ClassItem::Literal(members[member_index]));
                member_index += 1;
            }
        }
        Ok((Self { negated, items }, index + 1))
    }

    fn matches(&self, value: char) -> bool {
        let contained = self.items.iter().any(|item| match *item {
            ClassItem::Literal(expected) => expected == value,
            ClassItem::Range(start, end) => (start..=end).contains(&value),
        });
        contained != self.negated
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuleSpec {
    pub id: i64,
    pub key_kind: KeyKind,
    pub pattern: String,
    pub match_kind: MatchKind,
    pub category_id: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledRule {
    id: i64,
    key_kind: KeyKind,
    pattern_len: usize,
    match_kind: MatchKind,
    matcher: PatternMatcher,
    category_id: i64,
}

impl RuleSpec {
    pub(crate) fn compile(self) -> Result<CompiledRule> {
        Ok(CompiledRule {
            id: self.id,
            key_kind: self.key_kind,
            pattern_len: self.pattern.chars().count(),
            match_kind: self.match_kind,
            matcher: PatternMatcher::compile(self.match_kind, &self.pattern)?,
            category_id: self.category_id,
        })
    }
}

pub(crate) fn validate_pattern(kind: MatchKind, pattern: &str) -> Result<()> {
    PatternMatcher::compile(kind, pattern).map(|_| ())
}

pub(crate) fn pattern_matches(kind: MatchKind, pattern: &str, candidate: &str) -> bool {
    PatternMatcher::compile(kind, pattern).is_ok_and(|matcher| matcher.matches(candidate))
}

pub(crate) fn compile_rules<I>(rules: I) -> Vec<CompiledRule>
where
    I: IntoIterator<Item = RuleSpec>,
{
    let mut compiled: Vec<_> = rules
        .into_iter()
        // Old SQLite-backed versions accepted malformed globs. They remain visible in the
        // rule list, but cannot prevent the rest of history from being categorized.
        .filter_map(|rule| rule.compile().ok())
        .collect();
    compiled.sort_by_key(|rule| {
        (
            if rule.key_kind == KeyKind::Url { 0 } else { 1 },
            match_priority(rule.match_kind),
            Reverse(rule.pattern_len),
            rule.id,
        )
    });
    compiled
}

fn match_priority(kind: MatchKind) -> u8 {
    match kind {
        MatchKind::Exact => 0,
        MatchKind::Prefix => 1,
        MatchKind::Substring => 2,
        MatchKind::Glob => 3,
    }
}

pub(crate) struct SessionCategoryInput<'a> {
    pub key_kind: KeyKind,
    pub key: &'a str,
    pub url_key: Option<&'a str>,
}

pub(crate) fn resolve_category(
    rules: &[CompiledRule],
    input: &SessionCategoryInput<'_>,
) -> Option<i64> {
    for rule in rules {
        let candidate = match rule.key_kind {
            KeyKind::Url => input.url_key,
            kind if kind == input.key_kind => Some(input.key),
            _ => None,
        };
        if candidate.is_some_and(|candidate| rule.matcher.matches(candidate)) {
            return Some(rule.category_id);
        }
    }

    None
}

#[derive(Debug, Clone)]
pub(crate) struct CategoryTree {
    ids: HashSet<i64>,
    children: HashMap<i64, Vec<i64>>,
}

impl CategoryTree {
    pub(crate) fn new<I>(categories: I) -> Self
    where
        I: IntoIterator<Item = (i64, Option<i64>)>,
    {
        let mut ids = HashSet::new();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for (id, parent_id) in categories {
            ids.insert(id);
            if let Some(parent_id) = parent_id {
                children.entry(parent_id).or_default().push(id);
            }
        }
        for child_ids in children.values_mut() {
            child_ids.sort_unstable();
        }
        Self { ids, children }
    }

    pub(crate) fn contains(&self, id: i64) -> bool {
        self.ids.contains(&id)
    }

    pub(crate) fn descendants(&self, root: i64) -> Vec<i64> {
        if !self.contains(root) {
            return Vec::new();
        }
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            result.push(id);
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev().copied());
            }
        }
        result
    }

    pub(crate) fn deletion_order(&self, root: i64) -> Vec<i64> {
        let descendants = self.descendants(root);
        let descendant_set: HashSet<_> = descendants.iter().copied().collect();
        let mut depth = HashMap::new();
        depth.insert(root, 0usize);
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let next_depth = depth.get(&id).copied().unwrap_or(0) + 1;
            if let Some(children) = self.children.get(&id) {
                for child in children {
                    if descendant_set.contains(child) && !depth.contains_key(child) {
                        depth.insert(*child, next_depth);
                        stack.push(*child);
                    }
                }
            }
        }
        let mut ordered = descendants;
        ordered.sort_by_key(|id| (Reverse(depth.get(id).copied().unwrap_or(0)), *id));
        ordered
    }
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
    fn match_kind_parses_glob_and_no_longer_accepts_the_retired_regex_kind() {
        assert_eq!(MatchKind::parse("glob"), Some(MatchKind::Glob));
        assert_eq!(MatchKind::Glob.as_str(), "glob");
        assert_eq!(MatchKind::parse("regex"), None);
    }

    #[test]
    fn all_match_kinds_are_ascii_case_insensitive() {
        assert!(pattern_matches(MatchKind::Exact, "Code.EXE", "code.exe"));
        assert!(pattern_matches(
            MatchKind::Prefix,
            "GITHUB",
            "github.com/path"
        ));
        assert!(pattern_matches(
            MatchKind::Substring,
            "Hub.Com",
            "github.com"
        ));
        assert!(pattern_matches(
            MatchKind::Glob,
            "GAME*.EXE",
            "gameClient.exe"
        ));
    }

    #[test]
    fn glob_is_anchored_crosses_slashes_and_supports_classes() {
        assert!(pattern_matches(
            MatchKind::Glob,
            "github.com/*/pulls",
            "github.com/org/repo/pulls"
        ));
        assert!(!pattern_matches(
            MatchKind::Glob,
            "*.corp.example.com",
            "corp.example.com.attacker.net"
        ));
        assert!(pattern_matches(
            MatchKind::Glob,
            "game[0-9].exe",
            "game7.exe"
        ));
        assert!(pattern_matches(MatchKind::Glob, "file[^0-9]", "filex"));
        assert!(pattern_matches(MatchKind::Glob, "[[]draft]", "[draft]"));
        assert!(pattern_matches(
            MatchKind::Glob,
            "github.com/**/pulls",
            "github.com/org/repo/pulls"
        ));
    }

    #[test]
    fn malformed_globs_are_rejected() {
        assert!(validate_pattern(MatchKind::Glob, "unterminated[").is_err());
        assert!(validate_pattern(MatchKind::Glob, "[]").is_err());
        assert!(validate_pattern(MatchKind::Glob, "[z-a]").is_err());
    }

    #[test]
    fn resolver_uses_the_highest_ranked_matching_rule() {
        let rules = compile_rules([
            RuleSpec {
                id: 1,
                key_kind: KeyKind::Domain,
                pattern: "github.com".into(),
                match_kind: MatchKind::Exact,
                category_id: 10,
            },
            RuleSpec {
                id: 2,
                key_kind: KeyKind::Url,
                pattern: "github.com/org/repo".into(),
                match_kind: MatchKind::Prefix,
                category_id: 20,
            },
        ]);
        let input = SessionCategoryInput {
            key_kind: KeyKind::Domain,
            key: "github.com",
            url_key: Some("github.com/org/repo/pulls"),
        };
        assert_eq!(resolve_category(&rules, &input), Some(20));
    }

    #[test]
    fn unmatched_sessions_remain_uncategorized() {
        let input = SessionCategoryInput {
            key_kind: KeyKind::Exe,
            key: "unknown.exe",
            url_key: None,
        };
        assert_eq!(resolve_category(&[], &input), None);
    }

    #[test]
    fn category_tree_expands_descendants_and_deletes_deepest_first() {
        let tree = CategoryTree::new([
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, Some(1)),
            (5, None),
        ]);
        assert_eq!(tree.descendants(1), vec![1, 2, 3, 4]);
        assert_eq!(tree.deletion_order(1), vec![3, 2, 4, 1]);
        assert!(tree.descendants(99).is_empty());
    }

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
