export interface SessionRow {
  id: number;
  activity_id: number | null;
  activity_session_count: number;
  app_name: string;
  exe_path: string;
  window_title: string;
  url: string | null;
  start_time: string;
  end_time: string | null;
  category_id: number | null;
  category_name: string | null;
  category_color: string | null;
  category_is_pinned: boolean;
  category_source: "manual" | "rule" | null;
}

export interface UrlQueryParam {
  id: number;
  domain: string;
  param: string;
}

export interface SessionsPage {
  rows: SessionRow[];
  total: number;
}

export interface Category {
  id: number;
  name: string;
  color: string;
  parent_id: number | null;
  description: string | null;
}

export interface SeedCategory {
  name: string;
  color: string;
  description: string | null;
  children: SeedCategory[];
}

export interface SeedRule {
  key_kind: KeyKind;
  match_kind: MatchKind;
  pattern: string;
  category: string;
}

export interface SeedTaxonomy {
  categories: SeedCategory[];
  rules: SeedRule[];
}

export type KeyKind = "exe" | "domain" | "url";

/**
 * Change one, change the others: `MatchKind` in `src-tauri/src/core/category.rs`, the
 * CHECK constraints in the migrations, and the label maps in `RulesTab.tsx` and
 * `IgnoreListSection.tsx`. Nothing checks those four against each other.
 */
export type MatchKind = "exact" | "prefix" | "substring" | "glob";

export interface CategoryRule {
  id: number;
  key_kind: KeyKind;
  pattern: string;
  match_kind: MatchKind;
  category_id: number;
}

export interface RecategorizeOutcome {
  manual_categories_preserved: number;
  rows_resolved: number;
}

export interface CategorizationResetOutcome {
  rules_deleted: number;
  sessions_cleared: number;
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

export interface TimelineBlock {
  start_time: string;
  end_time: string;
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

export interface WorkSessionStats {
  window_start: string;
  window_end: string;
  in_progress: boolean;
  on_task_seconds: number;
  off_task_seconds: number;
  idle_seconds: number;
  focus_percent: number;
  switch_count: number;
  apps: WorkSessionAppStat[];
  timeline: TimelineBlock[];
}

export interface WorkSessionUpdate {
  id: number;
  name: string;
  note: string | null;
  target_seconds: number;
  start_time: string;
  end_time: string;
  category_ids: number[];
}
