use std::sync::Arc;

use crate::core::{
    ActivityStats, CategorizePreview, Category, CategoryFilter, CategoryRule, CategoryStore,
    HistoryReader, IgnoreRule, IgnoreStore, QueryWhitelistEntry, QueryWhitelistStore,
    RecategorizeOutcome, SessionFilter, SessionRow, WorkSession,
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
/// `search`/`category`/the `start`..`end` window, newest first.
///
/// `category` selects the category filter: omitted or `null` shows every
/// row, `"uncategorized"` shows rows with no resolved category, and any
/// other value is parsed as a category id.
///
/// `start`/`end` are optional RFC 3339 timestamps; rows *overlapping* that
/// window match (see `SessionFilter`), so scoping history to a focus
/// session's window shows the activity that ran through it, not only the
/// activity that happened to begin inside it.
#[tauri::command]
pub fn list_sessions(
    reader: State<HistoryReader>,
    page: i64,
    page_size: i64,
    search: Option<String>,
    category: Option<String>,
    start: Option<String>,
    end: Option<String>,
) -> Result<SessionsPage, String> {
    let page_size = page_size.max(1);
    let offset = page.max(0) * page_size;
    let start = start.as_deref().map(parse_rfc3339).transpose()?;
    let end = end.as_deref().map(parse_rfc3339).transpose()?;
    let filter = SessionFilter {
        search,
        category: parse_category_filter(category.as_deref())?,
        // Normalized to UTC here rather than passed through verbatim: the
        // stored `start_time`/`end_time` are UTC RFC 3339 and the SQL
        // compares them as plain strings, so an offset timestamp from the
        // frontend would otherwise compare wrong.
        start: start.map(|t| t.to_rfc3339()),
        end: end.map(|t| t.to_rfc3339()),
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

/// Manually assigns a category to many sessions at once — for
/// batch-categorizing a filtered set of history rows in one action. Pins
/// exactly the given sessions, leaving every other session sharing their
/// app/site keys alone, since a selection made under a search/time filter
/// means those rows and not "this app, forever". `apply_to_all` opts into
/// the wider behavior instead: every past and future session sharing a
/// selected row's key gets the category too (see
/// `CategoryStore::set_overrides`).
#[tauri::command]
pub fn set_sessions_category(
    categories: State<CategoryStore>,
    session_ids: Vec<i64>,
    category_id: i64,
    apply_to_all: bool,
) -> Result<(), String> {
    if apply_to_all {
        categories.set_overrides(&session_ids, category_id)
    } else {
        categories.set_row_categories(&session_ids, Some(category_id))
    }
    .map_err(|e| e.to_string())
}

/// Re-categorizes all of history from scratch: erases every manual
/// categorization in the database — per-session pins, key-wide overrides,
/// and the rules those overrides minted — so the current rule set (or
/// "Uncategorized") decides every activity again, already-categorized ones
/// included. Returns what it erased, for the UI to report. Destructive and
/// not undoable; the frontend confirms first.
#[tauri::command]
pub fn recategorize_all(categories: State<CategoryStore>) -> Result<RecategorizeOutcome, String> {
    categories.recategorize_all().map_err(|e| e.to_string())
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

/// One page of work sessions, newest first, optionally narrowed to those
/// whose name or note contains `search`. Backs the history page's "filter by
/// focus session" picker, which scrolls through them a page at a time rather
/// than loading every session the user has ever run.
#[tauri::command]
pub fn search_work_sessions(
    work_sessions: State<WorkSessionStore>,
    search: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<Vec<WorkSession>, String> {
    work_sessions
        .search(search.as_deref(), limit, offset)
        .map_err(|e| e.to_string())
}

/// One work session by id, or `null` if it has been deleted — for labeling a
/// session-scoped history filter without fetching the whole list.
#[tauri::command]
pub fn get_work_session(
    work_sessions: State<WorkSessionStore>,
    id: i64,
) -> Result<Option<WorkSession>, String> {
    work_sessions.get(id).map_err(|e| e.to_string())
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
