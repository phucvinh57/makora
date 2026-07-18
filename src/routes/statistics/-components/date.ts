import { format, parseISO, startOfDay, subDays } from "date-fns";

export type RangePreset = "today" | "7d" | "30d";

const RANGE_DAYS_BACK: Record<RangePreset, number> = {
  today: 0,
  "7d": 6,
  "30d": 29,
};

function localMidnight(daysAgo: number): Date {
  return subDays(startOfDay(new Date()), daysAgo);
}

export function computeRange(preset: RangePreset): {
  start: string;
  end: string;
} {
  return {
    start: localMidnight(RANGE_DAYS_BACK[preset]).toISOString(),
    end: localMidnight(-1).toISOString(),
  };
}

export function formatDay(day: string): string {
  return format(parseISO(day), "MMM d");
}
