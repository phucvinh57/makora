import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs } from "@tauri-apps/api/core";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as checkAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import type {
  ActivityStats,
  AiCategorizeOutcome,
  AiCategorizeStatus,
  AiReviewGroupPage,
  AiReviewPage,
  Category,
  CategorizationResetOutcome,
  CategoryRule,
  FeedbackStats,
  ModelDownloadStatus,
  PathSuggestion,
  PatternField,
  ReviewFilter,
  RuleDraft,
  RuleKind,
  RulePreview,
  SessionsPage,
  Settings,
  SourceSuggestion,
} from "./types";

/** Why a command failed, stable enough to branch on. */
export type ApiErrorCode =
  | "name_taken"
  | "parent_missing"
  | "cycle"
  | "not_found"
  | "invalid_rule"
  | "duplicate_rule"
  | "category_missing"
  | "already_running"
  | "invalid_input"
  | "internal";

/**
 * A failed command. `String(error)` gives just the message, so it can be shown as it is;
 * `code` says what went wrong.
 */
export class ApiError extends Error {
  readonly code: ApiErrorCode;

  constructor(code: ApiErrorCode, message: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
  }

  override toString(): string {
    return this.message;
  }
}

function isWireError(
  value: unknown,
): value is { code: ApiErrorCode; message: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { code?: unknown }).code === "string" &&
    typeof (value as { message?: unknown }).message === "string"
  );
}

/** Invokes a command, turning whatever it rejects with into an `ApiError`. */
async function call<T>(command: string, args?: InvokeArgs): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    // Tauri itself rejects with a bare string, e.g. for arguments it cannot read.
    throw isWireError(raw)
      ? new ApiError(raw.code, raw.message)
      : new ApiError("internal", String(raw));
  }
}

export interface SessionsQuery {
  /** Zero-based. */
  page: number;
  pageSize: number;
  search?: string;
  category?: string;
  source?: "manual";
  start?: string;
  end?: string;
}

/** What a category is made of; the id is the backend's to assign. */
export interface CategoryDraft {
  name: string;
  color: string;
  description: string | null;
  parentId: number | null;
}

/*
 * Commands are grouped by the domain they belong to, so a call site reads `category.create(...)`
 * rather than a flat `createCategory(...)`. Tauri registers every command under its bare function
 * name, so each command carries its domain as a prefix (`category_create`) and this layer
 * mirrors that split (`category.create`).
 */

export const session = {
  list(query: SessionsQuery): Promise<SessionsPage> {
    return call("session_list", { ...query });
  },

  /** Pins one session row's activity to a category; `null` clears the pin. */
  setRowCategory(sessionId: number, categoryId: number | null): Promise<void> {
    return call("session_set_row_category", { sessionId, categoryId });
  },

  setCategory(sessionIds: number[], categoryId: number): Promise<void> {
    return call("session_set_category", { sessionIds, categoryId });
  },
};

export const activity = {
  stats(start: string, end: string): Promise<ActivityStats> {
    return call("activity_stats", { start, end });
  },

  /** Pins activities to a category; `null` rejects their AI label. */
  setCategory(activityIds: number[], categoryId: number | null): Promise<void> {
    return call("activity_set_category", { activityIds, categoryId });
  },
};

export const category = {
  list(): Promise<Category[]> {
    return call("category_list");
  },

  create(draft: CategoryDraft): Promise<Category> {
    return call("category_create", { ...draft });
  },

  update(id: number, draft: CategoryDraft): Promise<void> {
    return call("category_update", { id, ...draft });
  },

  delete(id: number): Promise<void> {
    return call("category_delete", { id });
  },
};

export const categoryRule = {
  list(): Promise<CategoryRule[]> {
    return call("category_rule_list");
  },

  create(draft: RuleDraft): Promise<number> {
    return call("category_rule_create", { ...draft });
  },

  update(id: number, draft: RuleDraft): Promise<void> {
    return call("category_rule_update", { id, ...draft });
  },

  delete(id: number): Promise<void> {
    return call("category_rule_delete", { id });
  },

  /** What a rule written like this would catch, without saving it. */
  preview(
    kind: RuleKind,
    field: PatternField | null,
    value: string,
  ): Promise<RulePreview> {
    return call("category_rule_preview", { kind, field, value });
  },
};

/** Hints for writing a rule, drawn from the history. */
export const suggestion = {
  /** The apps and sites in the history, longest-tracked first. */
  sources(): Promise<SourceSuggestion[]> {
    return call("suggestion_sources");
  },

  /** The pages tracked on a domain, and the folders above them. */
  paths(domain: string): Promise<PathSuggestion[]> {
    return call("suggestion_paths", { domain });
  },
};

export const categorization = {
  reset(): Promise<CategorizationResetOutcome> {
    return call("categorization_reset");
  },

  /** Runs the AI over the uncategorized activities (all of them when `recheck`). */
  run(recheck = false, model?: string): Promise<AiCategorizeOutcome> {
    return call("categorization_run", { model, recheck });
  },

  status(): Promise<AiCategorizeStatus> {
    return call("categorization_status");
  },

  cancel(): Promise<void> {
    return call("categorization_cancel");
  },
};

export const aiReview = {
  list(
    page: number,
    pageSize: number,
    filter: ReviewFilter = {},
  ): Promise<AiReviewPage> {
    return call("ai_review_list", { page, pageSize, filter });
  },

  listGroups(
    page: number,
    pageSize: number,
    filter: ReviewFilter = {},
  ): Promise<AiReviewGroupPage> {
    return call("ai_review_list_groups", { page, pageSize, filter });
  },

  accept(activityIds: number[]): Promise<void> {
    return call("ai_review_accept", { activityIds });
  },

  feedbackStats(): Promise<FeedbackStats> {
    return call("ai_review_feedback_stats");
  },
};

export const aiModel = {
  isLoaded(): Promise<boolean> {
    return call("ai_model_is_loaded");
  },

  unload(): Promise<void> {
    return call("ai_model_unload");
  },

  isInstalled(model: string): Promise<boolean> {
    return call("ai_model_is_installed", { model });
  },

  listInstalled(): Promise<string[]> {
    return call("ai_model_list_installed");
  },

  /** Resolves `false` when the user stopped the download. */
  download(model: string): Promise<boolean> {
    return call("ai_model_download", { model });
  },

  cancelDownload(): Promise<void> {
    return call("ai_model_cancel_download");
  },

  downloadStatus(): Promise<ModelDownloadStatus> {
    return call("ai_model_download_status");
  },
};

export const settings = {
  get(): Promise<Settings> {
    return call("settings_get");
  },

  /** Resolves to the settings as saved. */
  setMinSessionSeconds(seconds: number): Promise<Settings> {
    return call("settings_set_min_session_seconds", { seconds });
  },
};

export const autostart = {
  isEnabled(): Promise<boolean> {
    return checkAutostartEnabled();
  },

  setEnabled(enabled: boolean): Promise<void> {
    return enabled ? enableAutostart() : disableAutostart();
  },
};
