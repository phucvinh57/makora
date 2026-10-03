import type { TimeRange } from "./timeRange";

export interface HistoryFilters {
  category: string;
  source: string;
  range: TimeRange;
}
