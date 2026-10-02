import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { categoryRule, suggestion as suggestionApi } from "@/api";
import { useCategoryPicker } from "@/components/CategoryOptions";
import { CreatableCategoryCombobox } from "@/components/CreatableCategoryCombobox";
import { type Suggestion, SuggestInput } from "@/components/SuggestInput";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useCategories } from "@/hooks/useCategories";
import { useDebouncedCallback } from "@/hooks/useDebouncedCallback";
import { useDialog } from "@/hooks/useDialog";
import { formatSeconds } from "@/duration";
import {
  invalidateCategorizationQueries,
  sourceSuggestionsQueryOptions,
} from "@/queries";
import {
  PATTERN_FIELD_LABELS,
  RULE_KIND_LABELS,
  RULE_VALUE_PLACEHOLDERS,
} from "@/rules";
import type { CategoryRule, PatternField, RuleKind } from "@/types";

const PREVIEW_DEBOUNCE_MS = 300;

const KINDS: RuleKind[] = ["app", "site", "page", "pattern"];
const PATTERN_FIELDS: PatternField[] = ["url", "title", "app"];

const KIND_ITEMS = Object.fromEntries(
  KINDS.map((kind) => [kind, RULE_KIND_LABELS[kind]]),
);
const FIELD_ITEMS = Object.fromEntries(
  PATTERN_FIELDS.map((field) => [field, PATTERN_FIELD_LABELS[field]]),
);

interface Form {
  kind: RuleKind;
  field: PatternField;
  /** The app, domain or glob; for a page, just the domain. */
  value: string;
  /** Only for a page: what follows the domain, e.g. `/acme/api`. */
  path: string;
  categoryId: number | null;
}

function formOf(rule: CategoryRule | null): Form {
  if (!rule) {
    return {
      kind: "site",
      field: "url",
      value: "",
      path: "",
      categoryId: null,
    };
  }
  const slash = rule.value.indexOf("/");
  const isPage = rule.kind === "page" && slash >= 0;
  return {
    kind: rule.kind,
    field: rule.field ?? "url",
    value: isPage ? rule.value.slice(0, slash) : rule.value,
    path: isPage ? rule.value.slice(slash) : "",
    categoryId: rule.category_id,
  };
}

/** What the backend is asked to match: for a page, the domain with its path. */
function matchValue(form: Form): string {
  if (form.kind !== "page") return form.value.trim();
  const domain = form.value.trim();
  const path = form.path.trim();
  if (!domain || !path) return domain;
  return `${domain}${path.startsWith("/") ? "" : "/"}${path}`;
}

function fieldOf(form: Form): PatternField | null {
  return form.kind === "pattern" ? form.field : null;
}

function sourceHint(name: string, key: string, seconds: number): string {
  const label = name && name !== key ? `${name} · ` : "";
  return `${label}${formatSeconds(seconds)}`;
}

/**
 * The form is its own component because the dialog unmounts its content on close: the fields
 * and the mutation's error start fresh every time it opens.
 */
