export interface SessionRow {
  id: number;
  activity_id: number | null;
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
  category_source: "manual" | "ai" | null;
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

export interface SeedTaxonomy {
  categories: SeedCategory[];
}

export type KeyKind = "exe" | "domain";

export interface AiCategorizeOutcome {
  labeled: number;
  skipped: number;
  changed: number;
  stopped: boolean;
}

export interface AiReviewRow {
  activity_id: number;
  app_name: string;
  url: string | null;
  title: string;
  category_id: number;
  category_name: string;
  category_color: string;
  total_seconds: number;
  session_count: number;
}

export interface AiReviewPage {
  rows: AiReviewRow[];
  total: number;
}

export interface FeedbackStats {
  corrected: number;
  confirmed: number;
  pinned: number;
  rejected: number;
  pending_review: number;
}

export interface AiCategorizeProgress {
  done: number;
  total: number;
}

export interface CategorizationResetOutcome {
  sessions_cleared: number;
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
