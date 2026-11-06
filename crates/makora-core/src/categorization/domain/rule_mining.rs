//! Rules proposed from the user's own filing: when nearly all the pinned time of one app, site or
//! page prefix sits in one category, a rule for it is suggested.
//!
//! - only pins are evidence: an unreviewed AI label is what a rule would replace, and an
//!   activity a rule files says nothing new;
//! - a candidate is an app, a site, or a page one or two path segments deep;
//! - it qualifies with enough activities and time in its top category, and a high enough share
//!   of its pinned time there (the purity);
//! - a page is not suggested when a site or shorter page above it qualifies for the same
//!   category, since that rule already covers it;
//! - a candidate an existing rule already files under the same category is not suggested; one a
//!   rule with the same matcher files elsewhere becomes a suggestion to change that rule;
//! - a dismissed (matcher, category) pair is never suggested again.

use std::collections::{HashMap, HashSet};

use crate::shared::ids::{CategoryId, RuleId};
use crate::tracking::domain::source_key::KeyKind;

use super::rules::{ActivityFacts, CategoryRule, RuleMatcher, RuleSet};

const PAGE_DEPTH: usize = 2;

#[derive(Debug, Clone)]
pub struct FiledActivity {
    pub kind: KeyKind,
    pub key: String,
    pub name: String,
    pub path: String,
    pub title: String,
    pub category: CategoryId,
    pub seconds: f64,
}

#[derive(Debug, Clone)]
pub struct ExistingRule {
    pub rule: CategoryRule,
    pub category: CategoryId,
}

