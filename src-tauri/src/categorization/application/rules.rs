//! Writing and reading the user's categorization rules. Every change to a rule re-evaluates all
//! activities against the new set in the same transaction, so a rule is retroactive: it files
//! what was already tracked, not just what comes next. The rules about rules themselves are in
//! [`crate::categorization::domain::rules`].

use std::collections::HashMap;

use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::Connection;
use serde::Serialize;

use crate::categorization::domain::rules::{ActivityFacts, InvalidRule, PatternField, RuleKind, RuleMatcher};
use crate::categorization::rules_store;
use crate::db::models::CategoryRuleValues;
use crate::db::{self, Db};
use crate::reporting::rules::RulesReportRepo;
use crate::shared::ids::{CategoryId, RuleId};
use crate::tracking::domain::source_key::KeyKind;

/// How many pages are offered for a domain while a page rule is being written.
const PATH_SUGGESTION_LIMIT: usize = 50;
/// How many example activities a preview names.
const PREVIEW_SAMPLES: usize = 5;

/// Why a change to the rules was refused.
#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error(transparent)]
    Invalid(#[from] InvalidRule),
    #[error("there is already a rule like this")]
    Duplicate,
    #[error("that category does not exist")]
    CategoryMissing,
    #[error("rule {0} does not exist")]
    NotFound(RuleId),
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

impl From<DieselError> for RuleError {
    fn from(err: DieselError) -> Self {
        match &err {
            DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => Self::Duplicate,
            DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) => Self::CategoryMissing,
            _ => Self::Failed(err.into()),
        }
    }
}

/// What the person typed into the rule form.
#[derive(Debug, Clone)]
pub struct RuleDraft {
    pub kind: RuleKind,
    /// Only for a pattern.
    pub field: Option<PatternField>,
    pub value: String,
    pub category_id: CategoryId,
}

/// A rule as the rules page lists it, with how much it covers right now.
#[derive(Debug, Clone, Serialize)]
pub struct CategoryRuleView {
    pub id: RuleId,
    pub kind: RuleKind,
    pub field: Option<PatternField>,
    pub value: String,
    pub category_id: CategoryId,
    /// Activities this rule decides now; ones the user pinned elsewhere are not counted.
    pub activity_count: i64,
    pub total_seconds: f64,
}

