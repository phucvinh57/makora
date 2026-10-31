import type { HistorySourceFilter } from "@/types";
import { ALL_SOURCES } from "./constants";
import type { TimeRange } from "./timeRange";

export type SourceChoice = typeof ALL_SOURCES | HistorySourceFilter;

export const SOURCE_CHOICE_LABELS: Record<SourceChoice, string> = {
  [ALL_SOURCES]: "All sources",
  manual: "Manual",
  rule: "Rule",
};

export function parseSourceFilter(
  value: unknown,
): HistorySourceFilter | undefined {
  return value === "manual" || value === "rule" ? value : undefined;
}

export interface HistoryFilters {
  category: string;
  source: SourceChoice;
  range: TimeRange;
}
