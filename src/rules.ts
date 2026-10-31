import { parseOneOf } from "@/lib/utils";
import type { PatternField, RuleKind } from "./types";

export const RULE_KINDS: readonly RuleKind[] = [
  "app",
  "site",
  "page",
  "pattern",
];
export const PATTERN_FIELDS: readonly PatternField[] = ["url", "title", "app"];

export function parseRuleKind(value: unknown): RuleKind | undefined {
  return parseOneOf(RULE_KINDS, value);
}

export function parsePatternField(value: unknown): PatternField | undefined {
  return parseOneOf(PATTERN_FIELDS, value);
}

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
