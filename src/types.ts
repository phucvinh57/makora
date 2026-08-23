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
  category_source: "manual" | "rule" | "ai" | null;
  category_confidence: number | null;
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

export interface CategorySuggestion {
  id: number;
  key_kind: "exe" | "domain" | "url";
  key: string;
  category_id: number;
  category_name: string;
  confidence: number;
  seconds: number;
  model_version: number;
  created_at: string;
}

export interface ActivityClusterMember {
  id: number;
  activity_id: number | null;
  key_kind: "exe" | "domain";
  key: string;
  url_key: string | null;
  window_title: string | null;
  seconds: number;
  session_count: number;
}

export interface ActivityCluster {
  id: number;
  scope: "key" | "title";
  parent_kind: "exe" | "domain" | null;
  parent_key: string | null;
  label: string;
  category_id: number | null;
  category_name: string | null;
  confidence: number | null;
  lead: number | null;
  cohesion: number;
  member_count: number;
  seconds: number;
  model_version: number;
  created_at: string;
  members: ActivityClusterMember[];
}

export interface ClusterAssignment {
  rules_created: number;
  sessions_pinned: number;
}

export type ClusteringGranularity = "broad" | "balanced" | "fine";

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
  multi_purpose: SeedMultiPurposeKey[];
}

export interface SeedMultiPurposeKey {
  key_kind: "exe" | "domain";
  key: string;
}

export interface MultiPurposeKey {
  key_kind: "exe" | "domain";
  key: string;
  source: "seed" | "auto" | "user";
  enabled: boolean;
  top_share: number | null;
  detected_at: string | null;
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
  inactive: boolean;
}

export interface RecategorizeOutcome {
  manual_categories_preserved: number;
  ai_predictions_cleared: number;
  rows_resolved: number;
}

export interface CategorizationResetOutcome {
  rules_deleted: number;
  sessions_cleared: number;
  suggestions_deleted: number;
  embeddings_deleted: number;
  clusters_deleted: number;
  auto_multi_purpose_keys_deleted: number;
  verdicts_deleted: number;
  run_evaluations_deleted: number;
}

export type AiCategorizationSchedule = "manual" | "hourly" | "daily" | "weekly";

export interface ClassifierStatus {
  state:
    | "idle"
    | "queued"
    | "running"
    | "ready"
    | "insufficient_data"
    | "error";
  generation: number;
  update_kind: "cached" | "incremental" | "full_rebuild" | null;
  training_example_count: number;
  trained_category_count: number;
  last_completed_at: string | null;
  last_execution_duration_ms: number | null;
  last_error: string | null;
  schedule: AiCategorizationSchedule;
  next_scheduled_at: string | null;
  confidence_threshold: number;
  holdout_accuracy: number | null;
  holdout_sample_count: number;
  cluster_granularity: ClusteringGranularity;
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

export interface VerdictTotals {
  total_count: number;
  total_seconds: number;
  agree_count: number;
  agree_seconds: number;
  corrected_count: number;
  corrected_seconds: number;
  rejected_count: number;
  rejected_seconds: number;
  no_prediction_count: number;
  no_prediction_seconds: number;
}

export interface ConfidenceBucket {
  lower: number;
  upper: number;
  count: number;
  seconds: number;
  precision: number | null;
}

export interface SurfaceStat {
  surface: "session" | "suggestion" | "cluster";
  totals: VerdictTotals;
  precision: number | null;
  time_weighted_precision: number | null;
  coverage: number | null;
}

export interface DailyPrecision {
  day: string;
  reviewed_count: number;
  agree_count: number;
  precision: number | null;
}

export interface ConfusionPair {
  predicted_category_name: string;
  chosen_category_name: string;
  count: number;
  seconds: number;
}

export interface RunEvaluation {
  recorded_at: string;
  model_version: number;
  update_kind: "cached" | "incremental" | "full_rebuild";
  confidence_threshold: number;
  holdout_accuracy: number | null;
  holdout_sample_count: number;
  manual_holdout_accuracy: number | null;
  manual_holdout_sample_count: number;
  predicted_session_count: number;
  labeled_key_count: number;
  category_count: number;
}

export interface AiAccuracyReport {
  totals: VerdictTotals;
  precision: number | null;
  time_weighted_precision: number | null;
  coverage: number | null;
  by_confidence: ConfidenceBucket[];
  by_surface: SurfaceStat[];
  daily: DailyPrecision[];
  confusions: ConfusionPair[];
  runs: RunEvaluation[];
  excluded_bulk_count: number;
  current_confidence_threshold: number;
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
