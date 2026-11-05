//! Loading and saving [`ActivityClassification`]s. A classification spans two tables — the pin
//! on `activities` and the row in `ai_labels` — and this is the only code that reads or writes
//! both together, always in batches: a fixed number of queries however many activities.

use std::collections::{HashMap, HashSet};

use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use crate::db;
use crate::shared::ids::{raw, ActivityId, CategoryId};

use super::domain::classification::{ActivityClassification, AiSuggestion};

/// The classification of each of `activity_ids` that exists, in the order given.
pub fn load_many(conn: &mut SqliteConnection, activity_ids: &[ActivityId]) -> QueryResult<Vec<ActivityClassification>> {
    let mut seen = HashSet::new();
    let unique: Vec<ActivityId> = activity_ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if unique.is_empty() {
        return Ok(Vec::new());
    }
    let raw_ids = raw(&unique);
    let pins: HashMap<i64, Option<i64>> = db::activities::ActivitiesRepo::get_pins_many(conn, &raw_ids)?
        .into_iter()
        .collect();
    let mut suggestions: HashMap<i64, AiSuggestion> = db::ai_labels::AiLabelsRepo::get_many(conn, &raw_ids)?
        .into_iter()
        .map(|label| {
            (
                label.activity_id,
                AiSuggestion {
                    category: label.category_id.into(),
                    rejected: label.rejected,
                },
            )
        })
        .collect();
    Ok(unique
        .into_iter()
        .filter_map(|id| {
            let pin = pins.get(&i64::from(id))?;
            Some(ActivityClassification::new(
                id,
                pin.map(CategoryId::from),
                suggestions.remove(&i64::from(id)),
            ))
        })
        .collect())
}

/// Writes what differs between `before` and `after` (the same activities, in the same order,
/// as returned by [`load_many`]): pins grouped per target category, AI labels in one statement
/// per kind of change.
pub fn save_changes(
    conn: &mut SqliteConnection,
    before: &[ActivityClassification],
    after: &[ActivityClassification],
) -> QueryResult<()> {
    let mut pins: HashMap<Option<CategoryId>, Vec<i64>> = HashMap::new();
    let mut upserts: Vec<(i64, i64)> = Vec::new();
    let mut rejects: Vec<i64> = Vec::new();
    let mut clears: Vec<i64> = Vec::new();

    for (old, new) in before.iter().zip(after) {
        let id = i64::from(new.activity_id());
        if old.pin() != new.pin() {
            pins.entry(new.pin()).or_default().push(id);
        }
        if old.suggestion() == new.suggestion() {
            continue;
        }
        match new.suggestion() {
            Some(suggestion) if !suggestion.rejected => upserts.push((id, suggestion.category.into())),
            Some(_) => rejects.push(id),
            None => clears.push(id),
        }
    }

    for (category, ids) in pins {
        db::activities::ActivitiesRepo::set_category_many(conn, &ids, category.map(i64::from))?;
    }
    db::ai_labels::AiLabelsRepo::upsert_many(conn, &upserts)?;
    if !rejects.is_empty() {
        db::ai_labels::AiLabelsRepo::reject_active_many(conn, &rejects)?;
    }
    if !clears.is_empty() {
        db::ai_labels::AiLabelsRepo::clear_active_many(conn, &clears)?;
    }
    Ok(())
}

/// Loads the classifications of `activity_ids`, lets `change` mutate them, and saves what it
/// changed. Returns the classifications as they were before and after, for callers that need
/// to tell what actually changed.
pub fn modify_many(
    conn: &mut SqliteConnection,
    activity_ids: &[ActivityId],
    change: impl FnOnce(&mut [ActivityClassification]),
) -> QueryResult<(Vec<ActivityClassification>, Vec<ActivityClassification>)> {
    let before = load_many(conn, activity_ids)?;
    let mut after = before.clone();
    change(&mut after);
    save_changes(conn, &before, &after)?;
    Ok((before, after))
}

/// Drops every pin, AI label and rule pointing at `category_ids` — called as those categories
/// are deleted, in the same transaction. Activities that lose a rule fall back to the next one
/// that matches them.
pub fn release_categories(conn: &mut SqliteConnection, category_ids: &[i64]) -> QueryResult<()> {
    db::activities::ActivitiesRepo::unpin_categories(conn, category_ids)?;
    db::ai_labels::AiLabelsRepo::delete_for_categories(conn, category_ids)?;
    db::category_rules::CategoryRulesRepo::delete_for_categories(conn, category_ids)?;
    super::rules_store::rematch_all(conn)
}

/// Drops the AI labels of activities that are about to be deleted. Their rule matches go with
/// them (the foreign key cascades).
pub fn discard_labels(conn: &mut SqliteConnection, activity_ids: &[i64]) -> QueryResult<()> {
    db::ai_labels::AiLabelsRepo::delete_for_activities(conn, activity_ids)
}

/// Clears every pin and every AI label. Returns how many were removed.
pub fn clear_all(conn: &mut SqliteConnection) -> QueryResult<usize> {
    let labels = db::ai_labels::AiLabelsRepo::delete_all(conn)?;
    let pins = db::activities::ActivitiesRepo::clear_all_pins(conn)?;
    Ok(labels + pins)
}
