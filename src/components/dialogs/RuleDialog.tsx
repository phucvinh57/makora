import { useQuery } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
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
import { queryKeys, sourceSuggestionsQueryOptions } from "@/queries";
import {
  PATTERN_FIELD_LABELS,
  PATTERN_FIELDS,
  parsePatternField,
  parseRuleKind,
  RULE_KIND_LABELS,
  RULE_KINDS,
  RULE_VALUE_PLACEHOLDERS,
} from "@/rules";
import type { CategoryRule } from "@/types";
import {
  fieldOf,
  matchValue,
  type RuleFormValues,
  useRuleForm,
} from "./useRuleForm";

const PREVIEW_DEBOUNCE_MS = 300;

function sourceHint(name: string, key: string, seconds: number): string {
  const label = name && name !== key ? `${name} · ` : "";
  return `${label}${formatSeconds(seconds)}`;
}

function RuleFields({
  form,
  onChange,
  onRemove,
  error,
  autoFocus,
}: {
  form: RuleFormValues;
  onChange: (patch: Partial<RuleFormValues>) => void;
  /** Absent when this is the only rule and cannot be removed. */
  onRemove?: () => void;
  error?: string;
  autoFocus?: boolean;
}) {
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

  function change(patch: Partial<RuleFormValues>) {
    const next = { ...form, ...patch };
    onChange(patch);
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
    queryKey: [...queryKeys.pathSuggestions, domain],
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
    queryKey: [
      ...queryKeys.rulePreview,
      previewed.kind,
      previewed.field,
      previewed.value,
    ],
    queryFn: () =>
      categoryRule.preview(previewed.kind, previewed.field, previewed.value),
    enabled: previewed.value !== "",
    retry: false,
  });
  const preview = previewQuery.data;

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center gap-2">
        <Select
          value={form.kind}
          onValueChange={(value) => {
            const kind = parseRuleKind(value);
            if (kind) change({ kind });
          }}
          items={RULE_KIND_LABELS}
        >
          <SelectTrigger className="min-w-0 flex-1" aria-label="What to match">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {RULE_KINDS.map((kind) => (
              <SelectItem key={kind} value={kind}>
                {RULE_KIND_LABELS[kind]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {form.kind === "pattern" && (
          <Select
            value={form.field}
            onValueChange={(value) => {
              const field = parsePatternField(value);
              if (field) change({ field });
            }}
            items={PATTERN_FIELD_LABELS}
          >
            <SelectTrigger
              className="min-w-0 flex-1"
              aria-label="Match against"
            >
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

        {onRemove && (
          <Button
            type="button"
            size="icon-xs"
            variant="ghost"
            aria-label="Remove rule"
            title="Remove rule"
            onClick={onRemove}
          >
            <X />
          </Button>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        {form.kind === "pattern" ? (
          <Input
            type="text"
            value={form.value}
            onChange={(event) => change({ value: event.target.value })}
            placeholder={RULE_VALUE_PLACEHOLDERS.pattern}
            aria-label="Pattern"
            autoFocus={autoFocus}
          />
        ) : (
          <SuggestInput
            value={form.value}
            onValueChange={(value) => change({ value })}
            suggestions={sourceSuggestions}
            placeholder={RULE_VALUE_PLACEHOLDERS[form.kind]}
            ariaLabel={form.kind === "app" ? "App" : "Website"}
            autoFocus={autoFocus}
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
              : "Pick from what you have tracked, or type your own."}
        </p>
      </div>

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

      {error && <p className="m-0 text-xs text-destructive">{error}</p>}
    </div>
  );
}

/**
 * The form is its own component because the dialog unmounts its content on close: the fields
 * and the mutation's error start fresh every time it opens.
 *
 * The category comes first and every rule written below it files there, so a batch of rules
 * for one category goes in with a single pick. Editing changes one rule, so it has one row.
 */
function RuleForm({
  rule,
  onClose,
}: {
  rule: CategoryRule | null;
  onClose: () => void;
}) {
  const { categories } = useCategories();
  const picker = useCategoryPicker(categories);
  const form = useRuleForm(rule, onClose);

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        form.save();
      }}
      className="flex min-w-0 flex-col gap-4"
    >
      <DialogHeader>
        <DialogTitle>{rule ? "Edit rule" : "Add rules"}</DialogTitle>
        <DialogDescription>
          {rule
            ? "Every activity that matches goes to the category you choose, including what you have already tracked."
            : "Choose a category, then add every rule that belongs in it. Every activity that matches goes there, including what you have already tracked."}{" "}
          Avoid multi-purpose sites or apps: every matching activity goes to the
          category.
        </DialogDescription>
      </DialogHeader>

      <CreatableCategoryCombobox
        picker={picker}
        value={form.categoryId === null ? null : String(form.categoryId)}
        onValueChange={(value) => form.setCategoryId(Number(value))}
        className="w-full"
      />

      <div className="-mx-1 flex max-h-[50vh] min-w-0 flex-col gap-3 overflow-x-hidden overflow-y-auto px-1">
        {form.rows.map((row) => {
          const fields = (
            <RuleFields
              key={row.id}
              form={row}
              onChange={(patch) => form.changeRow(row.id, patch)}
              onRemove={
                rule || form.rows.length === 1
                  ? undefined
                  : () => form.removeRow(row.id)
              }
              error={
                form.failed?.id === row.id ? form.failed.message : undefined
              }
              autoFocus={row.id === form.focusedId}
            />
          );
          return rule ? (
            fields
          ) : (
            <div key={row.id} className="rounded-lg border p-3">
              {fields}
            </div>
          );
        })}
      </div>

      {!rule && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="self-start"
          onClick={form.addRow}
        >
          <Plus />
          Add another rule
        </Button>
      )}

      {form.error != null && (
        <p className="m-0 text-xs text-destructive">{String(form.error)}</p>
      )}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" disabled={!form.canSave}>
          {rule
            ? "Save rule"
            : form.filledCount > 1
              ? `Add ${form.filledCount} rules`
              : "Add rule"}
        </Button>
      </DialogFooter>
    </form>
  );
}

export interface RuleDialogData {
  rule: CategoryRule | null;
}

export function RuleDialog() {
  const { open, setOpen, data } = useDialog("rule");

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <RuleForm rule={data?.rule ?? null} onClose={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}
