import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { categoryRule } from "@/api";
import { useCategoryPicker } from "@/components/CategoryOptions";
import { Badge } from "@/components/ui/badge";
import type { RuleDialogData } from "@/components/dialogs/RuleDialog";
import { Button } from "@/components/ui/button";
import { formatSeconds } from "@/duration";
import { useCategories } from "@/hooks/useCategories";
import { openDialog } from "@/hooks/useDialog";
import {
  categoryRulesQueryOptions,
  invalidateCategorizationQueries,
} from "@/queries";
import { PATTERN_FIELD_LABELS, RULE_KIND_LABELS } from "@/rules";
import type { CategoryRule } from "@/types";

function HowItWorks() {
  return (
    <div className="rounded-lg border bg-muted/30 px-4 py-3 text-sm">
      <h2 className="m-0 mb-1 text-sm font-medium">How categorization works</h2>
      <ul className="m-0 flex list-disc flex-col gap-1 pl-5 text-muted-foreground">
        <li>
          An activity's category comes from a pin you set, then from a rule,
          then from the AI. A rule never overrides a pin, and the AI skips
          anything a rule already covers.
        </li>
        <li>
          A rule files <em>every</em> activity it matches, past and future,
          under one category.
        </li>
        <li>
          So write rules for apps and sites you use for one purpose, like an
          editor, a code host or a project tracker. Leave multi-purpose ones to
          pins and the AI: AI chats such as claude.ai or chatgpt.com,
          youtube.com, search engines and social feeds mix work and leisure. A
          narrower rule such as a single channel or project page can still fit.
        </li>
      </ul>
    </div>
  );
}

export function RulesTab() {
  const queryClient = useQueryClient();
  const { categories } = useCategories();
  const picker = useCategoryPicker(categories);
  const rulesQuery = useQuery(categoryRulesQueryOptions);
  const rules = rulesQuery.data ?? [];
  const colorById = new Map(categories.map((c) => [c.id, c.color]));

  const deleteMutation = useMutation({
    mutationFn: (rule: CategoryRule) => categoryRule.delete(rule.id),
    onSuccess: () => {
      toast.success("Rule deleted");
      return invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });

  return (
    <div className="mt-4 flex flex-col gap-4">
      <HowItWorks />

      {rulesQuery.error && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {String(rulesQuery.error)}
        </div>
      )}

      <div className="flex justify-end">
        <Button
          size="sm"
          disabled={categories.length === 0}
          onClick={() => openDialog<RuleDialogData>("rule", { rule: null })}
        >
          Add rules
        </Button>
      </div>

      {categories.length === 0 && (
        <p className="m-0 text-sm text-muted-foreground">
          Add a category on the Taxonomy tab first; a rule files activity under
          one.
        </p>
      )}
      {!rulesQuery.isLoading && rules.length === 0 && categories.length > 0 && (
        <p className="m-0 text-sm text-muted-foreground">
          No rules yet. Use “Add rules” above to begin.
        </p>
      )}

      <ul className="m-0 flex list-none flex-col gap-2 p-0">
        {rules.map((rule) => (
          <li
            key={rule.id}
            className="flex items-center gap-3 rounded-lg border px-3 py-2"
          >
            <Badge variant="secondary">
              {rule.kind === "pattern"
                ? PATTERN_FIELD_LABELS[rule.field ?? "url"]
                : RULE_KIND_LABELS[rule.kind]}
            </Badge>
            <span className="min-w-0 flex-1 truncate font-mono text-sm">
              {rule.value}
            </span>
            <span className="flex min-w-0 shrink-0 items-center gap-1.5 text-sm">
              <span
                className="size-2 shrink-0 rounded-full"
                style={{
                  backgroundColor:
                    colorById.get(rule.category_id) ??
                    "var(--muted-foreground)",
                }}
              />
              <span className="truncate">
                {picker.pathById.get(rule.category_id) ?? "Unknown category"}
              </span>
            </span>
            <span className="w-32 shrink-0 text-right text-xs text-muted-foreground">
              {rule.activity_count} activit
              {rule.activity_count === 1 ? "y" : "ies"} ·{" "}
              {formatSeconds(rule.total_seconds)}
            </span>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label="Edit rule"
              title="Edit rule"
              onClick={() => openDialog<RuleDialogData>("rule", { rule })}
            >
              <Pencil />
            </Button>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label="Delete rule"
              title="Delete rule"
              disabled={deleteMutation.isPending}
              onClick={() => deleteMutation.mutate(rule)}
            >
              <Trash2 />
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
