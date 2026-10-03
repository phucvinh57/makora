import type { PatternField, RuleKind } from "./types";

export const RULE_KIND_LABELS: Record<RuleKind, string> = {
  app: "App",
  site: "Website",
  page: "Page",
  pattern: "Pattern",
};

export const PATTERN_FIELD_LABELS: Record<PatternField, string> = {
  url: "URL",
  title: "Window title",
  app: "App or site name",
};

export const RULE_VALUE_PLACEHOLDERS: Record<RuleKind, string> = {
  app: "code",
  site: "github.com",
  page: "github.com/your-org",
  pattern: "*standup*",
};

/** How a rule reads in a list: `Title matches *standup*`, `Website github.com`. */
export function describeRule(
  kind: RuleKind,
  field: PatternField | null,
  value: string,
): string {
  if (kind === "pattern") {
    return `${PATTERN_FIELD_LABELS[field ?? "url"]} matches ${value}`;
  }
  return `${RULE_KIND_LABELS[kind]} ${value}`;
}
