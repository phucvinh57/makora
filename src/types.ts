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

export type KeyKind = "exe" | "domain";

export type MatchKind = "exact" | "substring" | "regex";

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
