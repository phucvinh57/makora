import { format, parseISO } from "date-fns";
import { type DateBounds, daysUpToToday } from "@/dateRange";

export type RangePreset = "today" | "7d" | "30d";

const RANGE_DAYS_BACK: Record<RangePreset, number> = {
  today: 0,
  "7d": 6,
  "30d": 29,
};

export function computeRange(preset: RangePreset): DateBounds {
  return daysUpToToday(RANGE_DAYS_BACK[preset]);
}

export function formatDay(day: string): string {
  return format(parseISO(day), "MMM d");
}
