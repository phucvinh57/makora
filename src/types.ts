export interface SessionRow {
  id: number;
  activity_id: number;
  app_name: string;
  source_kind: KeyKind;
  source_key: string;
  title: string;
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

/** One executable or domain whose pending AI labels all suggest the same category. */
export interface AiReviewGroup {
  key_kind: "exe" | "domain";
  key: string;
  app_name: string;
  category_id: number;
  category_name: string;
  category_color: string;
  activity_count: number;
  total_seconds: number;
  /** The least sure the model was about any member; null when unknown. */
  min_confidence: number | null;
  activity_ids: number[];
}

export interface AiReviewGroupPage {
  groups: AiReviewGroup[];
  total: number;
}

export interface ReviewFilter {
  /** Only suggestions in this category or under it. */
  category?: number;
  /** Only these activities, e.g. the members of one group. */
  activities?: number[];
  /** Hide labels the model was confident about. */
  uncertain_only?: boolean;
  /** Hide activities (or groups) tracked for less than this. */
  min_seconds?: number;
}

export interface FeedbackStats {
  corrected: number;
  confirmed: number;
  pinned: number;
  rejected: number;
  pending_review: number;
  /** Tracked time still waiting for review. */
  pending_seconds: number;
  /** Tracked time of the AI labels the user has judged. */
  reviewed_seconds: number;
}

export interface AiCategorizeProgress {
  done: number;
  total: number;
}

/** Lets a page that reloaded mid-run find out a categorization is still going. */
export interface AiCategorizeStatus {
  running: boolean;
  recheck: boolean;
  done: number;
  total: number;
}

/** Lets a page that reloaded mid-download find out it is still going. */
export interface ModelDownloadStatus {
  running: boolean;
  model: string;
  downloaded: number;
  total: number;
}

export interface CategorizationResetOutcome {
  /** Manual pins and AI labels removed. */
  labels_cleared: number;
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
