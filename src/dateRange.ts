import { addDays, startOfDay } from "date-fns";

/** RFC 3339 bounds, `end` exclusive. */
export interface DateBounds {
  start: string;
  end: string;
}

/** From local midnight `daysBack` days before today up to the coming local midnight. */
export function daysUpToToday(daysBack: number, now = new Date()): DateBounds {
  const today = startOfDay(now);
  return {
    start: addDays(today, -daysBack).toISOString(),
    end: addDays(today, 1).toISOString(),
  };
}

export function yesterday(now = new Date()): DateBounds {
  const today = startOfDay(now);
  return {
    start: addDays(today, -1).toISOString(),
    end: today.toISOString(),
  };
}
