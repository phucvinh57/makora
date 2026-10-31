import {
  AppWindow,
  Box,
  Database,
  Globe,
  type LucideIcon,
  MousePointerClick,
  Settings2,
  Sparkles,
} from "lucide-react";
import type { LogArea, LogLevel } from "@/types";

export type Period = "24h" | "7d" | "30d";
export type GroupBy = "day" | "area";
export type MinLevel = Extract<LogLevel, "info" | "warn" | "error">;

export const PERIODS: { value: Period; label: string; days: number }[] = [
  { value: "24h", label: "24 hours", days: 1 },
  { value: "7d", label: "7 days", days: 7 },
  { value: "30d", label: "30 days", days: 30 },
];

export const PERIOD_PHRASES: Record<Period, string> = {
  "24h": "in the last 24 hours",
  "7d": "in the last 7 days",
  "30d": "in the last 30 days",
};

export const MIN_LEVEL_LABELS: Record<MinLevel, string> = {
  info: "All levels",
  warn: "Warnings & errors",
  error: "Errors only",
};

export const AREAS: LogArea[] = [
  "network",
  "ai",
  "browser",
  "tracking",
  "settings",
  "storage",
  "app",
];

export const AREA_META: Record<LogArea, { label: string; icon: LucideIcon }> = {
  network: { label: "Network", icon: Globe },
  ai: { label: "AI", icon: Sparkles },
  browser: { label: "Browser", icon: AppWindow },
  tracking: { label: "Tracking", icon: MousePointerClick },
  settings: { label: "Settings", icon: Settings2 },
  storage: { label: "Storage", icon: Database },
  app: { label: "App", icon: Box },
};

export const SEARCH_DEBOUNCE_MS = 300;

/** Fields shown on a collapsed row, in this order, before any others. */
export const KEY_FIELDS = [
  "host",
  "method",
  "path",
  "status",
  "setting",
  "value",
  "provider",
  "model",
  "browser",
  "activities_sent",
  "error",
];
export const INLINE_FIELD_COUNT = 4;
