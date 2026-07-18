export interface SessionRow {
  id: number;
  app_name: string;
  exe_path: string;
  window_title: string;
  url: string | null;
  start_time: string;
  end_time: string | null;
  category_id: number | null;
  category_name: string | null;
  category_color: string | null;
  category_is_override: boolean;
}

export interface SessionsPage {
  rows: SessionRow[];
  total: number;
}

export interface Category {
  id: number;
  name: string;
  color: string;
}

export type KeyKind = "exe" | "domain" | "url";

export type MatchKind = "exact" | "prefix" | "substring" | "regex";

export interface CategoryRule {
  id: number;
  key_kind: KeyKind;
  pattern: string;
  match_kind: MatchKind;
  category_id: number;
}

export interface CategoryKeyRef {
  key_kind: KeyKind;
  pattern: string;
}

export interface CategorizePreview {
  key_kind: KeyKind;
  key: string;
  affected_count: number;
}

export type IgnoreKind = "app" | "site";

export interface IgnoreRule {
  id: number;
  kind: IgnoreKind;
  pattern: string;
  match_kind: MatchKind;
}

export interface CategoryStat {
  category_id: number | null;
  name: string | null;
  color: string | null;
  total_seconds: number;
  session_count: number;
}

export interface AppStat {
  label: string;
  kind: KeyKind;
  category_color: string | null;
  total_seconds: number;
  session_count: number;
}

export interface DailyStat {
  day: string;
  total_seconds: number;
}

export interface ActivityStats {
  total_seconds: number;
  session_count: number;
  categories: CategoryStat[];
  apps: AppStat[];
  daily: DailyStat[];
}

/** A query-string param kept on stored URLs for a given domain — every
 * other param is stripped before a browser-tab session is ever recorded. */
export interface QueryWhitelistEntry {
  id: number;
  domain: string;
  param: string;
}

/** A user-created focus block. `end_time` is always set — start + target at
 * creation (the session auto-ends by reaching it), rewritten by an early
 * stop or edit. `category_ids` are the categories that count as on-task. */
export interface WorkSession {
  id: number;
  name: string;
  note: string | null;
  target_seconds: number;
  start_time: string;
  end_time: string;
  created_at: string;
  category_ids: number[];
}

export type TimelineBlockKind = "on_task" | "off_task" | "idle";

/** One contiguous stretch of a work session's analyzed window. */
export interface TimelineBlock {
  start_time: string;
  end_time: string;
  /** Empty for idle blocks. */
  label: string;
  kind: TimelineBlockKind;
  category_color: string | null;
}

export interface WorkSessionAppStat {
  label: string;
  kind: KeyKind;
  on_task: boolean;
  category_color: string | null;
  total_seconds: number;
}

/** Tracked history evaluated against a work session's target categories.
 * For an in-progress session the window is clamped to now, so this doubles
 * as the live view. `on + off + idle` equals the window length. */
export interface WorkSessionStats {
  window_start: string;
  window_end: string;
  in_progress: boolean;
  on_task_seconds: number;
  off_task_seconds: number;
  idle_seconds: number;
  /** 0-100; 0 when nothing was tracked. */
  focus_percent: number;
  switch_count: number;
  apps: WorkSessionAppStat[];
  timeline: TimelineBlock[];
}

/** Full rewrite of a work session, as the edit dialog sends it. */
export interface WorkSessionUpdate {
  id: number;
  name: string;
  note: string | null;
  target_seconds: number;
  start_time: string;
  end_time: string;
  category_ids: number[];
}
