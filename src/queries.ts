import { queryOptions, type QueryClient } from "@tanstack/react-query";
import {
  aiModel,
  aiReview,
  categorization,
  category,
  categoryRule,
  settings,
  suggestion,
} from "./api";
import type { AiCategorizeStatus, ModelDownloadStatus } from "./types";

export const queryKeys = {
  categories: ["categories"],
  categoryRules: ["categoryRules"],
  sourceSuggestions: ["sourceSuggestions"],
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
  settings: ["settings"],
} as const;

/** Polling only runs while the backend reports a task; starting one flags it up front. */
const RUNNING_POLL_MS = 1_000;

export const categoriesQueryOptions = queryOptions({
  queryKey: queryKeys.categories,
  queryFn: category.list,
});

export const categoryRulesQueryOptions = queryOptions({
  queryKey: queryKeys.categoryRules,
  queryFn: categoryRule.list,
});

export const sourceSuggestionsQueryOptions = queryOptions({
  queryKey: queryKeys.sourceSuggestions,
  queryFn: suggestion.sources,
  staleTime: 60_000,
});

export const settingsQueryOptions = queryOptions({
  queryKey: queryKeys.settings,
  queryFn: settings.get,
});

export const aiFeedbackStatsQueryOptions = queryOptions({
  queryKey: queryKeys.aiFeedbackStats,
  queryFn: aiReview.feedbackStats,
});

export const installedModelsQueryOptions = queryOptions({
  queryKey: queryKeys.aiInstalledModels,
  queryFn: aiModel.listInstalled,
});

export const categorizeStatusQueryOptions = queryOptions({
  queryKey: queryKeys.aiCategorizeStatus,
  queryFn: categorization.status,
  refetchInterval: (query) =>
    query.state.data?.running ? RUNNING_POLL_MS : false,
});

export const downloadStatusQueryOptions = queryOptions({
  queryKey: queryKeys.aiModelDownloadStatus,
  queryFn: aiModel.downloadStatus,
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
      queryKeys.categoryRules,
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
