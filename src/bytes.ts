export function formatBytes(bytes: number): string {
  return bytes >= 1e9
    ? `${(bytes / 1e9).toFixed(2)} GB`
    : `${Math.round(bytes / 1e6)} MB`;
}
