import { invoke } from "@tauri-apps/api/core";
import type { Category, SessionsPage } from "./types";

export function listSessions(
  page: number,
  pageSize: number,
): Promise<SessionsPage> {
  return invoke("list_sessions", { page, pageSize });
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
