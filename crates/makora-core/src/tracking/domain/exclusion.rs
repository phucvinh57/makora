//! What the person asked not to be tracked. An exclusion is written like a categorization rule —
//! an app, a website, a page or a pattern — and is matched by the same matcher, so the two
//! always agree on what `github.com/acme` or `*standup*` covers.
//!
//! One difference: an app exclusion covers the app even while a browser tab is being reported
//! for it, so excluding a browser drops everything done in it, not only the tabs the extension
//! could not name.

use serde::{Deserialize, Serialize};

use crate::categorization::domain::rules::{ActivityFacts, InvalidRule, PatternField, RuleKind, RuleMatcher};

use super::events::FocusedApp;
use super::source_key::{url_path, KeyKind, SourceKey};
use super::title::canonical_title;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Exclusion {
    pub kind: RuleKind,
    /// Only for a pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<PatternField>,
    pub value: String,
}

impl Exclusion {
    /// Reads what the person typed and keeps it in its normalized form, so the same exclusion
    /// typed two ways is one entry.
    pub fn parse(kind: RuleKind, field: Option<PatternField>, value: &str) -> Result<Self, InvalidRule> {
        RuleMatcher::parse(kind, field, value).map(|matcher| Self::from(&matcher))
    }

    fn matcher(&self) -> Result<RuleMatcher, InvalidRule> {
        RuleMatcher::parse(self.kind, self.field, &self.value)
    }
}

impl From<&RuleMatcher> for Exclusion {
    fn from(matcher: &RuleMatcher) -> Self {
        let (kind, field, value) = matcher.parts();
        Self { kind, field, value }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExclusionSet {
    matchers: Vec<RuleMatcher>,
}

impl ExclusionSet {
    /// An entry that no longer parses excludes nothing rather than everything.
    pub fn new(exclusions: &[Exclusion]) -> Self {
        Self {
            matchers: exclusions.iter().filter_map(|exclusion| exclusion.matcher().ok()).collect(),
        }
    }

    pub fn excludes(&self, app: &FocusedApp, window_title: &str, url: Option<&str>) -> bool {
        if self.matchers.is_empty() {
            return false;
        }
        let title = canonical_title(window_title, &app.app_name, url);

        let exe = SourceKey::of(&app.exe_path, None);
        let app_facts = ActivityFacts::new(exe.kind, &exe.key, &app.app_name, "", &title);
        let site_facts = url.and_then(|url| {
            let source = SourceKey::of(&app.exe_path, Some(url));
            (source.kind == KeyKind::Domain).then(|| {
                ActivityFacts::new(
                    source.kind,
                    &source.key,
                    &source.display_name(&app.app_name),
                    &url_path(url),
                    &title,
                )
            })
        });

        self.matchers
            .iter()
            .any(|matcher| matcher.matches(&app_facts) || site_facts.as_ref().is_some_and(|facts| matcher.matches(facts)))
    }
}
