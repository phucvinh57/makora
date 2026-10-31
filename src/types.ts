import type { AiRunStatus } from "./bindings/AiRunStatus";
import type { CategoryId } from "./bindings/CategoryId";
import type { PatternField } from "./bindings/PatternField";
import type { RuleKind } from "./bindings/RuleKind";

export type { ActivityId } from "./bindings/ActivityId";
export type { ActivityStats } from "./bindings/ActivityStats";
export type { AiReviewGroup } from "./bindings/AiReviewGroup";
export type { AiReviewGroupPage } from "./bindings/AiReviewGroupPage";
export type { AiReviewPage } from "./bindings/AiReviewPage";
export type { AiReviewRow } from "./bindings/AiReviewRow";
export type { AiRunOutcome } from "./bindings/AiRunOutcome";
export type { AiRunStatus } from "./bindings/AiRunStatus";
export type { AiSettings } from "./bindings/AiSettings";
export type { AppStat } from "./bindings/AppStat";
export type { CategorizationResetOutcome } from "./bindings/CategorizationResetOutcome";
export type { Category } from "./bindings/Category";
export type { CategoryId } from "./bindings/CategoryId";
export type { CategoryRuleView } from "./bindings/CategoryRuleView";
export type { CategorySource } from "./bindings/CategorySource";
export type { CategoryStat } from "./bindings/CategoryStat";
export type { DailyStat } from "./bindings/DailyStat";
export type { Exclusion } from "./bindings/Exclusion";
export type { FeedbackStats } from "./bindings/FeedbackStats";
export type { InstalledModel } from "./bindings/InstalledModel";
export type { KeyKind } from "./bindings/KeyKind";
export type { ModelDownloadStatus } from "./bindings/ModelDownloadStatus";
export type { ModelRole } from "./bindings/ModelRole";
export type { PathSuggestion } from "./bindings/PathSuggestion";
export type { PatternField } from "./bindings/PatternField";
export type { ProviderConfig } from "./bindings/ProviderConfig";
export type { ProviderDraft } from "./bindings/ProviderDraft";
export type { ProviderKind } from "./bindings/ProviderKind";
export type { ReviewFilter } from "./bindings/ReviewFilter";
export type { RuleId } from "./bindings/RuleId";
export type { RuleKind } from "./bindings/RuleKind";
export type { RulePreview } from "./bindings/RulePreview";
export type { SessionId } from "./bindings/SessionId";
export type { SessionRow } from "./bindings/SessionRow";
export type { SessionsPage } from "./bindings/SessionsPage";
export type { Settings } from "./bindings/Settings";
export type { SourceSuggestion } from "./bindings/SourceSuggestion";
export type { TrackingSettings } from "./bindings/TrackingSettings";

/** The payload of the `ai-categorize-progress` event. */
export type AiRunProgress = Pick<AiRunStatus, "done" | "total">;

/** The category sources the history list can be narrowed to. */
export type HistorySourceFilter = "manual" | "rule";

/** The arguments of `category_rule_create` / `category_rule_update`. */
export interface RuleDraft {
  kind: RuleKind;
  /** Only for a pattern. */
  field: PatternField | null;
  value: string;
  categoryId: CategoryId;
}
