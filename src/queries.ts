import { queryOptions, type QueryClient } from "@tanstack/react-query";
import {
  aiModel,
  aiProvider,
  aiReview,
  categorization,
  category,
  categoryRule,
  settings,
  suggestion,
} from "./api";
import type { AiRunStatus, ModelDownloadStatus } from "./types";

export const queryKeys = {
  categories: ["categories"],
  categoryRules: ["categoryRules"],
  sourceSuggestions: ["sourceSuggestions"],
  pathSuggestions: ["pathSuggestions"],
  rulePreview: ["rulePreview"],
  sessions: ["sessions"],
  activityStats: ["activityStats"],
  aiReview: ["aiReview"],
  aiReviewGroups: ["aiReviewGroups"],
  aiFeedbackStats: ["aiFeedbackStats"],
  aiCategorizeStatus: ["aiCategorizeStatus"],
  aiNextAutoRun: ["aiNextAutoRun"],
  aiModelDownloadStatus: ["aiModelDownloadStatus"],
  aiModelLoaded: ["aiModelLoaded"],
  aiInstalledModels: ["aiInstalledModels"],
  aiModelInstalled: ["aiModelInstalled"],
  aiProviderHasKey: ["aiProviderHasKey"],
  aiProviderModels: ["aiProviderModels"],
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

export const providerHasKeyQueryOptions = (id: string) =>
  queryOptions({
    queryKey: [...queryKeys.aiProviderHasKey, id],
    queryFn: () => aiProvider.hasKey(id),
  });

/** Also the connection test: it fails when the address or the key does not work. */
export const providerModelsQueryOptions = (id: string) =>
  queryOptions({
    queryKey: [...queryKeys.aiProviderModels, id],
    queryFn: () => aiProvider.listModels(id),
    staleTime: 5 * 60_000,
    retry: false,
  });

export const categorizeStatusQueryOptions = queryOptions({
  queryKey: queryKeys.aiCategorizeStatus,
  queryFn: categorization.status,
  refetchInterval: (query) =>
    query.state.data?.running ? RUNNING_POLL_MS : false,
});

/** `inputs` are whatever moves the due time; changing one is a new query, so it refetches. */
export const nextAutoRunQueryOptions = (...inputs: unknown[]) =>
  queryOptions({
    queryKey: [...queryKeys.aiNextAutoRun, ...inputs],
    queryFn: categorization.nextAutoRun,
    refetchInterval: 60_000,
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
  queryClient.setQueryData<AiRunStatus>(queryKeys.aiCategorizeStatus, {
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
