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
    let mut filed: Vec<i64> = Vec::new();

    for (old, new) in before.iter().zip(after) {
        let id = i64::from(new.activity_id());
        let now_filed = new.pin().is_some() || new.active_suggestion().is_some();
        if now_filed && (old.pin() != new.pin() || old.active_suggestion() != new.active_suggestion()) {
            filed.push(id);
        }
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
    // An activity filed by the user or the model is no longer one the model found no category for.
    if !filed.is_empty() {
        db::ai_unlabeled::AiUnlabeledRepo::delete_many(conn, &filed)?;
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

/// Moves every pin, AI label and rule from `from` to `into` — called as `from` is merged into
/// `into`, in the same transaction. Rules are matched again, since which one wins may change.
pub fn move_category(conn: &mut SqliteConnection, from: i64, into: i64) -> QueryResult<()> {
    db::activities::ActivitiesRepo::move_pins(conn, from, into)?;
    db::ai_labels::AiLabelsRepo::move_labels(conn, from, into)?;
    db::category_rules::CategoryRulesRepo::move_rules(conn, from, into)?;
    super::rules_store::rematch_all(conn)
}

/// Drops the AI labels of activities that are about to be deleted. Their rule matches go with
/// them (the foreign key cascades).
pub fn discard_labels(conn: &mut SqliteConnection, activity_ids: &[i64]) -> QueryResult<()> {
    db::ai_labels::AiLabelsRepo::delete_for_activities(conn, activity_ids)
}

/// Records that the model found no category for `activity_ids`, those of them still neither
/// pinned nor labeled.
pub fn mark_unlabeled(conn: &mut SqliteConnection, activity_ids: &[ActivityId], model: &str, at: &str) -> QueryResult<()> {
    let unfiled: Vec<i64> = load_many(conn, activity_ids)?
        .iter()
        .filter(|classification| classification.pin().is_none() && classification.active_suggestion().is_none())
        .map(|classification| classification.activity_id().into())
        .collect();
    if unfiled.is_empty() {
        return Ok(());
    }
    db::ai_unlabeled::AiUnlabeledRepo::upsert_many(conn, &unfiled, model, at)
}

/// Clears every pin, every AI label and every "no category" answer. Returns how many pins and
/// labels were removed.
pub fn clear_all(conn: &mut SqliteConnection) -> QueryResult<usize> {
    db::ai_unlabeled::AiUnlabeledRepo::delete_all(conn)?;
    let labels = db::ai_labels::AiLabelsRepo::delete_all(conn)?;
    let pins = db::activities::ActivitiesRepo::clear_all_pins(conn)?;
    Ok(labels + pins)
}

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;

    use super::*;
    use crate::db::ai_unlabeled::AiUnlabeledRepo;

    const AT: &str = "2026-03-01T10:00:00+00:00";

    fn ids(list: &[i64]) -> Vec<ActivityId> {
        list.iter().copied().map(ActivityId).collect()
    }

    /// Activity 1 is pinned, 2 has an AI label, 3 and 4 have neither.
    fn conn() -> SqliteConnection {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#000000');
             INSERT INTO sources (id, kind, key, name) VALUES (1, 'exe', 'code', 'Code');
             INSERT INTO activities (id, source_id, path, title, category_id) VALUES
               (1, 1, '', 'a', 1), (2, 1, '', 'b', NULL), (3, 1, '', 'c', NULL), (4, 1, '', 'd', NULL);
             INSERT INTO ai_labels (activity_id, category_id) VALUES (2, 1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn only_unfiled_activities_are_marked_unlabeled() {
        let mut conn = conn();
        mark_unlabeled(&mut conn, &ids(&[1, 2, 3, 4]), "m", AT).unwrap();
        assert_eq!(conn.get_ids().unwrap(), [3, 4]);
    }

    #[test]
    fn a_pin_or_a_label_clears_the_mark_and_a_reset_clears_all() {
        let mut conn = conn();
        mark_unlabeled(&mut conn, &ids(&[3, 4]), "m", AT).unwrap();
        modify_many(&mut conn, &ids(&[3, 4]), |classifications| {
            classifications[0].pin_to(CategoryId(1));
            classifications[1].suggest(CategoryId(1));
        })
        .unwrap();
        assert!(conn.get_ids().unwrap().is_empty());

        conn.batch_execute("INSERT INTO activities (id, source_id, path, title) VALUES (5, 1, '', 'e');")
            .unwrap();
        mark_unlabeled(&mut conn, &ids(&[5]), "m", AT).unwrap();
        clear_all(&mut conn).unwrap();
        assert!(conn.get_ids().unwrap().is_empty());
    }
}
