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

export function activityStats(
  start: string,
  end: string,
): Promise<ActivityStats> {
  return invoke("activity_stats", { start, end });
}

export function listCategories(): Promise<Category[]> {
  return invoke("list_categories");
}

export function setSessionRowCategory(
  sessionId: number,
  categoryId: number | null,
): Promise<void> {
  return invoke("set_session_row_category", { sessionId, categoryId });
}

export function setSessionsCategory(
  sessionIds: number[],
  categoryId: number,
): Promise<void> {
  return invoke("set_sessions_category", { sessionIds, categoryId });
}

export function recategorizeAll(): Promise<RecategorizeOutcome> {
  return invoke("recategorize_all");
}

export function createCategory(
  name: string,
  color: string,
  parentId: number | null,
): Promise<Category> {
  return invoke("create_category", { name, color, parentId });
}

export function updateCategory(
  id: number,
  name: string,
  color: string,
  parentId: number | null,
): Promise<void> {
  return invoke("update_category", { id, name, color, parentId });
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

export function searchWorkSessions(
  search: string | undefined,
  limit: number,
  offset: number,
): Promise<WorkSession[]> {
  return invoke("search_work_sessions", { search, limit, offset });
}

export function getWorkSession(id: number): Promise<WorkSession | null> {
  return invoke("get_work_session", { id });
}

export function updateWorkSession(update: WorkSessionUpdate): Promise<void> {
  return invoke("update_work_session", { update });
}

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
