export const ON_TASK_COLOR = "var(--chart-1)";
export const OFF_TASK_COLOR = "var(--chart-2)";

export const WORK_SESSIONS_KEY = ["workSessions"];

export function workSessionStatsKey(id: number) {
  return ["workSessionStats", id];
}
