//! What is taken out of an activity before a remote provider sees it. The local model always gets
//! the full text; only the remote branch of [`super::router`] runs it through a [`Redactor`].
//!
//! Personal data the finders in [`detectors`] recognize, words the person listed, and optionally
//! their own name are replaced by placeholders such as `[email]`; ids are taken out of page paths
//! per the [`UrlPolicy`]; and an activity on the never-send list goes out with its title, address
//! and site hidden, and its app too when the entry names the app.

mod account;
mod apply;
mod detectors;
mod url;

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Deserializer, Serialize};
use tracing::warn;

use crate::categorization::domain::retrieval::{activity_text, RelatedActivity, SourceRef};
use crate::categorization::domain::rules::{PatternField, RuleMatcher};
use crate::categorization::ports::ActivityText;
use crate::shared::sync::lock;
use crate::shared::text::single_line;
use crate::tracking::domain::exclusion::Exclusion;
use crate::tracking::domain::source_key::KeyKind;

use apply::Hit;
use detectors::Literals;

pub use url::UrlPolicy;

/// The names "Redact my account name" hides: the OS login, home folder and full name.
pub fn account_names() -> &'static [String] {
    account::names()
}

/// A kind of personal data a detector finds, each switched on or off in the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Email,
    Phone,
    Ip,
    Card,
    Iban,
    Secret,
    Uuid,
    LongNumber,
}

impl Kind {
    pub const ALL: [Kind; 8] = [
        Self::Email,
        Self::Phone,
        Self::Ip,
        Self::Card,
        Self::Iban,
        Self::Secret,
        Self::Uuid,
        Self::LongNumber,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Phone => "phone",
            Self::Ip => "ip",
            Self::Card => "card",
            Self::Iban => "iban",
            Self::Secret => "secret",
            Self::Uuid => "uuid",
            Self::LongNumber => "long_number",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    fn find(self, text: &str) -> detectors::Spans {
        match self {
            Self::Email => detectors::emails(text),
            Self::Phone => detectors::phones(text),
            Self::Ip => detectors::ip_addresses(text),
            Self::Card => detectors::card_numbers(text),
            Self::Iban => detectors::ibans(text),
            Self::Secret => detectors::secrets(text),
            Self::Uuid => detectors::uuids(text),
            Self::LongNumber => detectors::long_numbers(text),
        }
    }
}

/// What a stretch of redacted text was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    Kind(Kind),
    Term,
    Account,
}

impl Found {
    fn placeholder(self) -> &'static str {
        match self {
            Self::Kind(Kind::Email) => "[email]",
            Self::Kind(Kind::Phone) => "[phone]",
            Self::Kind(Kind::Ip) => "[ip]",
            Self::Kind(Kind::Card) => "[card]",
            Self::Kind(Kind::Iban) => "[iban]",
            Self::Kind(Kind::Secret) => "[secret]",
            Self::Kind(Kind::Uuid | Kind::LongNumber) => "[id]",
            Self::Term => "[term]",
            Self::Account => "[name]",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Kind(kind) => kind.as_str(),
            Self::Term => "term",
            Self::Account => "account",
        }
    }

    /// Which find wins when two cover exactly the same text; lower first.
    fn priority(self) -> u8 {
        match self {
            Self::Term => 0,
            Self::Account => 1,
            Self::Kind(Kind::Email) => 2,
            Self::Kind(Kind::Secret) => 3,
            Self::Kind(Kind::Uuid) => 4,
            Self::Kind(Kind::Card) => 5,
            Self::Kind(Kind::Iban) => 6,
            Self::Kind(Kind::Ip) => 7,
            Self::Kind(Kind::Phone) => 8,
            Self::Kind(Kind::LongNumber) => 9,
        }
    }
}

/// The `[ai.privacy]` settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    #[serde(deserialize_with = "known_kinds")]
    pub redact: Vec<Kind>,
    /// Off by default: a short login name matches too much ordinary text.
    pub redact_account: bool,
    pub terms: Vec<String>,
    #[serde(deserialize_with = "known_url_policy")]
    pub url_policy: UrlPolicy,
    /// Apps, sites, pages and patterns whose activities are sent masked: no title or address.
    pub never_send: Vec<Exclusion>,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            redact: Kind::ALL.to_vec(),
            redact_account: false,
            terms: Vec::new(),
            url_policy: UrlPolicy::default(),
            never_send: Vec::new(),
        }
    }
}

