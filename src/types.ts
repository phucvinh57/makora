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
  category_source: "manual" | "rule" | "ai" | null;
}

/** The category sources the history list can be narrowed to. */
export type HistorySourceFilter = "manual" | "rule";

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

export interface AiReviewGroup {
  key_kind: "exe" | "domain";
  key: string;
  app_name: string;
  category_id: number;
  category_name: string;
  category_color: string;
  activity_count: number;
  total_seconds: number;
  activity_ids: number[];
}

export interface AiReviewGroupPage {
  groups: AiReviewGroup[];
  total: number;
}

export interface ReviewFilter {
  category?: number;
  activities?: number[];
}

export interface FeedbackStats {
  corrected: number;
  confirmed: number;
  pinned: number;
  rejected: number;
  pending_review: number;
  pending_seconds: number;
  reviewed_seconds: number;
}

export interface AiCategorizeProgress {
  done: number;
  total: number;
}

export interface AiCategorizeStatus {
  running: boolean;
  recheck: boolean;
  done: number;
  total: number;
}

/** The LLM labels activities; the embedding model finds the filed ones most like each. */
export type ModelRole = "llm" | "embedding";

export interface InstalledModel {
  name: string;
  role: ModelRole;
}

export interface ModelDownloadStatus {
  running: boolean;
  model: string;
  downloaded: number;
  total: number;
}

export interface CategorizationResetOutcome {
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

export interface Exclusion {
  kind: RuleKind;
  /** Only for a pattern. */
  field?: PatternField;
  value: string;
}

export interface Settings {
  tracking: {
    min_session_seconds: number;
    excluded: Exclusion[];
  };
  ai: {
    /** The remote provider the LLM runs on; `null` runs it locally. */
    llm_provider: string | null;
    /** A local model name, or the remote provider's model id. */
    llm_model: string;
    embedding_model: string;
    /** Hours between automatic runs: 3, 6, 12, 24, 72 or 168; 0 means off. */
    auto_categorize_hours: number;
    providers: ProviderConfig[];
  };
}

/** `openai` covers every provider that speaks OpenAI's chat-completions API. */
export type ProviderKind = "openai" | "anthropic";

/** A remote LLM provider. Its API key lives in the OS keychain, never here. */
export interface ProviderConfig {
  id: string;
  name: string;
  kind: ProviderKind;
  base_url: string;
}

export interface ProviderDraft {
  /** Left out for a new provider. */
  id?: string;
  name: string;
  kind: ProviderKind;
  base_url: string;
}

export type RuleKind = "app" | "site" | "page" | "pattern";

export type PatternField = "url" | "title" | "app";

export interface RuleDraft {
  kind: RuleKind;
  /** Only for a pattern. */
  field: PatternField | null;
  value: string;
  categoryId: number;
}

export interface CategoryRule {
  id: number;
  kind: RuleKind;
  field: PatternField | null;
  value: string;
  category_id: number;
  /** Activities the rule decides now; ones pinned elsewhere are not counted. */
  activity_count: number;
  total_seconds: number;
}

export interface RulePreview {
  activity_count: number;
  total_seconds: number;
  /** Of those, how many the user pinned themselves, which the rule leaves alone. */
  pinned_count: number;
  samples: string[];
}

export interface SourceSuggestion {
  kind: KeyKind;
  key: string;
  name: string;
  total_seconds: number;
}

export interface PathSuggestion {
  path: string;
  total_seconds: number;
}