function RuleForm({
  rule,
  onClose,
}: {
  rule: CategoryRule | null;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const { categories } = useCategories();
  const picker = useCategoryPicker(categories);
  const [form, setForm] = useState(() => formOf(rule));
  // The preview follows what was typed a moment ago, so it is not asked on every keystroke.
  const [previewed, setPreviewed] = useState(() => ({
    kind: form.kind,
    field: fieldOf(form),
    value: matchValue(form),
  }));
  const schedulePreview = useDebouncedCallback(
    setPreviewed,
    PREVIEW_DEBOUNCE_MS,
  );

  function change(patch: Partial<Form>) {
    const next = { ...form, ...patch };
    setForm(next);
    schedulePreview({
      kind: next.kind,
      field: fieldOf(next),
      value: matchValue(next),
    });
  }

  const sourcesQuery = useQuery(sourceSuggestionsQueryOptions);
  const sourceSuggestions = useMemo<Suggestion[]>(() => {
    const wanted = form.kind === "app" ? "exe" : "domain";
    return (sourcesQuery.data ?? [])
      .filter((source) => source.kind === wanted)
      .map((source) => ({
        value: source.key,
        hint: sourceHint(source.name, source.key, source.total_seconds),
        search: source.name,
      }));
  }, [sourcesQuery.data, form.kind]);

  const domain = form.value.trim().toLowerCase();
  const pathsQuery = useQuery({
    queryKey: ["pathSuggestions", domain],
    queryFn: () => suggestionApi.paths(domain),
    enabled: form.kind === "page" && domain !== "",
    staleTime: 60_000,
  });
  const pathSuggestions = useMemo<Suggestion[]>(
    () =>
      (pathsQuery.data ?? []).map((page) => ({
        value: page.path,
        hint: formatSeconds(page.total_seconds),
      })),
    [pathsQuery.data],
  );

  const previewQuery = useQuery({
    queryKey: ["rulePreview", previewed.kind, previewed.field, previewed.value],
    queryFn: () =>
      categoryRule.preview(previewed.kind, previewed.field, previewed.value),
    enabled: previewed.value !== "",
    retry: false,
  });
  const preview = previewQuery.data;

  const saveMutation = useMutation({
    mutationFn: async (categoryId: number) => {
      const draft = {
        kind: form.kind,
        field: fieldOf(form),
        value: matchValue(form),
        categoryId,
      };
      if (rule) await categoryRule.update(rule.id, draft);
      else await categoryRule.create(draft);
    },
    onSuccess: () => {
      onClose();
      return invalidateCategorizationQueries(queryClient);
    },
  });

  const canSave = matchValue(form) !== "" && form.categoryId !== null;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (canSave && form.categoryId !== null) {
          saveMutation.mutate(form.categoryId);
        }
      }}
      className="flex flex-col gap-4"
    >
      <DialogHeader>
        <DialogTitle>{rule ? "Edit rule" : "Add rule"}</DialogTitle>
        <DialogDescription>
          Every activity that matches goes to the category you choose, including
          what you have already tracked.
        </DialogDescription>
      </DialogHeader>

      <Select
        value={form.kind}
        onValueChange={(value) => value && change({ kind: value as RuleKind })}
        items={KIND_ITEMS}
      >
        <SelectTrigger className="w-full" aria-label="What to match">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {KINDS.map((kind) => (
            <SelectItem key={kind} value={kind}>
              {RULE_KIND_LABELS[kind]}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {form.kind === "pattern" && (
        <Select
          value={form.field}
          onValueChange={(value) =>
            value && change({ field: value as PatternField })
          }
          items={FIELD_ITEMS}
        >
          <SelectTrigger className="w-full" aria-label="Match against">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PATTERN_FIELDS.map((field) => (
              <SelectItem key={field} value={field}>
                {PATTERN_FIELD_LABELS[field]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}

      <div className="flex flex-col gap-1.5">
        {form.kind === "pattern" ? (
          <Input
            type="text"
            value={form.value}
            onChange={(event) => change({ value: event.target.value })}
            placeholder={RULE_VALUE_PLACEHOLDERS.pattern}
            aria-label="Pattern"
            autoFocus
          />
        ) : (
          <SuggestInput
            value={form.value}
            onValueChange={(value) => change({ value })}
            suggestions={sourceSuggestions}
            placeholder={RULE_VALUE_PLACEHOLDERS[form.kind]}
            ariaLabel={form.kind === "app" ? "App" : "Website"}
          />
        )}
        {form.kind === "page" && (
          <SuggestInput
            value={form.path}
            onValueChange={(path) => change({ path })}
            suggestions={pathSuggestions}
            placeholder="/your-org/your-repo"
            ariaLabel="Path"
          />
        )}
        <p className="m-0 text-xs text-muted-foreground">
          {form.kind === "pattern"
            ? "* matches any text and ? matches one character, ignoring case."
            : form.kind === "page"
              ? "Covers this page and everything below it."
              : "Pick from what you have tracked, or type your own."}{" "}
          Avoid multi-purpose sites or apps: every matching activity goes to
          this category.
        </p>
      </div>

      <CreatableCategoryCombobox
        picker={picker}
        value={form.categoryId === null ? null : String(form.categoryId)}
        onValueChange={(value) => change({ categoryId: Number(value) })}
        className="w-full"
      />

      <div className="min-h-10 rounded-md border bg-muted/30 px-3 py-2 text-xs">
        {previewed.value === "" ? (
          <span className="text-muted-foreground">
            Type a value to see what it would match.
          </span>
        ) : previewQuery.isError ? (
          <span className="text-muted-foreground">
            {String(previewQuery.error)}
          </span>
        ) : !preview ? (
          <span className="text-muted-foreground">Checking your history…</span>
        ) : preview.activity_count === 0 ? (
          <span className="text-muted-foreground">
            Nothing tracked so far matches. It will apply to new activity.
          </span>
        ) : (
          <div className="flex flex-col gap-1">
            <span>
              Matches {preview.activity_count} activit
              {preview.activity_count === 1 ? "y" : "ies"} ·{" "}
              {formatSeconds(preview.total_seconds)}
              {preview.pinned_count > 0 &&
                ` (${preview.pinned_count} pinned by you will not change)`}
            </span>
            <ul className="m-0 list-none p-0 text-muted-foreground">
              {preview.samples.map((sample) => (
                <li key={sample} className="truncate">
                  {sample}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {saveMutation.isError && (
        <p className="m-0 text-xs text-destructive">
          {String(saveMutation.error)}
        </p>
      )}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" disabled={!canSave || saveMutation.isPending}>
          {rule ? "Save rule" : "Add rule"}
        </Button>
      </DialogFooter>
    </form>
  );
}

export interface RuleDialogData {
  /** The rule being edited; `null` for a new one. */
  rule: CategoryRule | null;
}

export function RuleDialog() {
  const { open, setOpen, data } = useDialog<RuleDialogData>("rule");

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <RuleForm rule={data?.rule ?? null} onClose={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}
