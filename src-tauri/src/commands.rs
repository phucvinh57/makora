use std::sync::Arc;

use crate::core::{
    ActivityStats, CategorizePreview, Category, CategoryFilter, CategoryKeyRef, CategoryRule,
    CategoryStore, HistoryReader, IgnoreRule, IgnoreStore, QueryWhitelistEntry,
    QueryWhitelistStore, RecategorizeFilter, SessionFilter, SessionRow, WorkSession,
    WorkSessionStats, WorkSessionStore, WorkSessionUpdate,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SessionsPage {
    pub rows: Vec<SessionRow>,
    pub total: i64,
}

/// One page of activity rows (app focus and browser-tab rows alike) matching
/// `search`/`category`, newest first.
///
/// `category` selects the category filter: omitted or `null` shows every
/// row, `"uncategorized"` shows rows with no resolved category, and any
/// other value is parsed as a category id.
#[tauri::command]
pub fn list_sessions(
    reader: State<HistoryReader>,
    page: i64,
    page_size: i64,
    search: Option<String>,
    category: Option<String>,
) -> Result<SessionsPage, String> {
    let page_size = page_size.max(1);
    let offset = page.max(0) * page_size;
    let filter = SessionFilter {
        search,
        category: parse_category_filter(category.as_deref())?,
    };
    let rows = reader
        .list_sessions(page_size, offset, &filter)
        .map_err(|e| e.to_string())?;
    let total = reader.count_sessions(&filter).map_err(|e| e.to_string())?;
    Ok(SessionsPage { rows, total })
}

/// Aggregate activity totals between `start` and `end` (RFC 3339 UTC
/// bounds, half-open) — a total, a per-category breakdown, a top-apps/sites
/// breakdown, and a per-day breakdown in the machine's local time zone, for
/// the Statistics page.
#[tauri::command]
pub fn activity_stats(
    reader: State<HistoryReader>,
    start: String,
    end: String,
) -> Result<ActivityStats, String> {
    reader
        .activity_stats(&start, &end)
        .map_err(|e| e.to_string())
}

fn parse_category_filter(category: Option<&str>) -> Result<CategoryFilter, String> {
    match category {
        None => Ok(CategoryFilter::All),
        Some("uncategorized") => Ok(CategoryFilter::Uncategorized),
        Some(id) => id
            .parse::<i64>()
            .map(CategoryFilter::Category)
            .map_err(|_| format!("invalid category filter: {id}")),
    }
}

/// All categories (built-in defaults plus any the user has added), for
/// populating a "change category" picker in the UI.
#[tauri::command]
pub fn list_categories(categories: State<CategoryStore>) -> Result<Vec<Category>, String> {
    categories.list_categories().map_err(|e| e.to_string())
}

/// Manually assigns a category to a session — and to every other session
/// sharing the same app/site, since that's what "always put GitHub under
/// Development" actually expects. For categorizing just one activity, see
/// `set_session_row_category`.
#[tauri::command]
pub fn set_session_category(
    categories: State<CategoryStore>,
    session_id: i64,
    category_id: i64,
) -> Result<(), String> {
    categories
        .set_override(session_id, category_id)
        .map_err(|e| e.to_string())
}

/// Assigns a category to a single activity only, leaving every other
/// session sharing its app/site key alone. `category_id: null` clears the
/// pin, falling back to any key-wide override, then rules, then
/// "Uncategorized".
#[tauri::command]
pub fn set_session_row_category(
    categories: State<CategoryStore>,
    session_id: i64,
    category_id: Option<i64>,
) -> Result<(), String> {
    categories
        .set_row_category(session_id, category_id)
        .map_err(|e| e.to_string())
}

/// How many sessions share `session_id`'s app/site key, i.e. how many rows
/// `set_session_category` would actually recategorize — for the frontend to
/// show a "this affects N other activities" confirmation before committing.
#[tauri::command]
pub fn preview_session_category(
    categories: State<CategoryStore>,
    session_id: i64,
) -> Result<CategorizePreview, String> {
    categories
        .preview_category_change(session_id)
        .map_err(|e| e.to_string())
}

/// Manually assigns a category to many sessions at once (and to every other
/// session sharing each one's app/site key) — for batch-categorizing a
/// filtered set of history rows in one action.
#[tauri::command]
pub fn set_sessions_category(
    categories: State<CategoryStore>,
    session_ids: Vec<i64>,
    category_id: i64,
) -> Result<(), String> {
    categories
        .set_overrides(&session_ids, category_id)
        .map_err(|e| e.to_string())
}

/// Distinct app/site keys eligible for a bulk recategorization pass — every
/// key with at least one session inside `start_date`..=`end_date` (plain
/// `YYYY-MM-DD`, either bound optional), optionally narrowed to keys that
/// currently resolve to no category at all (`only_uncategorized`), and
/// optionally excluding any key that already has a manual override
/// (`protect_manual`). The frontend drives the actual write in chunks over
/// this list via `recategorize_keys` so it can show progress.
#[tauri::command]
pub fn list_recategorization_keys(
    categories: State<CategoryStore>,
    start_date: Option<String>,
    end_date: Option<String>,
    only_uncategorized: bool,
    protect_manual: bool,
) -> Result<Vec<CategoryKeyRef>, String> {
    let filter = RecategorizeFilter {
        start_date,
        end_date,
        only_uncategorized,
        protect_manual,
    };
    categories
        .list_recategorization_keys(&filter)
        .map_err(|e| e.to_string())
}

/// Re-evaluates a batch of app/site keys (from `list_recategorization_keys`)
/// against the current rule set by clearing any manual override on them, so
/// the live rule (or "Uncategorized") resolution takes back over instead of
/// the frozen manual choice. Returns how many keys in this batch actually
/// had an override cleared.
#[tauri::command]
pub fn recategorize_keys(
    categories: State<CategoryStore>,
    keys: Vec<CategoryKeyRef>,
) -> Result<i64, String> {
    categories
        .recategorize_keys(&keys)
        .map_err(|e| e.to_string())
}

/// Creates a new user-defined category.
#[tauri::command]
pub fn create_category(
    categories: State<CategoryStore>,
    name: String,
    color: String,
) -> Result<Category, String> {
    categories
        .create_category(&name, &color)
        .map_err(|e| e.to_string())
}

/// Renames a category or changes its color.
#[tauri::command]
pub fn update_category(
    categories: State<CategoryStore>,
    id: i64,
    name: String,
    color: String,
) -> Result<(), String> {
    categories
        .update_category(id, &name, &color)
        .map_err(|e| e.to_string())
}

/// Deletes a category along with any rules/overrides that reference it.
#[tauri::command]
pub fn delete_category(categories: State<CategoryStore>, id: i64) -> Result<(), String> {
    categories.delete_category(id).map_err(|e| e.to_string())
}

/// All built-in categorization rules (exe name / URL domain / URL path
/// patterns), for the category management page. Conflicts between rules in
/// different categories resolve purely by specificity — see
/// `core::category::rule_join_sql` — so rules have no explicit ordering.
#[tauri::command]
pub fn list_category_rules(categories: State<CategoryStore>) -> Result<Vec<CategoryRule>, String> {
    categories.list_rules().map_err(|e| e.to_string())
}

/// Adds a new rule matching an exe file name, URL domain, or URL path to a
/// category. `key_kind` is `"exe"`, `"domain"`, or `"url"`; `match_kind` is
/// `"exact"`, `"prefix"`, `"substring"`, or `"regex"` — see
/// `core::category::KeyKind`/`MatchKind`.
#[tauri::command]
pub fn create_category_rule(
    categories: State<CategoryStore>,
    key_kind: String,
    pattern: String,
    match_kind: String,
    category_id: i64,
) -> Result<CategoryRule, String> {
    categories
        .create_rule(&key_kind, &pattern, &match_kind, category_id)
        .map_err(|e| e.to_string())
}

/// Removes a categorization rule.
#[tauri::command]
pub fn delete_category_rule(categories: State<CategoryStore>, id: i64) -> Result<(), String> {
    categories.delete_rule(id).map_err(|e| e.to_string())
}

/// All apps/sites excluded from tracking entirely, for the settings page.
#[tauri::command]
pub fn list_ignore_rules(ignore: State<Arc<IgnoreStore>>) -> Result<Vec<IgnoreRule>, String> {
    ignore.list_rules().map_err(|e| e.to_string())
}

/// Adds an app or site to the ignore list. `kind` is `"app"` (matched
/// against the exe file name) or `"site"` (matched against the URL domain);
/// `match_kind` is `"exact"`, `"substring"`, or `"regex"` — see
/// `core::category::MatchKind`.
#[tauri::command]
pub fn create_ignore_rule(
    ignore: State<Arc<IgnoreStore>>,
    kind: String,
    pattern: String,
    match_kind: String,
) -> Result<IgnoreRule, String> {
    ignore
        .create_rule(&kind, &pattern, &match_kind)
        .map_err(|e| e.to_string())
}

/// Removes an app/site from the ignore list.
#[tauri::command]
pub fn delete_ignore_rule(ignore: State<Arc<IgnoreStore>>, id: i64) -> Result<(), String> {
    ignore.delete_rule(id).map_err(|e| e.to_string())
}

/// Per-domain query-string params kept on stored browser-tab URLs — every
/// other param is stripped (see `ingest::normalize_url`). For the settings
/// page.
#[tauri::command]
pub fn list_query_whitelist(
    whitelist: State<Arc<QueryWhitelistStore>>,
) -> Result<Vec<QueryWhitelistEntry>, String> {
    whitelist.list_entries().map_err(|e| e.to_string())
}

/// Whitelists one query param for one domain, e.g. `youtube.com` + `v` so
/// different videos become different sessions.
#[tauri::command]
pub fn create_query_whitelist_entry(
    whitelist: State<Arc<QueryWhitelistStore>>,
    domain: String,
    param: String,
) -> Result<QueryWhitelistEntry, String> {
    whitelist
        .create_entry(&domain, &param)
        .map_err(|e| e.to_string())
}

/// Removes a domain/param entry from the whitelist.
#[tauri::command]
pub fn delete_query_whitelist_entry(
    whitelist: State<Arc<QueryWhitelistStore>>,
    id: i64,
) -> Result<(), String> {
    whitelist.delete_entry(id).map_err(|e| e.to_string())
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| format!("invalid timestamp: {value}"))
}

