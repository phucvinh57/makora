import { invoke } from "@tauri-apps/api/core";
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

export function listSessions(
  page: number,
  pageSize: number,
  search?: string,
  category?: string,
  source?: "manual",
  start?: string,
  end?: string,
): Promise<SessionsPage> {
  return invoke("list_sessions", {
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

export function resetCategorization(): Promise<CategorizationResetOutcome> {
  return invoke("reset_categorization");
}

export function createCategory(
  name: string,
  color: string,
  description: string | null,
  parentId: number | null,
): Promise<Category> {
  return invoke("create_category", { name, color, description, parentId });
}

export function updateCategory(
  id: number,
  name: string,
  color: string,
  description: string | null,
  parentId: number | null,
): Promise<void> {
  return invoke("update_category", { id, name, color, description, parentId });
}

export function deleteCategory(id: number): Promise<void> {
  return invoke("delete_category", { id });
}

export function isAutostartEnabled(): Promise<boolean> {
  return checkAutostartEnabled();
}

export function setAutostartEnabled(enabled: boolean): Promise<void> {
  return enabled ? enableAutostart() : disableAutostart();
}

export function cancelCategorize(): Promise<void> {
  return invoke("cancel_categorize");
}

export function categorizeStatus(): Promise<AiCategorizeStatus> {
  return invoke("categorize_status");
}

export function aiModelLoaded(): Promise<boolean> {
  return invoke("ai_model_loaded");
}

export function unloadAiModel(): Promise<void> {
  return invoke("unload_ai_model");
}

export function aiModelInstalled(model: string): Promise<boolean> {
  return invoke("ai_model_installed", { model });
}

export function listInstalledAiModels(): Promise<string[]> {
  return invoke("list_installed_ai_models");
}

/** Resolves `false` when the user stopped the download. */
export function downloadAiModel(model: string): Promise<boolean> {
  return invoke("download_ai_model", { model });
}

export function cancelModelDownload(): Promise<void> {
  return invoke("cancel_model_download");
}

export function modelDownloadStatus(): Promise<ModelDownloadStatus> {
  return invoke("model_download_status");
}

export function categorize(
  recheck = false,
  model?: string,
): Promise<AiCategorizeOutcome> {
  return invoke("categorize", { model, recheck });
}

export function listAiReview(
  page: number,
  pageSize: number,
  filter: ReviewFilter = {},
): Promise<AiReviewPage> {
  return invoke("list_ai_review", { page, pageSize, filter });
}

export function listAiReviewGroups(
  page: number,
  pageSize: number,
  filter: ReviewFilter = {},
): Promise<AiReviewGroupPage> {
  return invoke("list_ai_review_groups", { page, pageSize, filter });
}

export function acceptAiLabels(activityIds: number[]): Promise<void> {
  return invoke("accept_ai_labels", { activityIds });
}

/** Pins activities to a category; `null` rejects their AI label. */
export function setActivitiesCategory(
  activityIds: number[],
  categoryId: number | null,
): Promise<void> {
  return invoke("set_activities_category", { activityIds, categoryId });
}

export function aiFeedbackStats(): Promise<FeedbackStats> {
  return invoke("ai_feedback_stats");
}
