import { addDays, format, startOfDay } from "date-fns";

export type TimeRangeKind =
  | "all"
  | "today"
  | "yesterday"
  | "3d"
  | "7d"
  | "custom";

export interface TimeRange {
  kind: TimeRangeKind;
  from?: string;
  to?: string;
}

export const ALL_TIME: TimeRange = { kind: "all" };

const PRESET_DAYS_BACK: Record<"today" | "3d" | "7d", number> = {
  today: 0,
  "3d": 2,
  "7d": 6,
};

export const RELATIVE_PRESETS: { kind: TimeRangeKind; label: string }[] = [
  { kind: "all", label: "All time" },
  { kind: "today", label: "Today" },
  { kind: "yesterday", label: "Yesterday" },
  { kind: "3d", label: "Last 3 days" },
  { kind: "7d", label: "Last 7 days" },
];

export function resolveRange(range: TimeRange): {
  start?: string;
  end?: string;
} {
  const today = startOfDay(new Date());
  switch (range.kind) {
    case "all":
      return {};
    case "today":
    case "3d":
    case "7d":
      return {
        start: addDays(today, -PRESET_DAYS_BACK[range.kind]).toISOString(),
        end: addDays(today, 1).toISOString(),
      };
    case "yesterday":
      return {
        start: addDays(today, -1).toISOString(),
        end: today.toISOString(),
      };
    case "custom":
      return { start: range.from, end: range.to };
  }
}

export function describeRange(range: TimeRange): string {
  switch (range.kind) {
    case "custom": {
      const from = range.from ? formatBound(range.from) : "…";
      const to = range.to ? formatBound(range.to) : "…";
      return `${from} – ${to}`;
    }
    default:
      return (
        RELATIVE_PRESETS.find((preset) => preset.kind === range.kind)?.label ??
        "All time"
      );
  }
}

function formatBound(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "…" : format(date, "MMM d, HH:mm");
}

export function toLocalInputValue(value: string | undefined): string {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : format(date, "yyyy-MM-dd'T'HH:mm");
}

export function fromLocalInputValue(value: string): string | undefined {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
}

export function isRangeActive(range: TimeRange): boolean {
  return range.kind !== "all";
}