impl PrivacySettings {
    /// Kinds in their usual order without repeats; terms on one line, without blanks or repeats
    /// that differ only in case. `never_send` is left to the caller, which parses it like the
    /// tracking exclusions.
    pub fn normalized(mut self) -> Self {
        self.redact = Kind::ALL.into_iter().filter(|kind| self.redact.contains(kind)).collect();
        let mut terms: Vec<String> = Vec::with_capacity(self.terms.len());
        for term in self.terms.iter().map(|term| single_line(term)) {
            if !term.is_empty() && !terms.iter().any(|other| other.to_lowercase() == term.to_lowercase()) {
                terms.push(term);
            }
        }
        self.terms = terms;
        self
    }
}

fn known_kinds<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Kind>, D::Error> {
    let names = Vec::<String>::deserialize(deserializer)?;
    Ok(names
        .iter()
        .filter_map(|name| {
            let kind = Kind::parse(name);
            if kind.is_none() {
                warn!(kind = %name, "ai.privacy.redact names an unknown kind; ignoring");
            }
            kind
        })
        .collect())
}

fn known_url_policy<'de, D: Deserializer<'de>>(deserializer: D) -> Result<UrlPolicy, D::Error> {
    let name = String::deserialize(deserializer)?;
    Ok(UrlPolicy::parse(&name).unwrap_or_else(|| {
        let fallback = UrlPolicy::default();
        warn!(value = %name, fallback = fallback.as_str(), "ai.privacy.url_policy is not one of full, strip_ids, domain_only; using the default");
        fallback
    }))
}

