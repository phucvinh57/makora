/** Status colors for the on/off/idle encoding, shared by the timeline
 * strip, the proportion bar, and the per-app list so a reader learns them
 * once. `--chart-1` (indigo) vs `--chart-2` (amber) is a blue/orange pair —
 * validated CVD-safe in both themes — and idle is deliberately colorless. */
export const ON_TASK_COLOR = "var(--chart-1)";
export const OFF_TASK_COLOR = "var(--chart-2)";

export const WORK_SESSIONS_KEY = ["workSessions"];

export function workSessionStatsKey(id: number) {
  return ["workSessionStats", id];
}
