export function formatDuration(
  startTime: string,
  endTime: string | null,
): string {
  const start = new Date(startTime).getTime();
  const end = endTime ? new Date(endTime).getTime() : Date.now();
  const totalSeconds = Math.max(0, Math.floor((end - start) / 1000));
  return formatSeconds(totalSeconds);
}

/** Same `Xh Ym` / `Xm Ys` / `Xs` shape as `formatDuration`, but for an
 * already-aggregated duration (e.g. a summed total from `activityStats`)
 * rather than a start/end timestamp pair. */
export function formatSeconds(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));

  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;

  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${secs}s`;
  return `${secs}s`;
}

export function formatTimestamp(value: string): string {
  return new Date(value).toLocaleString();
}
