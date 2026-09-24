import type { QueryClient } from "@tanstack/react-query";

const CATEGORIZATION_QUERY_KEYS = [
  "categories",
  "sessions",
  "activityStats",
  "aiReview",
  "aiFeedbackStats",
] as const;

export function invalidateCategorizationQueries(queryClient: QueryClient) {
  for (const key of CATEGORIZATION_QUERY_KEYS) {
    void queryClient.invalidateQueries({ queryKey: [key] });
  }
}
