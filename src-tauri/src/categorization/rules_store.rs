//! Keeping `rule_matches` current: the winning rule of every activity, computed in Rust by the
//! [`RuleSet`] and stored so that reads never evaluate a rule. Anything that changes the rules,
//! or the categories they file under, rewrites it all; a new activity is matched on its own.

use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use crate::db::models::{ActivityIdentityRow, CategoryRuleRow};
use crate::db::{self};
use crate::shared::ids::RuleId;
use crate::tracking::domain::source_key::KeyKind;

use super::domain::rules::{ActivityFacts, CategoryRule, PatternField, RuleKind, RuleMatcher, RuleSet};

/// A stored rule as a domain one. `None` for a row the current code cannot read, which is left
/// alone rather than failing every write that needs the rules.
pub fn rule_from_row(row: CategoryRuleRow) -> Option<CategoryRule> {
    let kind = RuleKind::from_db(&row.kind)?;
    let field = PatternField::from_db(&row.field);
    Some(CategoryRule {
        id: RuleId::from(row.id),
        matcher: RuleMatcher::parse(kind, field, &row.value).ok()?,
    })
}

pub fn facts_of(row: &ActivityIdentityRow) -> ActivityFacts {
    ActivityFacts::new(
        KeyKind::from_db(&row.source_kind),
        &row.source_key,
        &row.source_name,
        &row.path,
        &row.title,
    )
}

pub fn load_rules(conn: &mut SqliteConnection) -> QueryResult<RuleSet> {
    Ok(RuleSet::new(
        db::category_rules::CategoryRulesRepo::get_list(conn)?
            .into_iter()
            .filter_map(rule_from_row)
            .collect(),
    ))
}

/// Recomputes which rule wins for every activity.
pub fn rematch_all(conn: &mut SqliteConnection) -> QueryResult<()> {
    let rules = load_rules(conn)?;
    let matches: Vec<(i64, i64)> = if rules.is_empty() {
        Vec::new()
    } else {
        db::activities::ActivitiesRepo::get_identities(conn)?
            .iter()
            .filter_map(|row| {
                let rule = rules.resolve(&facts_of(row))?;
                Some((row.id, rule.id.into()))
            })
            .collect()
    };
    db::rule_matches::RuleMatchesRepo::replace_all(conn, &matches)
}

/// Matches an activity that was just created.
pub fn match_new(conn: &mut SqliteConnection, activity_id: i64) -> QueryResult<()> {
    let rules = load_rules(conn)?;
    if rules.is_empty() {
        return Ok(());
    }
    let Some(row) = db::activities::ActivitiesRepo::get_identity(conn, activity_id)? else {
        return Ok(());
    };
    if let Some(rule) = rules.resolve(&facts_of(&row)) {
        db::rule_matches::RuleMatchesRepo::set(conn, activity_id, rule.id.into())?;
    }
    Ok(())
}
