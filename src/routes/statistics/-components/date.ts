export type RangePreset = "today" | "7d" | "30d";

// How many calendar days back each preset's range starts (inclusive of
// today), e.g. "7 days" spans today and the 6 days before it.
const RANGE_DAYS_BACK: Record<RangePreset, number> = {
  today: 0,
  "7d": 6,
  "30d": 29,
};

function localMidnight(daysAgo: number): Date {
  const date = new Date();
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() - daysAgo);
  return date;
}

// `start`/`end` are RFC 3339 UTC bounds computed against local midnight, so
// "Today" lines up with the user's calendar day rather than the UTC one.
// The range is half-open — `end` is tomorrow's local midnight, matching the
// backend's `start_time >= start AND start_time < end` filter.
export function computeRange(preset: RangePreset): {
  start: string;
  end: string;
} {
  return {
    start: localMidnight(RANGE_DAYS_BACK[preset]).toISOString(),
    end: localMidnight(-1).toISOString(),
  };
}

// `day` is a local-calendar date string like "2024-06-15" (see
// `date(start_time, 'localtime')` in `activity_stats`). Building the `Date`
// from explicit local components — rather than `new Date(day)`, which parses
// as UTC midnight — avoids the label shifting a day off in negative UTC
// offsets.
export function formatDay(day: string): string {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}
