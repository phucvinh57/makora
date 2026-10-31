import { format, isToday, isYesterday } from "date-fns";
import type { LogEntry } from "@/types";
import { INLINE_FIELD_COUNT, KEY_FIELDS } from "./constants";

type FieldValue = LogEntry["fields"][string];

export function formatSize(bytes: number): string {
  if (bytes < 1_000) return `${bytes} B`;
  if (bytes < 1_000_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
}

export function formatDayLabel(date: Date): string {
  if (isToday(date)) return "Today";
  if (isYesterday(date)) return "Yesterday";
  return format(date, "EEEE, MMM d");
}

export function formatRelative(ts: string): string {
  const date = new Date(ts);
  if (isToday(date)) return format(date, "HH:mm");
  if (isYesterday(date)) return `Yesterday ${format(date, "HH:mm")}`;
  return format(date, "MMM d, HH:mm");
}

export function formatFieldValue(key: string, value: FieldValue): string {
  if (typeof value === "number") {
    if (key.startsWith("bytes_") || key.endsWith("_bytes")) {
      return formatSize(value);
    }
    if (key === "duration_ms") return `${value} ms`;
    return value.toLocaleString();
  }
  return String(value);
}

/** The fields worth seeing without expanding a row: the well-known ones first. */
export function keyFields(fields: LogEntry["fields"]): [string, FieldValue][] {
  const entries = Object.entries(fields);
  const rank = (key: string) => {
    const index = KEY_FIELDS.indexOf(key);
    return index === -1 ? KEY_FIELDS.length : index;
  };
  return entries
    .sort(([a], [b]) => rank(a) - rank(b))
    .slice(0, INLINE_FIELD_COUNT);
}
