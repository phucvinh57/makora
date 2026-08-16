import { invoke } from "@tauri-apps/api/core";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as checkAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import type {
  ActivityStats,
  Category,
  CategoryRule,
  IgnoreKind,
  IgnoreRule,
  KeyKind,
  MatchKind,
  RecategorizeOutcome,
  SessionsPage,
  WorkSession,
  WorkSessionStats,
  WorkSessionUpdate,
} from "./types";

/** `start`/`end` are RFC 3339 timestamps bounding the activity shown. Rows
 * *overlapping* the window match, so an activity that began before the
 * window and ran into it is included. */
export function listSessions(
  page: number,
  pageSize: number,
  search?: string,
  category?: string,
  start?: string,
  end?: string,
): Promise<SessionsPage> {
  return invoke("list_sessions", {
    page,
    pageSize,
    search,
    category,
    start,
    end,
  });
}

/** `start`/`end` are RFC 3339 UTC timestamps; the range is half-open
 * (`start_time >= start && start_time < end`). */
export function activityStats(
  start: string,
  end: string,
): Promise<ActivityStats> {
  return invoke("activity_stats", { start, end });
}

export function listCategories(): Promise<Category[]> {
  return invoke("list_categories");
}

/** Categorizes exactly this one activity and nothing else. The only manual
 * categorization there is — to make a choice stick across an app's whole
 * history, add a rule for it on the Labelling page. `categoryId: null`
 * clears the pin, falling back to the rules. */
export function setSessionRowCategory(
  sessionId: number,
  categoryId: number | null,
): Promise<void> {
  return invoke("set_session_row_category", { sessionId, categoryId });
}

/** Categorizes exactly `sessionIds` and nothing else — the bulk form of
 * `setSessionRowCategory`. */
export function setSessionsCategory(
  sessionIds: number[],
  categoryId: number,
): Promise<void> {
  return invoke("set_sessions_category", { sessionIds, categoryId });
}

/** Clears every per-activity pin so the current rules decide all of history
 * again, already-categorized activities included. Rules are left alone.
 * Not undoable; confirm before calling. */
export function recategorizeAll(): Promise<RecategorizeOutcome> {
  return invoke("recategorize_all");
}

export function createCategory(name: string, color: string): Promise<Category> {
  return invoke("create_category", { name, color });
}

export function updateCategory(
  id: number,
  name: string,
  color: string,
): Promise<void> {
  return invoke("update_category", { id, name, color });
}

export function deleteCategory(id: number): Promise<void> {
  return invoke("delete_category", { id });
}

export function listCategoryRules(): Promise<CategoryRule[]> {
  return invoke("list_category_rules");
}

export function createCategoryRule(
  keyKind: KeyKind,
  pattern: string,
  matchKind: MatchKind,
  categoryId: number,
): Promise<CategoryRule> {
  return invoke("create_category_rule", {
    keyKind,
    pattern,
    matchKind,
    categoryId,
  });
}

export function deleteCategoryRule(id: number): Promise<void> {
  return invoke("delete_category_rule", { id });
}

export function listIgnoreRules(): Promise<IgnoreRule[]> {
  return invoke("list_ignore_rules");
}

export function createIgnoreRule(
  kind: IgnoreKind,
  pattern: string,
  matchKind: MatchKind,
): Promise<IgnoreRule> {
  return invoke("create_ignore_rule", { kind, pattern, matchKind });
}

export function deleteIgnoreRule(id: number): Promise<void> {
  return invoke("delete_ignore_rule", { id });
}

/** `startTime` (RFC 3339 UTC) omitted means "start now"; the end is derived
 * as start + target and reaching it is the auto-end. */
export function createWorkSession(
  name: string,
  note: string | null,
  targetSeconds: number,
  categoryIds: number[],
  startTime?: string,
): Promise<WorkSession> {
  return invoke("create_work_session", {
    name,
    note,
    targetSeconds,
    categoryIds,
    startTime,
  });
}

export function listWorkSessions(): Promise<WorkSession[]> {
  return invoke("list_work_sessions");
}

/** One page of work sessions, newest first — what the history page's focus
 * session picker scrolls through. */
export function searchWorkSessions(
  search: string | undefined,
  limit: number,
  offset: number,
): Promise<WorkSession[]> {
  return invoke("search_work_sessions", { search, limit, offset });
}

/** `null` when the session has since been deleted. */
export function getWorkSession(id: number): Promise<WorkSession | null> {
  return invoke("get_work_session", { id });
}

export function updateWorkSession(update: WorkSessionUpdate): Promise<void> {
  return invoke("update_work_session", { update });
}

/** Ends an in-progress work session now; past sessions are left untouched. */
export function stopWorkSession(id: number): Promise<void> {
  return invoke("stop_work_session", { id });
}

export function deleteWorkSession(id: number): Promise<void> {
  return invoke("delete_work_session", { id });
}

export function workSessionStats(id: number): Promise<WorkSessionStats> {
  return invoke("work_session_stats", { id });
}

export function isAutostartEnabled(): Promise<boolean> {
  return checkAutostartEnabled();
}

export function setAutostartEnabled(enabled: boolean): Promise<void> {
  return enabled ? enableAutostart() : disableAutostart();
}
