//! User-written rules that file activities under a category, and which rule wins when several
//! match. The rules about rules live here and nowhere else:
//!
//! - a rule is one of four kinds: an **app** (an executable), a **site** (a domain), a **page**
//!   (a domain and a path prefix) or a **pattern** (a `*`/`?` glob over a URL, title or app name);
//! - everything is compared case-insensitively, and a page matches whole path segments, so
//!   `github.com/acme` covers `/acme/api` but not `/acme-labs`;
//! - when several rules match one activity, the most specific wins: a page (the longer path
//!   first), then an app or site, then a pattern (the one with more literal text first); a tie
//!   goes to the oldest rule.
//!
//! A rule sits below a pin and above an AI suggestion; that precedence is in
//! [`super::classification`] and the `activity_category` view.

use serde::{Deserialize, Serialize};

use crate::shared::ids::RuleId;
use crate::tracking::domain::source_key::{normalized_domain, url_path, KeyKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleKind {
    App,
    Site,
    Page,
    Pattern,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Site => "site",
            Self::Page => "page",
            Self::Pattern => "pattern",
        }
    }
}

impl TryFrom<&str> for RuleKind {
    type Error = UnknownValue;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "app" => Ok(Self::App),
            "site" => Ok(Self::Site),
            "page" => Ok(Self::Page),
            "pattern" => Ok(Self::Pattern),
            _ => Err(UnknownValue::new("rule kind", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatternField {
    Url,
    Title,
    App,
}

impl PatternField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Title => "title",
            Self::App => "app",
        }
    }
}

impl TryFrom<&str> for PatternField {
    type Error = UnknownValue;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "url" => Ok(Self::Url),
            "title" => Ok(Self::Title),
            "app" => Ok(Self::App),
            _ => Err(UnknownValue::new("pattern field", value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {what} {value:?}")]
pub struct UnknownValue {
    what: &'static str,
    value: String,
}

impl UnknownValue {
    fn new(what: &'static str, value: &str) -> Self {
        Self {
            what,
            value: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct InvalidRule(pub &'static str);

/// What a rule recognizes. Values are held normalized (lowercase, no scheme or `www.`), so
/// [`RuleMatcher::parts`] is what gets stored and parsing it again changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleMatcher {
    App(String),
    Site(String),
    Page { domain: String, path: String },
    Pattern { field: PatternField, glob: String },
}

impl RuleMatcher {
    /// Reads what the person typed. `field` only matters for a pattern, and is required there.
    pub fn parse(kind: RuleKind, field: Option<PatternField>, value: &str) -> Result<Self, InvalidRule> {
        let value = value.trim();
        if value.is_empty() {
            return Err(InvalidRule("enter a value for the rule"));
        }
        match kind {
            RuleKind::App => Ok(Self::App(value.to_lowercase())),
            RuleKind::Site => {
                let (domain, path) = location(value).ok_or(InvalidRule("that is not a website address"))?;
                if !path.is_empty() {
                    return Err(InvalidRule("a website rule covers a whole domain; use a page rule for a path"));
                }
                Ok(Self::Site(domain))
            }
            RuleKind::Page => {
                let (domain, path) = location(value).ok_or(InvalidRule("that is not a website address"))?;
                if path.is_empty() {
                    return Err(InvalidRule("add a path after the domain, or use a website rule"));
                }
                Ok(Self::Page { domain, path })
            }
            RuleKind::Pattern => {
                let field = field.ok_or(InvalidRule("choose what the pattern is matched against"))?;
                let glob = value.to_lowercase();
                if !glob.chars().any(|c| c != '*' && c != '?') {
                    return Err(InvalidRule("a pattern needs some text besides wildcards"));
                }
                Ok(Self::Pattern { field, glob })
            }
        }
    }

    pub fn parts(&self) -> (RuleKind, Option<PatternField>, String) {
        match self {
            Self::App(key) => (RuleKind::App, None, key.clone()),
            Self::Site(domain) => (RuleKind::Site, None, domain.clone()),
            Self::Page { domain, path } => (RuleKind::Page, None, format!("{domain}{path}")),
            Self::Pattern { field, glob } => (RuleKind::Pattern, Some(*field), glob.clone()),
        }
    }

    pub fn matches(&self, facts: &ActivityFacts) -> bool {
        match self {
            Self::App(key) => facts.kind == KeyKind::Exe && facts.key == *key,
            Self::Site(domain) => facts.kind == KeyKind::Domain && facts.key == *domain,
            Self::Page { domain, path } => {
                facts.kind == KeyKind::Domain && facts.key == *domain && path_within(&facts.path, path)
            }
            Self::Pattern { field, glob } => match field {
                PatternField::Url => facts.url.as_deref().is_some_and(|url| glob_matches(glob, url)),
                PatternField::Title => glob_matches(glob, &facts.title),
                PatternField::App => glob_matches(glob, &facts.name),
            },
        }
    }

    /// Ordered so that a greater value wins when several rules match.
    fn specificity(&self) -> (u8, usize) {
        match self {
            Self::Page { path, .. } => (3, path.len()),
            Self::App(_) | Self::Site(_) => (2, 0),
            Self::Pattern { glob, .. } => (1, glob.chars().filter(|&c| c != '*' && c != '?').count()),
        }
    }
}

fn path_within(activity_path: &str, rule_path: &str) -> bool {
    match activity_path.strip_prefix(rule_path) {
        Some("") => true,
        Some(rest) => rest.starts_with(['/', '?', '&']),
        None => false,
    }
}

/// `(domain, path)` of an address typed with or without its scheme. The path is lowercase and
/// shaped like an activity's `path` (`/a/b`, possibly with `?query`), or empty.
fn location(value: &str) -> Option<(String, String)> {
    let address = if value.contains("://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    let domain = normalized_domain(&address)?;
    Some((domain, url_path(&address).to_lowercase()))
}

/// Case-insensitive match of `text` against a glob where `*` is any run of characters and `?`
/// is exactly one. `pattern` must already be lowercase; `text` is lowercased by the caller.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    // Where the last `*` was, and how much of the text it has swallowed so far.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[derive(Debug, Clone)]
pub struct ActivityFacts {
    kind: KeyKind,
    key: String,
    name: String,
    path: String,
    title: String,
    url: Option<String>,
}

impl ActivityFacts {
    pub fn new(kind: KeyKind, key: &str, name: &str, path: &str, title: &str) -> Self {
        let key = key.to_lowercase();
        let path = path.to_lowercase();
        let url = (kind == KeyKind::Domain).then(|| format!("{key}{path}"));
        Self {
            kind,
            key,
            name: name.to_lowercase(),
            path,
            title: title.to_lowercase(),
            url,
        }
    }
}

/// A rule's identity and what it recognizes. The category it files under is the stored row's
/// to know: reads join it through `rule_matches`.
#[derive(Debug, Clone)]
pub struct CategoryRule {
    pub id: RuleId,
    pub matcher: RuleMatcher,
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<CategoryRule>,
}

impl RuleSet {
    pub fn new(rules: Vec<CategoryRule>) -> Self {
        Self { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn resolve(&self, facts: &ActivityFacts) -> Option<&CategoryRule> {
        self.rules
            .iter()
            .filter(|rule| rule.matcher.matches(facts))
            .max_by(|a, b| {
                a.matcher
                    .specificity()
                    .cmp(&b.matcher.specificity())
                    .then_with(|| b.id.cmp(&a.id))
            })
    }
}
