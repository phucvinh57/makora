import type { QueryClient } from "@tanstack/react-query";

const LABELING_QUERY_KEYS = [
  "categories",
  "categoryRules",
  "sessions",
  "activityStats",
  "workSessionStats",
  "categorySuggestions",
  "multiPurposeKeys",
  "classifierStatus",
  "activityClusters",
  "aiAccuracyReport",
] as const;

export function invalidateLabelingQueries(queryClient: QueryClient) {
  for (const key of LABELING_QUERY_KEYS) {
    void queryClient.invalidateQueries({ queryKey: [key] });
  }
}
