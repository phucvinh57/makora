import type { TimeRange } from "./timeRange";

/** The History filters other than the free-text search, as the pickers show them. */
export interface HistoryFilters {
  category: string;
  source: string;
  range: TimeRange;
}
