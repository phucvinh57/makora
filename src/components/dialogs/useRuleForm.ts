import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { categoryRule } from "@/api";
import { invalidateCategorizationQueries } from "@/queries";
import type { CategoryRuleView, PatternField, RuleKind } from "@/types";

export interface RuleFormValues {
  kind: RuleKind;
  field: PatternField;
  value: string;
  /** Only for a page: what follows the domain, e.g. `/acme/api`. */
  path: string;
}

interface Row extends RuleFormValues {
  id: number;
}

interface SaveResult {
  saved: number[];
  /** The row that stopped the save, if one did; the rows after it were not tried. */
  failed: { id: number; message: string } | null;
}

function emptyForm(like?: RuleFormValues): RuleFormValues {
  return {
    kind: like?.kind ?? "site",
    field: like?.field ?? "url",
    value: "",
    path: "",
  };
}

function formOf(rule: CategoryRuleView): RuleFormValues {
  const slash = rule.value.indexOf("/");
  const isPage = rule.kind === "page" && slash >= 0;
  return {
    kind: rule.kind,
    field: rule.field ?? "url",
    value: isPage ? rule.value.slice(0, slash) : rule.value,
    path: isPage ? rule.value.slice(slash) : "",
  };
}

export function matchValue(form: RuleFormValues): string {
  if (form.kind !== "page") return form.value.trim();
  const domain = form.value.trim();
  const path = form.path.trim();
  if (!domain || !path) return domain;
  return `${domain}${path.startsWith("/") ? "" : "/"}${path}`;
}

export function fieldOf(form: RuleFormValues): PatternField | null {
  return form.kind === "pattern" ? form.field : null;
}

/**
 * The rows of the rule dialog and saving them. Editing has one row; adding can have many, all
 * filed under one category.
 */
export function useRuleForm(
  rule: CategoryRuleView | null,
  onClose: () => void,
) {
  const queryClient = useQueryClient();
  const [categoryId, setCategoryId] = useState<number | null>(
    rule?.category_id ?? null,
  );
  const nextId = useRef(1);
  const [rows, setRows] = useState<Row[]>(() => [
    { id: 0, ...(rule ? formOf(rule) : emptyForm()) },
  ]);
  // Only rows added after the first take focus: the dialog opens on the category.
  const [focusedId, setFocusedId] = useState<number | null>(null);

  function changeRow(id: number, patch: Partial<RuleFormValues>) {
    setRows((prev) =>
      prev.map((row) => (row.id === id ? { ...row, ...patch } : row)),
    );
  }

  function addRow() {
    const id = nextId.current++;
    setRows((prev) => [...prev, { id, ...emptyForm(prev[prev.length - 1]) }]);
    setFocusedId(id);
  }

  function removeRow(id: number) {
    setRows((prev) => prev.filter((row) => row.id !== id));
  }

  // A blank row is one still waiting to be filled, so it is skipped rather than blocking.
  const filled = rows.filter((row) => matchValue(row) !== "");

  const saveMutation = useMutation({
    mutationFn: async (category: number): Promise<SaveResult> => {
      const draftOf = (row: Row) => ({
        kind: row.kind,
        field: fieldOf(row),
        value: matchValue(row),
        categoryId: category,
      });
      if (rule) {
        await categoryRule.update(rule.id, draftOf(filled[0]));
        return { saved: [filled[0].id], failed: null };
      }
      const saved: number[] = [];
      for (const row of filled) {
        try {
          await categoryRule.create(draftOf(row));
          saved.push(row.id);
        } catch (err) {
          return { saved, failed: { id: row.id, message: String(err) } };
        }
      }
      return { saved, failed: null };
    },
    onSuccess: (result) => {
      // Rules saved before a failure stay saved, so they leave the form: saving again must
      // not add them twice.
      if (result.failed) {
        setRows((prev) => prev.filter((row) => !result.saved.includes(row.id)));
      } else {
        onClose();
      }
      return invalidateCategorizationQueries(queryClient);
    },
  });

  return {
    categoryId,
    setCategoryId,
    rows,
    focusedId,
    changeRow,
    addRow,
    removeRow,
    filledCount: filled.length,
    canSave:
      filled.length > 0 && categoryId !== null && !saveMutation.isPending,
    save: () => {
      if (categoryId !== null && filled.length > 0)
        saveMutation.mutate(categoryId);
    },
    failed: saveMutation.data?.failed ?? null,
    error: saveMutation.error,
  };
}
