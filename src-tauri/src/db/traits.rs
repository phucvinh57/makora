use diesel::QueryResult;

use super::models::{
    ActivityIdentityRow, ActivityRow, AiLabelRow, CategorizedEmbeddingRow, CategoryChanges, CategoryRow,
    CategoryRuleRow, CategoryRuleValues, NewActivityRow, NewCategoryRow, NewSessionRow, NewSourceRow, SessionRow,
    SourceRow,
};

pub trait ActivitiesRepo {
    fn get(&mut self, id: i64) -> QueryResult<Option<ActivityRow>>;
    fn get_by_identity(&mut self, source_id: i64, path: &str, title: &str) -> QueryResult<Option<ActivityRow>>;
    fn insert(&mut self, new: &NewActivityRow<'_>) -> QueryResult<ActivityRow>;
    /// `(id, category_id)` for each of `ids` that exists; `None` means unpinned. One query.
    fn get_pins_many(&mut self, ids: &[i64]) -> QueryResult<Vec<(i64, Option<i64>)>>;
    fn set_category_many(&mut self, ids: &[i64], category_id: Option<i64>) -> QueryResult<()>;
    fn unpin_categories(&mut self, category_ids: &[i64]) -> QueryResult<()>;
    fn clear_all_pins(&mut self) -> QueryResult<usize>;
    /// Every activity among `ids` that is unpinned and no session references any more —
    /// candidates for deletion once their last session goes away.
    fn get_prunable_many(&mut self, ids: &[i64]) -> QueryResult<Vec<ActivityRow>>;
    fn delete_many(&mut self, ids: &[i64]) -> QueryResult<()>;
    fn get_identities(&mut self) -> QueryResult<Vec<ActivityIdentityRow>>;
    fn get_identity(&mut self, id: i64) -> QueryResult<Option<ActivityIdentityRow>>;
}

pub trait AiLabelsRepo {
    /// The labels among `activity_ids`, rejected ones included, in one query.
    fn get_many(&mut self, activity_ids: &[i64]) -> QueryResult<Vec<AiLabelRow>>;
    /// Sets the label of every `(activity_id, category_id)`, replacing whatever was there, a
    /// rejection included.
    fn upsert_many(&mut self, labels: &[(i64, i64)]) -> QueryResult<()>;
    /// Removes the labels that are not rejected. Returns how many were removed.
    fn clear_active_many(&mut self, activity_ids: &[i64]) -> QueryResult<usize>;
    /// Marks the labels rejected instead of deleting them, so "not this" is remembered.
    fn reject_active_many(&mut self, activity_ids: &[i64]) -> QueryResult<()>;
    fn delete_for_activities(&mut self, activity_ids: &[i64]) -> QueryResult<()>;
    fn delete_for_categories(&mut self, category_ids: &[i64]) -> QueryResult<()>;
    /// Returns how many labels were removed.
    fn delete_all(&mut self) -> QueryResult<usize>;
}

pub trait CategoriesRepo {
    /// Every category, flat. Siblings come out in the order they should be shown under their
    /// parent; the caller assembles the tree from `parent_id`.
    fn get_list(&mut self) -> QueryResult<Vec<CategoryRow>>;
    fn get(&mut self, id: i64) -> QueryResult<Option<CategoryRow>>;
    fn filter_existing(&mut self, ids: &[i64]) -> QueryResult<Vec<i64>>;
    fn get_parent_pairs(&mut self) -> QueryResult<Vec<(i64, Option<i64>)>>;
    fn insert(&mut self, new: &NewCategoryRow<'_>) -> QueryResult<CategoryRow>;
    /// Replaces every editable field; `Ok(None)` when `id` does not exist.
    fn update(&mut self, id: i64, changes: &CategoryChanges<'_>) -> QueryResult<Option<CategoryRow>>;
    /// Deletes every category in `ids`, in the order given — callers pass deepest-first so a
    /// child never outlives the parent it was deleted alongside.
    fn delete_many(&mut self, ids: &[i64]) -> QueryResult<()>;
}

pub trait SessionsRepo {
    fn get(&mut self, id: i64) -> QueryResult<Option<SessionRow>>;
    /// The most recently opened session, whether or not it has closed yet.
    fn get_last(&mut self) -> QueryResult<Option<SessionRow>>;
    fn insert(&mut self, new: &NewSessionRow<'_>) -> QueryResult<SessionRow>;
    fn reopen(&mut self, id: i64, at: &str) -> QueryResult<()>;
    /// Records that the session, if still open, was running at `at`.
    fn touch(&mut self, id: i64, at: &str) -> QueryResult<()>;
    fn set_end(&mut self, id: i64, end_time: &str) -> QueryResult<()>;
    fn reassign_activity(&mut self, id: i64, activity_id: i64) -> QueryResult<()>;
    fn delete(&mut self, id: i64) -> QueryResult<()>;
    /// The activities behind `session_ids`. Ids with no matching session are silently skipped.
    fn get_activity_ids_for(&mut self, session_ids: &[i64]) -> QueryResult<Vec<i64>>;
    /// Sessions that never closed — left dangling by a crash or a forced quit.
    fn get_open(&mut self) -> QueryResult<Vec<SessionRow>>;
}

pub trait SourcesRepo {
    fn get_by_key(&mut self, kind: &str, key: &str) -> QueryResult<Option<SourceRow>>;
    fn insert(&mut self, new: &NewSourceRow<'_>) -> QueryResult<SourceRow>;
    fn set_name(&mut self, id: i64, name: &str) -> QueryResult<()>;
    fn delete_unused_many(&mut self, ids: &[i64]) -> QueryResult<()>;
}

pub trait CategoryRulesRepo {
    fn get_list(&mut self) -> QueryResult<Vec<CategoryRuleRow>>;
    fn insert(&mut self, new: &CategoryRuleValues<'_>) -> QueryResult<CategoryRuleRow>;
    /// Replaces every editable field; `Ok(None)` when `id` does not exist.
    fn update(&mut self, id: i64, values: &CategoryRuleValues<'_>) -> QueryResult<Option<CategoryRuleRow>>;
    /// Returns whether a rule was deleted. Its matches go with it.
    fn delete(&mut self, id: i64) -> QueryResult<bool>;
    fn delete_for_categories(&mut self, category_ids: &[i64]) -> QueryResult<()>;
}

pub trait RuleMatchesRepo {
    fn replace_all(&mut self, matches: &[(i64, i64)]) -> QueryResult<()>;
    fn set(&mut self, activity_id: i64, rule_id: i64) -> QueryResult<()>;
}

pub trait ActivityEmbeddingsRepo {
    /// Up to `limit` activities with no vector from `model` — none at all, or one from another
    /// model — the most recently created first.
    fn get_missing(&mut self, model: &str, limit: i64) -> QueryResult<Vec<ActivityIdentityRow>>;
    /// Sets the vector of every `(activity_id, vector)`, replacing whatever was there. An
    /// activity deleted in the meantime is skipped.
    fn upsert_many(&mut self, model: &str, vectors: &[(i64, Vec<u8>)]) -> QueryResult<()>;
    /// The vectors from `model` of those among `activity_ids` that have one.
    fn get_many(&mut self, model: &str, activity_ids: &[i64]) -> QueryResult<Vec<(i64, Vec<u8>)>>;
    /// Every activity the user has filed, by pin or by rule, that has a vector from `model`.
    fn get_categorized(&mut self, model: &str) -> QueryResult<Vec<CategorizedEmbeddingRow>>;
}
