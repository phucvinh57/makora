import type { RangePreset } from "./date";

export const RANGE_PRESETS: { value: RangePreset; label: string }[] = [
  { value: "today", label: "Today" },
  { value: "7d", label: "7 days" },
  { value: "30d", label: "30 days" },
];

// Slice/bar color for sessions with no resolved category — deliberately not
// one of the app's own category colors (those are user-assigned and
// arbitrary), just a neutral gray that reads on both light and dark surfaces.
export const UNCATEGORIZED_COLOR = "#9ca3af";
