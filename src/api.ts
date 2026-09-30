import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs } from "@tauri-apps/api/core";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as checkAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import type {
  ActivityStats,
  AiCategorizeOutcome,
  AiCategorizeStatus,
  AiReviewGroupPage,
  AiReviewPage,
  Category,
  CategorizationResetOutcome,
  FeedbackStats,
  ModelDownloadStatus,
  ReviewFilter,
  SessionsPage,
} from "./types";

/** Why a command failed, stable enough to branch on. */
export type ApiErrorCode =
  | "name_taken"
  | "parent_missing"
  | "cycle"
  | "not_found"
  | "already_running"
  | "invalid_input"
  | "internal";

/**
 * A failed command. `String(error)` gives just the message, so it can be shown as it is;
 * `code` says what went wrong.
 */
export class ApiError extends Error {
  readonly code: ApiErrorCode;

  constructor(code: ApiErrorCode, message: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
  }

  override toString(): string {
    return this.message;
  }
}

function isWireError(
  value: unknown,
): value is { code: ApiErrorCode; message: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { code?: unknown }).code === "string" &&
    typeof (value as { message?: unknown }).message === "string"
  );
}

/** Invokes a command, turning whatever it rejects with into an `ApiError`. */
async function call<T>(command: string, args?: InvokeArgs): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    // Tauri itself rejects with a bare string, e.g. for arguments it cannot read.
    throw isWireError(raw)
      ? new ApiError(raw.code, raw.message)
      : new ApiError("internal", String(raw));
  }
}

export function listSessions(
  page: number,
  pageSize: number,
  search?: string,
  category?: string,
  source?: "manual",
  start?: string,
  end?: string,
): Promise<SessionsPage> {
  return call("list_sessions", {
    page,
    pageSize,
    search,
    category,
    source,
    start,
    end,
  });
}

export function activityStats(
  start: string,
  end: string,
): Promise<ActivityStats> {
  return call("activity_stats", { start, end });
}

export function listCategories(): Promise<Category[]> {
  return call("list_categories");
}

export function setSessionRowCategory(
  sessionId: number,
  categoryId: number | null,
): Promise<void> {
  return call("set_session_row_category", { sessionId, categoryId });
}

export function setSessionsCategory(
  sessionIds: number[],
  categoryId: number,
): Promise<void> {
  return call("set_sessions_category", { sessionIds, categoryId });
}

export function resetCategorization(): Promise<CategorizationResetOutcome> {
  return call("reset_categorization");
}

export function createCategory(
  name: string,
  color: string,
  description: string | null,
  parentId: number | null,
): Promise<Category> {
  return call("create_category", { name, color, description, parentId });
}

export function updateCategory(
  id: number,
  name: string,
  color: string,
  description: string | null,
  parentId: number | null,
): Promise<void> {
  return call("update_category", { id, name, color, description, parentId });
}

export function deleteCategory(id: number): Promise<void> {
  return call("delete_category", { id });
}

export function isAutostartEnabled(): Promise<boolean> {
  return checkAutostartEnabled();
}

export function setAutostartEnabled(enabled: boolean): Promise<void> {
  return enabled ? enableAutostart() : disableAutostart();
}

export function cancelCategorize(): Promise<void> {
  return call("cancel_categorize");
}

export function categorizeStatus(): Promise<AiCategorizeStatus> {
  return call("categorize_status");
}

export function aiModelLoaded(): Promise<boolean> {
  return call("ai_model_loaded");
}

export function unloadAiModel(): Promise<void> {
  return call("unload_ai_model");
}

export function aiModelInstalled(model: string): Promise<boolean> {
  return call("ai_model_installed", { model });
}

export function listInstalledAiModels(): Promise<string[]> {
  return call("list_installed_ai_models");
}

/** Resolves `false` when the user stopped the download. */
export function downloadAiModel(model: string): Promise<boolean> {
  return call("download_ai_model", { model });
}

export function cancelModelDownload(): Promise<void> {
  return call("cancel_model_download");
}

export function modelDownloadStatus(): Promise<ModelDownloadStatus> {
  return call("model_download_status");
}

export function categorize(
  recheck = false,
  model?: string,
): Promise<AiCategorizeOutcome> {
  return call("categorize", { model, recheck });
}

export function listAiReview(
  page: number,
  pageSize: number,
  filter: ReviewFilter = {},
): Promise<AiReviewPage> {
  return call("list_ai_review", { page, pageSize, filter });
}

export function listAiReviewGroups(
  page: number,
  pageSize: number,
  filter: ReviewFilter = {},
): Promise<AiReviewGroupPage> {
  return call("list_ai_review_groups", { page, pageSize, filter });
}

export function acceptAiLabels(activityIds: number[]): Promise<void> {
  return call("accept_ai_labels", { activityIds });
}

/** Pins activities to a category; `null` rejects their AI label. */
export function setActivitiesCategory(
  activityIds: number[],
  categoryId: number | null,
): Promise<void> {
  return call("set_activities_category", { activityIds, categoryId });
}

export function aiFeedbackStats(): Promise<FeedbackStats> {
  return call("ai_feedback_stats");
}
