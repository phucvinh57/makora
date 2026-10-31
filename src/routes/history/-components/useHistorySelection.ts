import { useCallback } from "react";
import { useScopedState } from "@/hooks/useScopedState";
import type { SessionRow } from "@/types";

const EMPTY_IDS: ReadonlySet<number> = new Set();

/**
 * The rows ticked for a bulk change and the category picked for them. Both start over when
 * `scope` (the page and filters) changes, and only rows still on screen count as selected.
 */
export function useHistorySelection(scope: string, rows: SessionRow[]) {
  const [selectedIds, updateSelectedIds] = useScopedState(scope, EMPTY_IDS);
  const [bulkCategory, updateBulkCategory] = useScopedState(scope, "");

  const clear = useCallback(() => {
    updateSelectedIds(() => EMPTY_IDS);
    updateBulkCategory(() => "");
  }, [updateSelectedIds, updateBulkCategory]);

  const toggleRow = useCallback(
    (id: number, checked: boolean) =>
      updateSelectedIds((prev) => {
        const next = new Set(prev);
        if (checked) next.add(id);
        else next.delete(id);
        return next;
      }),
    [updateSelectedIds],
  );

  const toggleAllVisible = useCallback(
    (checked: boolean) =>
      updateSelectedIds(() =>
        checked ? new Set(rows.map((r) => r.id)) : EMPTY_IDS,
      ),
    [updateSelectedIds, rows],
  );

  const setBulkCategory = useCallback(
    (value: string) => updateBulkCategory(() => value),
    [updateBulkCategory],
  );

  return {
    selectedIds,
    /** The selection narrowed to the rows on screen, which a refetch can change. */
    selectedRowIds: rows.filter((r) => selectedIds.has(r.id)).map((r) => r.id),
    bulkCategory,
    setBulkCategory,
    toggleRow,
    toggleAllVisible,
    clear,
  };
}
