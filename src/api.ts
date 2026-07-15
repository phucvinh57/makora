import { invoke } from "@tauri-apps/api/core";
import type { Category, CategoryRule, KeyKind, SessionsPage } from "./types";

export function listSessions(
  page: number,
  pageSize: number,
  search?: string,
  category?: string,
): Promise<SessionsPage> {
  return invoke("list_sessions", { page, pageSize, search, category });
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

export function clearSessionCategory(sessionId: number): Promise<void> {
  return invoke("clear_session_category", { sessionId });
}

export function setSessionsCategory(
  sessionIds: number[],
  categoryId: number,
): Promise<void> {
  return invoke("set_sessions_category", { sessionIds, categoryId });
}

export function createCategory(
  name: string,
  color: string,
): Promise<Category> {
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
  categoryId: number,
): Promise<CategoryRule> {
  return invoke("create_category_rule", {
    keyKind,
    pattern,
    categoryId,
  });
}

export function deleteCategoryRule(id: number): Promise<void> {
  return invoke("delete_category_rule", { id });
}
