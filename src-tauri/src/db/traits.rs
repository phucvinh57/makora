//! The repository layer's public interface, one trait per table or read-model, gathered here so
//! the whole surface can be read in one place. Each trait is implemented for [`SqliteConnection`]
//! in the sibling module of the same name (`ActivitiesRepo` in `activities.rs`, and so on); that
//! module also re-exports its trait so callers keep writing `db::activities::ActivitiesRepo`.

use diesel::QueryResult;

use super::models::{
    ActivityRow, AiLabelRow, CategoryChanges, CategoryRow, NewActivityRow, NewCategoryRow, NewSessionRow,
    NewSourceRow, SessionRow, SourceRow,
};
use super::{history, review};

pub trait ActivitiesRepo {
    fn get(&mut self, id: i64) -> QueryResult<Option<ActivityRow>>;
    fn get_by_identity(&mut self, source_id: i64, path: &str, title: &str) -> QueryResult<Option<ActivityRow>>;
    fn insert(&mut self, new: &NewActivityRow<'_>) -> QueryResult<ActivityRow>;
    /// `(id, category_id)` for each of `ids` that exists; `None` means unpinned. One query.
    fn get_pins_many(&mut self, ids: &[i64]) -> QueryResult<Vec<(i64, Option<i64>)>>;
    /// Pins every activity in `ids` to `category_id`, or unpins them with `None`, in one query.
    fn set_category_many(&mut self, ids: &[i64], category_id: Option<i64>) -> QueryResult<()>;
    /// Unpins every activity currently pinned to one of `category_ids` — used when those
    /// categories are about to be deleted.
    fn unpin_categories(&mut self, category_ids: &[i64]) -> QueryResult<()>;
    /// Clears every manual pin. Returns how many were cleared.
    fn clear_all_pins(&mut self) -> QueryResult<usize>;
    /// Every activity among `ids` that is unpinned and no session references any more —
    /// candidates for deletion once their last session goes away.
    fn get_prunable_many(&mut self, ids: &[i64]) -> QueryResult<Vec<ActivityRow>>;
    fn delete_many(&mut self, ids: &[i64]) -> QueryResult<()>;
}

pub trait AiLabelsRepo {
    /// The labels among `activity_ids`, rejected ones included, in one query.
    fn get_many(&mut self, activity_ids: &[i64]) -> QueryResult<Vec<AiLabelRow>>;
    /// Sets the AI label for every `(activity_id, category_id, confidence)` in `labels`,
    /// replacing whatever was there (rejected or not), in one query. `activity_id` is the
    /// table's primary key, so this is insert-or-overwrite per row.
    fn upsert_many(&mut self, labels: &[(i64, i64, Option<f64>)]) -> QueryResult<()>;
    /// Removes the active (non-rejected) labels for `activity_ids`, leaving them with none.
    /// Returns how many were actually removed.
    fn clear_active_many(&mut self, activity_ids: &[i64]) -> QueryResult<usize>;
    /// Marks the active label(s) rejected instead of deleting them, so the model can be told
    /// "not this" next time.
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
    /// The subset of `ids` that exist, in one query.
    fn filter_existing(&mut self, ids: &[i64]) -> QueryResult<Vec<i64>>;
    /// `(id, parent_id)` for every category, to build a [`crate::taxonomy::domain::CategoryTree`] from.
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
    /// Reopens a session that was closed within the merge gap.
    fn reopen(&mut self, id: i64) -> QueryResult<()>;
    fn set_end(&mut self, id: i64, end_time: &str) -> QueryResult<()>;
    /// Moves a session onto a different activity — used when a refined title resolves to a
    /// different activity row than the one the session opened against.
    fn reassign_activity(&mut self, id: i64, activity_id: i64) -> QueryResult<()>;
    fn delete(&mut self, id: i64) -> QueryResult<()>;
    /// The activities behind `session_ids`. Ids with no matching session are silently skipped.
    fn get_activity_ids_for(&mut self, session_ids: &[i64]) -> QueryResult<Vec<i64>>;
    /// Activities behind a session that never closed — left dangling by a crash or a forced quit.
    fn get_open_activity_ids(&mut self) -> QueryResult<Vec<i64>>;
    fn delete_open(&mut self) -> QueryResult<()>;
}

pub trait SourcesRepo {
    fn get_by_key(&mut self, kind: &str, key: &str) -> QueryResult<Option<SourceRow>>;
    fn insert(&mut self, new: &NewSourceRow<'_>) -> QueryResult<SourceRow>;
    fn set_name(&mut self, id: i64, name: &str) -> QueryResult<()>;
    /// Deletes every source among `ids` that nothing in `activities` points to any more, in one
    /// query.
    fn delete_unused_many(&mut self, ids: &[i64]) -> QueryResult<()>;
}

pub trait HistoryRepo {
    fn get_session_count(&mut self, query: &history::SessionQuery) -> QueryResult<i64>;
    fn get_sessions(
        &mut self,
        limit: i64,
        offset: i64,
        query: &history::SessionQuery,
    ) -> QueryResult<Vec<history::SessionListRow>>;
    fn get_totals(&mut self, start: &str, end: &str) -> QueryResult<history::TotalsRow>;
    fn get_category_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<history::CategoryStatRow>>;
    fn get_app_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<history::AppStatRow>>;
    fn get_daily_stats(&mut self, start: &str, end: &str) -> QueryResult<Vec<history::DailyStatRow>>;
}

pub trait ReviewRepo {
    /// Activities for the LLM to label, most recently seen first. Normally those with neither a
    /// pin nor an AI label nor a rejected one; with `recheck`, those carrying an AI label the user
    /// has not confirmed.
    fn get_activities_to_label(&mut self, recheck: bool) -> QueryResult<Vec<review::ActivityToLabelRow>>;
    /// Every activity the user has pinned or rejected an AI label for, newest first.
    fn get_labeled_activities(&mut self) -> QueryResult<Vec<review::LabeledActivityRow>>;
    /// AI labels awaiting review, longest-tracked first so the ones that matter come up first.
    /// `Ok(None)` when the category scope matches nothing.
    #[allow(clippy::too_many_arguments)]
    fn get_review_page(
        &mut self,
        page: i64,
        page_size: i64,
        category: &review::CategoryScope,
        activity_ids: Option<&[i64]>,
        uncertain_only: bool,
        confident: f64,
        min_seconds: f64,
    ) -> QueryResult<Option<(Vec<review::ReviewRow>, i64)>>;
    /// The pending AI labels folded into one row per source and suggested category, longest-tracked
    /// first. `Ok(None)` when the category scope matches nothing.
    fn get_review_groups(
        &mut self,
        page: i64,
        page_size: i64,
        category: &review::CategoryScope,
        uncertain_only: bool,
        confident: f64,
        min_seconds: f64,
    ) -> QueryResult<Option<(Vec<review::ReviewGroupRow>, i64)>>;
    fn get_feedback_counts(&mut self) -> QueryResult<review::FeedbackCountsRow>;
    /// Time, not rows: a few long activities matter more than many short ones.
    fn get_feedback_seconds(&mut self) -> QueryResult<review::FeedbackSecondsRow>;
}