/// How many of each kind were taken out during one run, for the network log. Filled from the
/// worker threads that build the prompts.
#[derive(Debug, Default)]
pub struct RedactionReport(Mutex<BTreeMap<&'static str, u32>>);

impl RedactionReport {
    fn add(&self, name: &'static str) {
        *lock(&self.0).entry(name).or_default() += 1;
    }

    pub fn total(&self) -> u32 {
        lock(&self.0).values().sum()
    }

    /// `email:3,secret:1`, or empty when nothing was taken out.
    pub fn summary(&self) -> String {
        lock(&self.0)
            .iter()
            .map(|(name, count)| format!("{name}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

const NEVER_SEND: &str = "never_send";

/// One activity as it is sent today and as it would be sent with the settings being previewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyPreview {
    pub before: String,
    pub after: String,
    /// It is on the never-send list.
    pub masked: bool,
}

pub struct Redactor {
    kinds: Vec<Kind>,
    terms: Literals,
    account: Literals,
    url_policy: UrlPolicy,
    never_send: Vec<RuleMatcher>,
}

impl Redactor {
    pub fn new(settings: &PrivacySettings) -> Self {
        let account = if settings.redact_account {
            Literals::new(account::names())
        } else {
            Literals::default()
        };
        Self {
            kinds: settings.redact.clone(),
            terms: Literals::new(&settings.terms),
            account,
            url_policy: settings.url_policy,
            never_send: settings
                .never_send
                .iter()
                .filter_map(|entry| RuleMatcher::parse(entry.kind, entry.field, &entry.value).ok())
                .collect(),
        }
    }

    pub fn redact_activity(&self, activity: &ActivityText, report: &RedactionReport) -> ActivityText {
        if let Some(matcher) = self.never_send_match(&activity.source, &activity.app_name, &activity.title) {
            report.add(NEVER_SEND);
            return self.masked(activity, matcher, report);
        }
        ActivityText {
            app_name: self.redact_text(&activity.app_name, report),
            url: activity.url.as_deref().map(|url| self.redact_url(url, report)),
            title: self.redact_text(&activity.title, report),
            source: activity.source.clone(),
        }
    }

    /// `None` for an activity on the never-send list: an example says nothing once masked.
    pub fn redact_related(&self, related: &RelatedActivity, report: &RedactionReport) -> Option<RelatedActivity> {
        if self.never_send_match(&related.source, &related.app_name, &related.title).is_some() {
            report.add(NEVER_SEND);
            return None;
        }
        Some(RelatedActivity {
            app_name: self.redact_text(&related.app_name, report),
            url: related.url.as_deref().map(|url| self.redact_url(url, report)),
            title: self.redact_text(&related.title, report),
            ..related.clone()
        })
    }

    pub fn preview(&self, activity: &ActivityText) -> PrivacyPreview {
        let report = RedactionReport::default();
        let after = self.redact_activity(activity, &report);
        let masked = lock(&report.0).contains_key(NEVER_SEND);
        PrivacyPreview {
            before: activity_text(&activity.app_name, activity.url.as_deref(), &activity.title),
            after: activity_text(&after.app_name, after.url.as_deref(), &after.title),
            masked,
        }
    }

    fn never_send_match(&self, source: &SourceRef, app_name: &str, title: &str) -> Option<&RuleMatcher> {
        if self.never_send.is_empty() {
            return None;
        }
        let facts = source.facts(app_name, title);
        self.never_send.iter().find(|matcher| matcher.matches(&facts))
    }

    /// The app and nothing else; for a site, whose app name is its domain, not even that.
    fn masked(&self, activity: &ActivityText, matcher: &RuleMatcher, report: &RedactionReport) -> ActivityText {
        let app_name = if activity.source.kind == KeyKind::Domain {
            "[site]".to_string()
        } else if matches!(matcher, RuleMatcher::App(_) | RuleMatcher::Pattern { field: PatternField::App, .. }) {
            "[app]".to_string()
        } else {
            self.redact_text(&activity.app_name, report)
        };
        ActivityText {
            app_name,
            url: None,
            title: "[hidden]".to_string(),
            source: activity.source.clone(),
        }
    }

    fn redact_url(&self, location: &str, report: &RedactionReport) -> String {
        self.redact_text(&self.url_policy.apply(&url::decoded(location)), report)
    }

    /// Spaces of any kind are made plain first, so a number split by a non-breaking space or a
    /// tab is found as well as one split by a space.
    fn redact_text(&self, text: &str, report: &RedactionReport) -> String {
        let text = single_line(text);
        let text = text.as_str();
        let mut hits: Vec<Hit> = Vec::new();
        let mut collect = |spans: detectors::Spans, found: Found| {
            hits.extend(spans.into_iter().map(|range| Hit { range, found }));
        };
        collect(self.terms.find(text), Found::Term);
        collect(self.account.find(text), Found::Account);
        for kind in &self.kinds {
            collect(kind.find(text), Found::Kind(*kind));
        }
        let hits = apply::resolve(text, hits);
        for hit in &hits {
            report.add(hit.found.name());
        }
        apply::replace(text, &hits)
    }
}

#[cfg(test)]
mod tests {
    use crate::categorization::domain::rules::RuleKind;
    use crate::shared::ids::CategoryId;

    use super::*;

    fn app(app_name: &str, exe: &str, title: &str) -> ActivityText {
        ActivityText {
            app_name: app_name.to_string(),
            url: None,
            title: title.to_string(),
            source: SourceRef {
                kind: KeyKind::Exe,
                key: exe.to_string(),
                path: String::new(),
            },
        }
    }

    fn page(location: &str, title: &str) -> ActivityText {
        let (domain, path) = location.split_once('/').unwrap_or((location, ""));
        ActivityText {
            app_name: domain.to_string(),
            url: Some(location.to_string()),
            title: title.to_string(),
            source: SourceRef {
                kind: KeyKind::Domain,
                key: domain.to_string(),
                path: if path.is_empty() { String::new() } else { format!("/{path}") },
            },
        }
    }

    fn never_send(kind: RuleKind, field: Option<PatternField>, value: &str) -> PrivacySettings {
        PrivacySettings {
            never_send: vec![Exclusion::parse(kind, field, value).unwrap()],
            ..PrivacySettings::default()
        }
    }

    #[test]
    fn personal_data_is_replaced_and_counted() {
        let settings = PrivacySettings {
            terms: vec!["Acme Corp".into()],
            ..PrivacySettings::default()
        };
        let redactor = Redactor::new(&settings);
        let report = RedactionReport::default();
        let activity = page(
            "mail.google.com/mail/u/0/inbox/18c2f3a9b7d4e5f6",
            "Invoice 98765432 from billing@acme.com, Acme Corp - call +1 415 555 0132",
        );
        let sent = redactor.redact_activity(&activity, &report);
        assert_eq!(sent.title, "Invoice [id] from [email], [term] - call [phone]");
        assert_eq!(sent.url.as_deref(), Some("mail.google.com/mail/u/{id}/inbox/{id}"));
        assert_eq!(report.summary(), "email:1,long_number:1,phone:1,term:1");
        assert_eq!(report.total(), 4);
    }

    #[test]
    fn numbers_split_by_odd_spaces_are_found() {
        let redactor = Redactor::new(&PrivacySettings::default());
        let report = RedactionReport::default();
        let sent = redactor.redact_activity(&app("Pay", "pay", "Card 4111\u{a0}1111\u{a0}1111\u{a0}1111\tok"), &report);
        assert_eq!(sent.title, "Card [card] ok");
    }

    #[test]
    fn escaped_addresses_in_paths_are_found() {
        let redactor = Redactor::new(&PrivacySettings::default());
        let report = RedactionReport::default();
        let sent = redactor.redact_activity(&page("app.acme.com/users/jane.doe%40acme.com/settings", "Settings"), &report);
        assert_eq!(sent.url.as_deref(), Some("app.acme.com/users/[email]/settings"));
    }

    #[test]
    fn redacting_twice_changes_nothing() {
        let settings = PrivacySettings {
            terms: vec!["email".into(), "id".into()],
            ..PrivacySettings::default()
        };
        let redactor = Redactor::new(&settings);
        let report = RedactionReport::default();
        let once = redactor.redact_activity(&app("Slack", "slack", "DM jane@acme.com re 12345678"), &report);
        let twice = redactor.redact_activity(&once, &report);
        assert_eq!(once.title, twice.title);
    }

    #[test]
    fn switched_off_kinds_are_kept() {
        let settings = PrivacySettings {
            redact: vec![Kind::Email],
            url_policy: UrlPolicy::Full,
            ..PrivacySettings::default()
        };
        let redactor = Redactor::new(&settings);
        let report = RedactionReport::default();
        let sent = redactor.redact_activity(&page("github.com/acme/api/pull/42", "PR 12345678 by a@b.io"), &report);
        assert_eq!(sent.title, "PR 12345678 by [email]");
        assert_eq!(sent.url.as_deref(), Some("github.com/acme/api/pull/42"));
    }

    #[test]
    fn a_never_send_site_goes_out_masked() {
        let redactor = Redactor::new(&never_send(RuleKind::Site, None, "mybank.com"));
        let report = RedactionReport::default();
        let sent = redactor.redact_activity(&page("mybank.com/accounts/checking", "Checking - Jane Doe"), &report);
        assert_eq!((sent.app_name.as_str(), sent.url.as_deref(), sent.title.as_str()), ("[site]", None, "[hidden]"));
        assert_eq!(report.summary(), "never_send:1");
    }

    #[test]
    fn never_send_apps_pages_and_patterns_are_masked() {
        let report = RedactionReport::default();
        let by_app = Redactor::new(&never_send(RuleKind::App, None, "signal"));
        let sent = by_app.redact_activity(&app("Signal", "signal", "Chat with Mom"), &report);
        assert_eq!((sent.app_name.as_str(), sent.title.as_str()), ("[app]", "[hidden]"));

        let by_page = Redactor::new(&never_send(RuleKind::Page, None, "github.com/acme-private"));
        let sent = by_page.redact_activity(&page("github.com/acme-private/secrets", "secrets repo"), &report);
        assert_eq!(sent.title, "[hidden]");
        let kept = by_page.redact_activity(&page("github.com/acme/api", "api repo"), &report);
        assert_eq!(kept.title, "api repo");

        let by_title = Redactor::new(&never_send(RuleKind::Pattern, Some(PatternField::Title), "*therapy*"));
        let sent = by_title.redact_activity(&app("Zoom", "zoom", "Therapy session"), &report);
        assert_eq!((sent.app_name.as_str(), sent.title.as_str()), ("Zoom", "[hidden]"));
    }

    #[test]
    fn never_send_examples_are_dropped() {
        let redactor = Redactor::new(&never_send(RuleKind::Site, None, "mybank.com"));
        let report = RedactionReport::default();
        let related = |activity: ActivityText| RelatedActivity {
            app_name: activity.app_name,
            url: activity.url,
            title: activity.title,
            category: CategoryId(1),
            source: activity.source,
        };
        assert!(redactor.redact_related(&related(page("mybank.com/home", "Home")), &report).is_none());
        let kept = redactor.redact_related(&related(page("github.com/acme", "mail a@b.io")), &report).unwrap();
        assert_eq!(kept.title, "mail [email]");
    }

    #[test]
    fn the_preview_shows_both_sides() {
        let redactor = Redactor::new(&PrivacySettings::default());
        let preview = redactor.preview(&page("github.com/acme/api/pull/42", "PR by a@b.io"));
        assert_eq!(preview.before, "github.com | github.com/acme/api/pull/42 | PR by a@b.io");
        assert_eq!(preview.after, "github.com | github.com/acme/api/pull/{id} | PR by [email]");
        assert!(!preview.masked);
    }

    #[test]
    fn settings_are_normalized() {
        let settings = PrivacySettings {
            redact: vec![Kind::Secret, Kind::Email, Kind::Secret],
            terms: vec!["  Acme\n Corp ".into(), "acme corp".into(), " ".into(), "Jane".into()],
            ..PrivacySettings::default()
        }
        .normalized();
        assert_eq!(settings.redact, [Kind::Email, Kind::Secret]);
        assert_eq!(settings.terms, ["Acme Corp", "Jane"]);
    }
}
