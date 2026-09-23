import type { QueryClient } from "@tanstack/react-query";

const CATEGORIZATION_QUERY_KEYS = [
  "categories",
  "categoryRules",
  "sessions",
  "activityStats",
  "workSessionStats",
] as const;

export function invalidateCategorizationQueries(queryClient: QueryClient) {
  for (const key of CATEGORIZATION_QUERY_KEYS) {
    void queryClient.invalidateQueries({ queryKey: [key] });
  }
}