fn validate_work_session(name: &str, target_seconds: i64) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("work session name must not be empty".to_string());
    }
    if target_seconds <= 0 {
        return Err("target duration must be positive".to_string());
    }
    Ok(())
}

/// Creates a planned focus block. `start_time` (RFC 3339) is optional —
/// omitted means "start now". The end is derived as start + target; there is
/// no timer anywhere, reaching that instant *is* the auto-end.
#[tauri::command]
pub fn create_work_session(
    work_sessions: State<WorkSessionStore>,
    name: String,
    note: Option<String>,
    target_seconds: i64,
    start_time: Option<String>,
    category_ids: Vec<i64>,
) -> Result<WorkSession, String> {
    validate_work_session(&name, target_seconds)?;
    let start = start_time.as_deref().map(parse_rfc3339).transpose()?;
    work_sessions
        .create(name.trim(), note.as_deref(), target_seconds, start, &category_ids)
        .map_err(|e| e.to_string())
}

/// All work sessions, newest first.
#[tauri::command]
pub fn list_work_sessions(
    work_sessions: State<WorkSessionStore>,
) -> Result<Vec<WorkSession>, String> {
    work_sessions.list().map_err(|e| e.to_string())
}

/// Rewrites every field of a work session, including its time window —
/// editing is unrestricted by design, the user owns their own plan.
#[tauri::command]
pub fn update_work_session(
    work_sessions: State<WorkSessionStore>,
    update: WorkSessionUpdate,
) -> Result<(), String> {
    validate_work_session(&update.name, update.target_seconds)?;
    let start = parse_rfc3339(&update.start_time)?;
    let end = parse_rfc3339(&update.end_time)?;
    if end <= start {
        return Err("end time must be after start time".to_string());
    }
    let update = WorkSessionUpdate {
        name: update.name.trim().to_string(),
        start_time: start.to_rfc3339(),
        end_time: end.to_rfc3339(),
        ..update
    };
    work_sessions.update(&update).map_err(|e| e.to_string())
}

/// Ends an in-progress work session now; a session whose window has already
/// passed is left untouched.
#[tauri::command]
pub fn stop_work_session(work_sessions: State<WorkSessionStore>, id: i64) -> Result<(), String> {
    work_sessions.stop(id).map_err(|e| e.to_string())
}

/// Deletes a work session and its target-category links. Tracked history is
/// untouched — a work session only ever reads it.
#[tauri::command]
pub fn delete_work_session(work_sessions: State<WorkSessionStore>, id: i64) -> Result<(), String> {
    work_sessions.delete(id).map_err(|e| e.to_string())
}

/// Evaluates the tracked history overlapping a work session's window against
/// its target categories: on/off-task totals, focus percentage, switch
/// count, per-app breakdown, and a timeline of focus/distraction/idle
/// blocks. For an in-progress session the window is clamped to now, so this
/// doubles as the live view.
#[tauri::command]
pub fn work_session_stats(
    work_sessions: State<WorkSessionStore>,
    id: i64,
) -> Result<WorkSessionStats, String> {
    work_sessions.stats(id).map_err(|e| e.to_string())
}
