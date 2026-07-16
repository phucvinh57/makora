import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityStats,
  CategorizePreview,
  Category,
  CategoryKeyRef,
  CategoryRule,
  IgnoreKind,
  IgnoreRule,
  KeyKind,
  MatchKind,
  SessionsPage,
} from "./types";

export function listSessions(
  page: number,
  pageSize: number,
  search?: string,
  category?: string,
): Promise<SessionsPage> {
  return invoke("list_sessions", { page, pageSize, search, category });
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

export function setSessionCategory(
  sessionId: number,
  categoryId: number,
): Promise<void> {
  return invoke("set_session_category", { sessionId, categoryId });
}

export function setSessionRowCategory(
  sessionId: number,
  categoryId: number | null,
): Promise<void> {
  return invoke("set_session_row_category", { sessionId, categoryId });
}

export function previewSessionCategory(
  sessionId: number,
): Promise<CategorizePreview> {
  return invoke("preview_session_category", { sessionId });
}

export function setSessionsCategory(
  sessionIds: number[],
  categoryId: number,
): Promise<void> {
  return invoke("set_sessions_category", { sessionIds, categoryId });
}

export function listRecategorizationKeys(
  startDate: string | undefined,
  endDate: string | undefined,
  onlyUncategorized: boolean,
  protectManual: boolean,
): Promise<CategoryKeyRef[]> {
  return invoke("list_recategorization_keys", {
    startDate,
    endDate,
    onlyUncategorized,
    protectManual,
  });
}

export function recategorizeKeys(keys: CategoryKeyRef[]): Promise<number> {
  return invoke("recategorize_keys", { keys });
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
