import { queryOptions, type QueryClient } from "@tanstack/react-query";
import {
  aiFeedbackStats,
  categorizeStatus,
  listCategories,
  listInstalledAiModels,
  modelDownloadStatus,
} from "./api";
import type { AiCategorizeStatus, ModelDownloadStatus } from "./types";

/** Every query key in one place, so invalidation cannot drift from what the pages register. */
export const queryKeys = {
  categories: ["categories"],
  sessions: ["sessions"],
  activityStats: ["activityStats"],
  aiReview: ["aiReview"],
  aiReviewGroups: ["aiReviewGroups"],
  aiFeedbackStats: ["aiFeedbackStats"],
  aiCategorizeStatus: ["aiCategorizeStatus"],
  aiModelDownloadStatus: ["aiModelDownloadStatus"],
  aiModelLoaded: ["aiModelLoaded"],
  aiInstalledModels: ["aiInstalledModels"],
  aiModelInstalled: ["aiModelInstalled"],
  autostartEnabled: ["autostartEnabled"],
} as const;

/** Polling only runs while the backend reports a task; starting one flags it up front. */
const RUNNING_POLL_MS = 1_000;

export const categoriesQueryOptions = queryOptions({
  queryKey: queryKeys.categories,
  queryFn: listCategories,
});

export const aiFeedbackStatsQueryOptions = queryOptions({
  queryKey: queryKeys.aiFeedbackStats,
  queryFn: aiFeedbackStats,
});

export const installedModelsQueryOptions = queryOptions({
  queryKey: queryKeys.aiInstalledModels,
  queryFn: listInstalledAiModels,
});

export const categorizeStatusQueryOptions = queryOptions({
  queryKey: queryKeys.aiCategorizeStatus,
  queryFn: categorizeStatus,
  refetchInterval: (query) =>
    query.state.data?.running ? RUNNING_POLL_MS : false,
});

export const downloadStatusQueryOptions = queryOptions({
  queryKey: queryKeys.aiModelDownloadStatus,
  queryFn: modelDownloadStatus,
  refetchInterval: (query) =>
    query.state.data?.running ? RUNNING_POLL_MS : false,
});

/**
 * Everything a category edit, a pin or an AI label can change. Returning the promise lets a
 * mutation stay pending until the screen shows the new data.
 */
export function invalidateCategorizationQueries(
  queryClient: QueryClient,
): Promise<unknown> {
  return Promise.all(
    [
      queryKeys.categories,
      queryKeys.sessions,
      queryKeys.activityStats,
      queryKeys.aiReview,
      queryKeys.aiReviewGroups,
      queryKeys.aiFeedbackStats,
    ].map((queryKey) => queryClient.invalidateQueries({ queryKey })),
  );
}

/**
 * The backend only reports a task once its command is running, and nothing polls while idle, so
 * the page that starts one marks it running itself. In-flight status fetches are cancelled so
 * a stale "idle" answer cannot land on top.
 */
export async function markCategorizeStarted(
  queryClient: QueryClient,
  recheck: boolean,
) {
  await queryClient.cancelQueries({ queryKey: queryKeys.aiCategorizeStatus });
  queryClient.setQueryData<AiCategorizeStatus>(queryKeys.aiCategorizeStatus, {
    running: true,
    recheck,
    done: 0,
    total: 0,
  });
}

export async function markDownloadStarted(
  queryClient: QueryClient,
  model: string,
) {
  await queryClient.cancelQueries({
    queryKey: queryKeys.aiModelDownloadStatus,
  });
  queryClient.setQueryData<ModelDownloadStatus>(
    queryKeys.aiModelDownloadStatus,
    { running: true, model, downloaded: 0, total: 0 },
  );
}