#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub min_activities: usize,
    pub min_seconds: f64,
    pub min_purity: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_activities: 5,
            min_seconds: 30. * 60.,
            min_purity: 0.9,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    New,
    ChangeRule { rule_id: RuleId, current: CategoryId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleSuggestion {
    pub matcher: RuleMatcher,
    pub category: CategoryId,
    /// Pinned activities in `category`, and their time.
    pub activity_count: usize,
    pub total_seconds: f64,
    /// Every pinned activity the matcher covers, whatever its category.
    pub pinned_count: usize,
    pub purity: f64,
    pub kind: SuggestionKind,
}

#[derive(Default)]
struct Tally {
    per_category: HashMap<CategoryId, (Vec<usize>, f64)>,
}

impl Tally {
    fn add(&mut self, category: CategoryId, activity: usize, seconds: f64) {
        let entry = self.per_category.entry(category).or_default();
        entry.0.push(activity);
        entry.1 += seconds;
    }

    /// The category with the most time; a tie goes to the lower id.
    fn top(&self) -> Option<(CategoryId, &[usize], f64)> {
        self.per_category
            .iter()
            .max_by(|a, b| a.1 .1.total_cmp(&b.1 .1).then_with(|| b.0.cmp(a.0)))
            .map(|(category, (members, seconds))| (*category, members.as_slice(), *seconds))
    }

    fn count(&self) -> usize {
        self.per_category.values().map(|(members, _)| members.len()).sum()
    }

    fn seconds(&self) -> f64 {
        self.per_category.values().map(|(_, seconds)| seconds).sum()
    }
}

struct Candidate {
    suggestion: RuleSuggestion,
    /// Indexes of the activities pinned to the suggested category.
    members: Vec<usize>,
}

pub fn suggest(
    filed: &[FiledActivity],
    rules: &[ExistingRule],
    dismissed: &[(RuleMatcher, CategoryId)],
    thresholds: &Thresholds,
) -> Vec<RuleSuggestion> {
    let mut tallies: HashMap<RuleMatcher, Tally> = HashMap::new();
    for (ix, activity) in filed.iter().enumerate().filter(|(_, activity)| activity.seconds > 0.) {
        for candidate in candidates(activity) {
            tallies
                .entry(candidate)
                .or_default()
                .add(activity.category, ix, activity.seconds);
        }
    }
    let qualified: HashMap<RuleMatcher, Candidate> = tallies
        .into_iter()
        .filter_map(|(matcher, tally)| qualify(matcher, &tally, thresholds))
        .map(|candidate| (candidate.suggestion.matcher.clone(), candidate))
        .collect();

    let by_id: HashMap<RuleId, &ExistingRule> = rules.iter().map(|existing| (existing.rule.id, existing)).collect();
    let rule_set = RuleSet::new(rules.iter().map(|existing| existing.rule.clone()).collect());
    let resolved: Vec<Option<&ExistingRule>> = filed
        .iter()
        .map(|activity| {
            let facts = ActivityFacts::new(activity.kind, &activity.key, &activity.name, &activity.path, &activity.title);
            rule_set.resolve(&facts).and_then(|rule| by_id.get(&rule.id).copied())
        })
        .collect();
    let dismissed: HashSet<&(RuleMatcher, CategoryId)> = dismissed.iter().collect();

    let mut suggestions: Vec<(String, RuleSuggestion)> = qualified
        .values()
        .filter(|candidate| !covered_above(candidate, &qualified, &resolved))
        .filter_map(|candidate| {
            let kind = against_rules(candidate, rules, &resolved)?;
            Some(RuleSuggestion {
                kind,
                ..candidate.suggestion.clone()
            })
        })
        .filter(|suggestion| !dismissed.contains(&(suggestion.matcher.clone(), suggestion.category)))
        .map(|suggestion| {
            let (kind, _, value) = suggestion.matcher.parts();
            (format!("{value} {}", kind.as_str()), suggestion)
        })
        .collect();
    suggestions.sort_by(|(a_key, a), (b_key, b)| b.total_seconds.total_cmp(&a.total_seconds).then_with(|| a_key.cmp(b_key)));
    suggestions.into_iter().map(|(_, suggestion)| suggestion).collect()
}

fn candidates(activity: &FiledActivity) -> Vec<RuleMatcher> {
    let key = activity.key.to_lowercase();
    match activity.kind {
        KeyKind::Exe => vec![RuleMatcher::App(key)],
        KeyKind::Domain => {
            let mut out = vec![RuleMatcher::Site(key.clone())];
            let path = activity.path.split(['?', '#']).next().unwrap_or_default().to_lowercase();
            let mut folder = String::new();
            for segment in path.split('/').filter(|segment| !segment.is_empty()).take(PAGE_DEPTH) {
                folder.push('/');
                folder.push_str(segment);
                out.push(RuleMatcher::Page {
                    domain: key.clone(),
                    path: folder.clone(),
                });
            }
            out
        }
    }
}

fn qualify(matcher: RuleMatcher, tally: &Tally, thresholds: &Thresholds) -> Option<Candidate> {
    let (category, members, seconds) = tally.top()?;
    let all_seconds = tally.seconds();
    let purity = if all_seconds > 0. { seconds / all_seconds } else { 0. };
    let enough = members.len() >= thresholds.min_activities && seconds >= thresholds.min_seconds;
    (enough && purity >= thresholds.min_purity).then(|| Candidate {
        suggestion: RuleSuggestion {
            matcher,
            category,
            activity_count: members.len(),
            total_seconds: seconds,
            pinned_count: tally.count(),
            purity,
            kind: SuggestionKind::New,
        },
        members: members.to_vec(),
    })
}

/// The site or page one level up from a page.
fn parent(matcher: &RuleMatcher) -> Option<RuleMatcher> {
    let RuleMatcher::Page { domain, path } = matcher else {
        return None;
    };
    Some(match path.rfind('/') {
        Some(0) | None => RuleMatcher::Site(domain.clone()),
        Some(slash) => RuleMatcher::Page {
            domain: domain.clone(),
            path: path[..slash].to_string(),
        },
    })
}

/// Whether a site or page above this one qualifies for the same category and, once added, would
/// win for every one of its activities: no rule more specific than it files them elsewhere.
fn covered_above(
    candidate: &Candidate,
    qualified: &HashMap<RuleMatcher, Candidate>,
    resolved: &[Option<&ExistingRule>],
) -> bool {
    let category = candidate.suggestion.category;
    let mut above = parent(&candidate.suggestion.matcher);
    while let Some(matcher) = above {
        let same = qualified
            .get(&matcher)
            .is_some_and(|ancestor| ancestor.suggestion.category == category);
        let wins = || {
            candidate.members.iter().all(|&ix| {
                resolved[ix].is_none_or(|rule| {
                    rule.category == category
                        || rule.rule.matcher == matcher
                        || rule.rule.matcher.specificity() < matcher.specificity()
                })
            })
        };
        if same && wins() {
            return true;
        }
        above = parent(&matcher);
    }
    false
}

/// What the suggestion is given the rules there are now, or `None` when they already file its
/// activities under its category.
fn against_rules(
    candidate: &Candidate,
    rules: &[ExistingRule],
    resolved: &[Option<&ExistingRule>],
) -> Option<SuggestionKind> {
    let category = candidate.suggestion.category;
    if let Some(same) = rules
        .iter()
        .find(|existing| existing.rule.matcher == candidate.suggestion.matcher)
    {
        return (same.category != category).then_some(SuggestionKind::ChangeRule {
            rule_id: same.rule.id,
            current: same.category,
        });
    }
    let covered = candidate
        .members
        .iter()
        .all(|&ix| resolved[ix].is_some_and(|rule| rule.category == category));
    (!covered).then_some(SuggestionKind::New)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categorization::domain::rules::RuleKind;

    const WORK: CategoryId = CategoryId(1);
    const FUN: CategoryId = CategoryId(2);
    const MIN: f64 = 60.;

    fn site(key: &str, path: &str, category: CategoryId, seconds: f64) -> FiledActivity {
        FiledActivity {
            kind: KeyKind::Domain,
            key: key.into(),
            name: key.into(),
            path: path.into(),
            title: String::new(),
            category,
            seconds,
        }
    }

    fn app(key: &str, category: CategoryId, seconds: f64) -> FiledActivity {
        FiledActivity {
            kind: KeyKind::Exe,
            key: key.into(),
            name: key.into(),
            path: String::new(),
            title: String::new(),
            category,
            seconds,
        }
    }

    fn times(n: usize, make: impl Fn(usize) -> FiledActivity) -> Vec<FiledActivity> {
        (0..n).map(make).collect()
    }

    fn rule(id: i64, kind: RuleKind, value: &str, category: CategoryId) -> ExistingRule {
        ExistingRule {
            rule: CategoryRule {
                id: RuleId(id),
                matcher: RuleMatcher::parse(kind, None, value).unwrap(),
            },
            category,
        }
    }

    fn run(filed: &[FiledActivity], rules: &[ExistingRule]) -> Vec<RuleSuggestion> {
        suggest(filed, rules, &[], &Thresholds::default())
    }

    fn values(suggestions: &[RuleSuggestion]) -> Vec<String> {
        suggestions.iter().map(|s| s.matcher.parts().2).collect()
    }

    #[test]
    fn an_app_filed_one_way_is_suggested() {
        let filed = times(5, |_| app("code", WORK, 10. * MIN));
        let found = run(&filed, &[]);
        assert_eq!(values(&found), ["code"]);
        assert_eq!((found[0].category, found[0].activity_count, found[0].kind), (WORK, 5, SuggestionKind::New));
        assert_eq!(found[0].total_seconds, 50. * MIN);
    }

    #[test]
    fn too_few_activities_or_too_little_time_is_not_enough() {
        assert!(run(&times(4, |_| app("code", WORK, 60. * MIN)), &[]).is_empty());
        assert!(run(&times(5, |_| app("code", WORK, 5. * MIN)), &[]).is_empty());
    }

    #[test]
    fn mixed_filing_falls_below_the_purity_threshold() {
        let mut filed = times(9, |_| app("firefox", WORK, 10. * MIN));
        filed.push(app("firefox", FUN, 11. * MIN));
        assert!(run(&filed, &[]).is_empty());
        filed.pop();
        filed.push(app("firefox", FUN, 9. * MIN));
        let found = run(&filed, &[]);
        assert_eq!((found.len(), found[0].pinned_count), (1, 10));
        assert!((0.9..0.92).contains(&found[0].purity));
    }

    #[test]
    fn a_site_hides_its_pages_of_the_same_category() {
        let filed = times(6, |i| site("github.com", &format!("/acme/repo{i}?tab=1"), WORK, 10. * MIN));
        assert_eq!(values(&run(&filed, &[])), ["github.com"]);
    }

    #[test]
    fn a_page_filed_elsewhere_stays_as_an_exception() {
        let mut filed = times(60, |i| site("github.com", &format!("/acme/{i}"), WORK, 10. * MIN));
        filed.extend(times(5, |_| site("github.com", "/fun/game", FUN, 10. * MIN)));
        let found = run(&filed, &[]);
        assert_eq!(values(&found), ["github.com", "github.com/fun"]);
        assert_eq!(found[1].category, FUN);
    }

    #[test]
    fn pages_go_two_segments_deep() {
        let found = candidates(&site("GitHub.com", "/Acme/api/issues?x=1", WORK, 0.));
        let found: Vec<String> = found.iter().map(|m| m.parts().2).collect();
        assert_eq!(found, ["github.com", "github.com/acme", "github.com/acme/api"]);
    }

    #[test]
    fn a_rule_that_already_files_it_there_is_left_alone() {
        let filed = times(5, |i| site("github.com", &format!("/acme/{i}"), WORK, 10. * MIN));
        assert!(run(&filed, &[rule(1, RuleKind::Site, "github.com", WORK)]).is_empty());
    }

    #[test]
    fn a_rule_that_files_it_elsewhere_is_offered_for_change() {
        let filed = times(5, |_| app("code", WORK, 10. * MIN));
        let found = run(&filed, &[rule(7, RuleKind::App, "code", FUN)]);
        assert_eq!(
            found[0].kind,
            SuggestionKind::ChangeRule {
                rule_id: RuleId(7),
                current: FUN
            }
        );
    }

    #[test]
    fn a_broader_rule_filed_elsewhere_gives_a_new_narrower_rule() {
        let mut filed = times(50, |i| site("github.com", &format!("/{i}"), FUN, 10. * MIN));
        filed.extend(times(5, |_| site("github.com", "/acme", WORK, 10. * MIN)));
        let found = run(&filed, &[rule(1, RuleKind::Site, "github.com", FUN)]);
        assert_eq!(values(&found), ["github.com/acme"]);
        assert_eq!(found[0].kind, SuggestionKind::New);
    }

    #[test]
    fn a_dismissed_pair_is_not_suggested_but_another_category_is() {
        let filed = times(5, |_| app("code", WORK, 10. * MIN));
        let code = RuleMatcher::App("code".into());
        let dismissed = suggest(&filed, &[], &[(code.clone(), WORK)], &Thresholds::default());
        assert!(dismissed.is_empty());
        let other = suggest(&filed, &[], &[(code, FUN)], &Thresholds::default());
        assert_eq!(other.len(), 1);
    }

    #[test]
    fn dismissing_a_site_hides_its_pages_too() {
        let filed = times(6, |_| site("github.com", "/acme", WORK, 10. * MIN));
        let site = RuleMatcher::Site("github.com".into());
        assert!(suggest(&filed, &[], &[(site, WORK)], &Thresholds::default()).is_empty());
    }

    #[test]
    fn order_is_time_first_then_value() {
        let mut filed = times(5, |_| app("zed", WORK, 10. * MIN));
        filed.extend(times(5, |_| app("code", WORK, 10. * MIN)));
        filed.extend(times(5, |_| app("vim", WORK, 20. * MIN)));
        assert_eq!(values(&run(&filed, &[])), ["vim", "code", "zed"]);
    }

    fn pattern(id: i64, field: crate::categorization::domain::rules::PatternField, glob: &str, category: CategoryId) -> ExistingRule {
        ExistingRule {
            rule: CategoryRule {
                id: RuleId(id),
                matcher: RuleMatcher::parse(RuleKind::Pattern, Some(field), glob).unwrap(),
            },
            category,
        }
    }

    #[test]
    fn a_pattern_rule_that_already_files_the_activities_there_is_left_alone() {
        use crate::categorization::domain::rules::PatternField;
        let filed = times(5, |_| FiledActivity {
            title: "Standup notes".into(),
            ..app("obsidian", WORK, 10. * MIN)
        });
        assert!(run(&filed, &[pattern(1, PatternField::Title, "*standup*", WORK)]).is_empty());
        assert_eq!(run(&filed, &[pattern(1, PatternField::Title, "*notes*", FUN)]).len(), 1);
    }

    #[test]
    fn a_page_stays_when_a_narrower_rule_would_beat_its_site() {
        let filed = times(10, |_| site("github.com", "/acme", WORK, 10. * MIN));
        let rules = [
            rule(1, RuleKind::Site, "github.com", FUN),
            rule(2, RuleKind::Page, "github.com/acme", FUN),
        ];
        let found = run(&filed, &rules);
        assert_eq!(values(&found), ["github.com", "github.com/acme"]);
        assert!(found.iter().all(|s| matches!(s.kind, SuggestionKind::ChangeRule { .. })));
    }

    #[test]
    fn untracked_pins_are_not_evidence() {
        let mut filed = times(5, |_| app("code", WORK, 0.));
        filed.push(app("code", WORK, 31. * MIN));
        assert!(run(&filed, &[]).is_empty());
    }
}