/// What a draft rule would catch among the tracked history.
#[derive(Debug, Clone, Serialize)]
pub struct RulePreview {
    pub activity_count: i64,
    pub total_seconds: f64,
    /// Of those, how many the user pinned to a category themselves, which the rule will not
    /// change.
    pub pinned_count: i64,
    /// The longest-tracked matches, to recognize them by.
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceSuggestion {
    pub kind: KeyKind,
    pub key: String,
    pub name: String,
    pub total_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathSuggestion {
    pub path: String,
    pub total_seconds: f64,
}

pub trait RuleSvc: Send + Sync {
    /// Every rule, newest first.
    fn list(&self) -> Result<Vec<CategoryRuleView>, RuleError>;
    fn create(&self, draft: &RuleDraft) -> Result<RuleId, RuleError>;
    /// Replaces everything the rule says.
    fn update(&self, id: RuleId, draft: &RuleDraft) -> Result<(), RuleError>;
    fn delete(&self, id: RuleId) -> Result<(), RuleError>;
    /// What a rule written like this would catch, without saving it. The category does not
    /// matter, so none is asked for.
    fn preview(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> Result<RulePreview, RuleError>;
    /// The apps and sites in the history, longest-tracked first.
    fn source_suggestions(&self) -> Result<Vec<SourceSuggestion>, RuleError>;
    /// The pages tracked on `domain`, and the folders above them, longest-tracked first.
    fn path_suggestions(&self, domain: &str) -> Result<Vec<PathSuggestion>, RuleError>;
}

pub struct SqliteRules {
    db: Db,
}

impl SqliteRules {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

/// The columns to store for a rule filing under `category`.
fn values_of(category: CategoryId, parts: &(RuleKind, Option<PatternField>, String)) -> CategoryRuleValues<'_> {
    CategoryRuleValues {
        category_id: category.into(),
        kind: parts.0.as_str(),
        field: parts.1.map_or("", PatternField::as_str),
        value: &parts.2,
    }
}

impl RuleSvc for SqliteRules {
    fn list(&self) -> Result<Vec<CategoryRuleView>, RuleError> {
        let mut conn = self.db.read()?;
        let stats: HashMap<i64, (i64, f64)> = RulesReportRepo::get_rule_stats(&mut *conn)?
            .into_iter()
            .map(|row| (row.rule_id, (row.activity_count, row.total_seconds)))
            .collect();
        let mut views: Vec<CategoryRuleView> = db::category_rules::CategoryRulesRepo::get_list(&mut *conn)?
            .into_iter()
            .filter_map(|row| {
                let (activity_count, total_seconds) = stats.get(&row.id).copied().unwrap_or((0, 0.0));
                Some(CategoryRuleView {
                    id: RuleId::from(row.id),
                    kind: RuleKind::from_db(&row.kind)?,
                    field: PatternField::from_db(&row.field),
                    value: row.value,
                    category_id: CategoryId::from(row.category_id),
                    activity_count,
                    total_seconds,
                })
            })
            .collect();
        views.reverse();
        Ok(views)
    }

    fn create(&self, draft: &RuleDraft) -> Result<RuleId, RuleError> {
        let matcher = RuleMatcher::parse(draft.kind, draft.field, &draft.value)?;
        let parts = matcher.parts();
        let mut conn = self.db.write();
        conn.transaction(|conn| -> Result<RuleId, RuleError> {
            let row = db::category_rules::CategoryRulesRepo::insert(conn, &values_of(draft.category_id, &parts))?;
            rules_store::rematch_all(conn)?;
            Ok(RuleId::from(row.id))
        })
    }

    fn update(&self, id: RuleId, draft: &RuleDraft) -> Result<(), RuleError> {
        let matcher = RuleMatcher::parse(draft.kind, draft.field, &draft.value)?;
        let parts = matcher.parts();
        let mut conn = self.db.write();
        conn.transaction(|conn| -> Result<(), RuleError> {
            db::category_rules::CategoryRulesRepo::update(conn, id.into(), &values_of(draft.category_id, &parts))?
                .ok_or(RuleError::NotFound(id))?;
            rules_store::rematch_all(conn)?;
            Ok(())
        })
    }

    fn delete(&self, id: RuleId) -> Result<(), RuleError> {
        let mut conn = self.db.write();
        conn.transaction(|conn| -> Result<(), RuleError> {
            if !db::category_rules::CategoryRulesRepo::delete(conn, id.into())? {
                return Err(RuleError::NotFound(id));
            }
            rules_store::rematch_all(conn)?;
            Ok(())
        })
    }

    fn preview(&self, kind: RuleKind, field: Option<PatternField>, value: &str) -> Result<RulePreview, RuleError> {
        let matcher = RuleMatcher::parse(kind, field, value)?;
        let mut conn = self.db.read()?;
        let mut matched: Vec<(String, f64, bool)> = RulesReportRepo::get_activities_for_preview(&mut *conn)?
            .into_iter()
            .filter_map(|row| {
                let facts = ActivityFacts::new(
                    KeyKind::from_db(&row.source_kind),
                    &row.source_key,
                    &row.source_name,
                    &row.path,
                    &row.title,
                );
                matcher.matches(&facts).then(|| (sample_label(&row), row.total_seconds, row.pinned))
            })
            .collect();
        let activity_count = matched.len() as i64;
        let total_seconds = matched.iter().map(|(_, seconds, _)| seconds).sum();
        let pinned_count = matched.iter().filter(|(_, _, pinned)| *pinned).count() as i64;
        matched.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut samples: Vec<String> = Vec::new();
        for (label, _, _) in matched {
            if samples.len() == PREVIEW_SAMPLES {
                break;
            }
            if !samples.contains(&label) {
                samples.push(label);
            }
        }
        Ok(RulePreview {
            activity_count,
            total_seconds,
            pinned_count,
            samples,
        })
    }

    fn source_suggestions(&self) -> Result<Vec<SourceSuggestion>, RuleError> {
        let mut conn = self.db.read()?;
        Ok(RulesReportRepo::get_source_suggestions(&mut *conn)?
            .into_iter()
            .map(|row| SourceSuggestion {
                kind: KeyKind::from_db(&row.kind),
                key: row.key,
                name: row.name,
                total_seconds: row.total_seconds,
            })
            .collect())
    }

    fn path_suggestions(&self, domain: &str) -> Result<Vec<PathSuggestion>, RuleError> {
        let mut conn = self.db.read()?;
        let rows = RulesReportRepo::get_paths_of(&mut *conn, &domain.trim().to_lowercase())?;
        Ok(folder_totals(rows.into_iter().map(|row| (row.path, row.total_seconds))))
    }
}

/// What to call a matched activity in a preview: its title, else its address, else its source.
fn sample_label(row: &crate::reporting::rules::ActivityPreviewRow) -> String {
    if !row.title.is_empty() {
        return row.title.clone();
    }
    match KeyKind::from_db(&row.source_kind) {
        KeyKind::Domain => format!("{}{}", row.source_key, row.path),
        KeyKind::Exe => row.source_name.clone(),
    }
}

/// Adds each page's time to every folder above it (`/acme/api/issues?x=1` counts towards
/// `/acme`, `/acme/api` and `/acme/api/issues`), since a page rule covers everything below its
/// path. Longest-tracked first, capped.
fn folder_totals(pages: impl Iterator<Item = (String, f64)>) -> Vec<PathSuggestion> {
    let mut totals: HashMap<String, f64> = HashMap::new();
    for (path, seconds) in pages {
        let path = path.split(['?', '#']).next().unwrap_or_default().to_lowercase();
        let mut folder = String::new();
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            folder.push('/');
            folder.push_str(segment);
            *totals.entry(folder.clone()).or_default() += seconds;
        }
    }
    let mut suggestions: Vec<PathSuggestion> = totals
        .into_iter()
        .map(|(path, total_seconds)| PathSuggestion { path, total_seconds })
        .collect();
    suggestions.sort_by(|a, b| b.total_seconds.total_cmp(&a.total_seconds).then_with(|| a.path.cmp(&b.path)));
    suggestions.truncate(PATH_SUGGESTION_LIMIT);
    suggestions
}
