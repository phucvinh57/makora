import { differenceInSeconds } from "date-fns";

export function formatDuration(
  startTime: string,
  endTime: string | null,
): string {
  const start = new Date(startTime);
  const end = endTime ? new Date(endTime) : new Date();
  const totalSeconds = Math.max(0, differenceInSeconds(end, start));
  return formatSeconds(totalSeconds);
}

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
