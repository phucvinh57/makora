import { useQuery } from "@tanstack/react-query";
import { categoriesQueryOptions } from "@/queries";
import type { Category } from "@/types";

const NO_CATEGORIES: Category[] = [];

/** The category list, never `undefined` and with a stable identity while it loads. */
export function useCategories() {
  const query = useQuery(categoriesQueryOptions);
  return {
    categories: query.data ?? NO_CATEGORIES,
    isLoading: query.isLoading,
    error: query.error,
  };
}
