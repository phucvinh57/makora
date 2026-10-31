import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { aiReview, session } from "@/api";
import { useCategoryPicker } from "@/components/CategoryOptions";
import { PaginationControls } from "@/components/PaginationControls";
import { useCategories } from "@/hooks/useCategories";
import { useDebouncedCallback } from "@/hooks/useDebouncedCallback";
import { useScopedState } from "@/hooks/useScopedState";
import { parseOneOf } from "@/lib/utils";
import { invalidateCategorizationQueries, queryKeys } from "@/queries";
import type { HistorySourceFilter, SessionRow } from "@/types";
import { BulkActionsBar } from "./-components/BulkActionsBar";
import {
  ALL_CATEGORIES,
  ALL_SOURCES,
  PAGE_SIZE,
  SEARCH_DEBOUNCE_MS,
  UNCATEGORIZED,
} from "./-components/constants";
import { type HistoryFilters, parseSourceFilter } from "./-components/filters";
import { HistoryHeader } from "./-components/HistoryHeader";
import { SessionsTable } from "./-components/SessionsTable";
import {
  ALL_TIME,
  isRangeActive,
  resolveRange,
  type TimeRange,
  type TimeRangeKind,
} from "./-components/timeRange";

const RANGE_KINDS: readonly TimeRangeKind[] = [
  "all",
  "today",
  "yesterday",
  "3d",
  "7d",
  "custom",
];

interface HistorySearch extends TimeRange {
  q?: string;
  category?: string;
  source?: HistorySourceFilter;
  /** One-based, and absent for the first page. */
  page?: number;
}

function validateRange(search: Record<string, unknown>): TimeRange {
  const kind = parseOneOf(RANGE_KINDS, search.kind);
  if (!kind || kind === "all") return ALL_TIME;
  if (kind === "custom") {
    const from = typeof search.from === "string" ? search.from : undefined;
    const to = typeof search.to === "string" ? search.to : undefined;
    return from || to ? { kind, from, to } : ALL_TIME;
  }
  return { kind };
}

function validateSearch(search: Record<string, unknown>): HistorySearch {
  return {
    ...validateRange(search),
    q: typeof search.q === "string" && search.q ? search.q : undefined,
    category:
      typeof search.category === "string" && search.category !== ALL_CATEGORIES
        ? search.category
        : undefined,
    source: parseSourceFilter(search.source),
    page:
      typeof search.page === "number" &&
      Number.isInteger(search.page) &&
      search.page > 1
        ? search.page
        : undefined,
  };
}

export const Route = createFileRoute("/history")({
  component: HistoryPage,
  validateSearch,
});

const EMPTY_IDS: ReadonlySet<number> = new Set();
const NO_ROWS: SessionRow[] = [];

function HistoryPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const search = Route.useSearch();

  const query = search.q ?? "";
  const page = (search.page ?? 1) - 1;
  const filters: HistoryFilters = {
    category: search.category ?? ALL_CATEGORIES,
    source: search.source ?? ALL_SOURCES,
    range: { kind: search.kind, from: search.from, to: search.to },
  };

  const changeSearch = useCallback(
    (patch: Partial<HistorySearch>, replace = false) =>
      navigate({
        search: (prev) => ({ ...prev, ...patch, page: undefined }),
        replace,
      }),
    [navigate],
  );
  const goToPage = useCallback(
    (next: number) =>
      navigate({
        search: (prev) => ({ ...prev, page: next > 0 ? next + 1 : undefined }),
      }),
    [navigate],
  );

  function handleFiltersChange(patch: Partial<HistoryFilters>) {
    const { category, source, range } = patch;
    changeSearch({
      ...(category !== undefined && {
        category: category === ALL_CATEGORIES ? undefined : category,
      }),
      ...(source !== undefined && {
        source: source === ALL_SOURCES ? undefined : source,
      }),
      // Spelled out so switching to a preset also drops a custom range's bounds.
      ...(range && { kind: range.kind, from: range.from, to: range.to }),
    });
  }

  const [searchInput, setSearchInput] = useState(query);
  const [syncedQuery, setSyncedQuery] = useState(query);
  if (query !== syncedQuery) {
    setSyncedQuery(query);
    if (query !== searchInput.trim()) setSearchInput(query);
  }
  const commitQuery = useDebouncedCallback(
    (text: string) => changeSearch({ q: text.trim() || undefined }, true),
    SEARCH_DEBOUNCE_MS,
  );
  function handleSearchInputChange(text: string) {
    setSearchInput(text);
    commitQuery(text);
  }

  // Selections belong to the page and filters they were made on.
  const scope = JSON.stringify([page, query, filters]);
  const [selectedIds, updateSelectedIds] = useScopedState(scope, EMPTY_IDS);
  const [bulkCategory, updateBulkCategory] = useScopedState(scope, "");

  const clearSelection = useCallback(() => {
    updateSelectedIds(() => EMPTY_IDS);
    updateBulkCategory(() => "");
  }, [updateSelectedIds, updateBulkCategory]);

  const sessionsQuery = useQuery({
    queryKey: [...queryKeys.sessions, { page, query, filters }],
    // The range is resolved per fetch, so "Today" is still today after midnight.
    queryFn: () => {
      const { start, end } = resolveRange(filters.range);
      return session.list({
        page,
        pageSize: PAGE_SIZE,
        search: query || undefined,
        category:
          filters.category === ALL_CATEGORIES ? undefined : filters.category,
        source: filters.source === ALL_SOURCES ? undefined : filters.source,
        start,
        end,
      });
    },
    placeholderData: keepPreviousData,
  });
  const { categories, error: categoriesError } = useCategories();
  const picker = useCategoryPicker(categories);

  const rows = sessionsQuery.data?.rows ?? NO_ROWS;
  const total = sessionsQuery.data?.total ?? 0;
  const loading = sessionsQuery.isFetching;
  const error = sessionsQuery.error ?? categoriesError;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  // A link or a reload can land past the last page; settle on the last one that exists.
  const settled = sessionsQuery.data && !sessionsQuery.isPlaceholderData;
  useEffect(() => {
    if (settled && page > pageCount - 1) {
      void navigate({
        search: (prev) => ({
          ...prev,
          page: pageCount > 1 ? pageCount : undefined,
        }),
        replace: true,
      });
    }
  }, [settled, page, pageCount, navigate]);

  const { mutate: changeRowCategory } = useMutation({
    mutationFn: async ({ row, value }: { row: SessionRow; value: string }) => {
      const categoryId = value === UNCATEGORIZED ? null : Number(value);
      await session.setRowCategory(row.id, categoryId);
      return { row, categoryId };
    },
    onSuccess: ({ row, categoryId }) => {
      if (categoryId === null) {
        if (row.category_source === "rule") {
          toast.info("This activity matches a rule", {
            description:
              "Pin another category to override it, or edit the rule on the Categories page.",
          });
        } else {
          toast.success(
            row.category_source === "ai"
              ? "Rejected the AI suggestion"
              : "Cleared this activity's category",
          );
        }
      } else {
        toast.success(
          `Set to ${picker.pathById.get(categoryId) ?? "category"}`,
          {
            description: row.title || row.url || row.app_name,
          },
        );
      }
      return invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });

  const { mutate: acceptAiLabel } = useMutation({
    mutationFn: (row: SessionRow) => aiReview.accept([row.activity_id]),
    onSuccess: () => invalidateCategorizationQueries(queryClient),
    onError: (err) => toast.error(String(err)),
  });

  const bulkApplyMutation = useMutation({
    mutationFn: ({ ids, categoryId }: { ids: number[]; categoryId: number }) =>
      session.setCategory(ids, categoryId),
    onSuccess: (_, { ids }) => {
      clearSelection();
      toast.success(`Categorized ${ids.length} activities`);
      return invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });

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

  // A row that left the page (refetch, or placeholder rows replaced) is no longer selected.
  const selectedRowIds = rows
    .filter((r) => selectedIds.has(r.id))
    .map((r) => r.id);

  const handleRowCategoryChange = useCallback(
    (row: SessionRow, value: string) => changeRowCategory({ row, value }),
    [changeRowCategory],
  );

  const filtered =
    query !== "" ||
    filters.category !== ALL_CATEGORIES ||
    filters.source !== ALL_SOURCES ||
    isRangeActive(filters.range);

  return (
    <div className="flex h-full flex-col box-border">
      <HistoryHeader
        searchInput={searchInput}
        onSearchInputChange={handleSearchInputChange}
        filters={filters}
        onFiltersChange={handleFiltersChange}
        picker={picker}
        loading={loading}
        onRefresh={() => sessionsQuery.refetch()}
      />

      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load sessions: {String(error)}
        </div>
      )}

      {selectedRowIds.length > 0 && (
        <BulkActionsBar
          selectedCount={selectedRowIds.length}
          picker={picker}
          bulkCategory={bulkCategory}
          onBulkCategoryChange={(value) => updateBulkCategory(() => value)}
          onApply={() =>
            bulkApplyMutation.mutate({
              ids: selectedRowIds,
              categoryId: Number(bulkCategory),
            })
          }
          applyPending={bulkApplyMutation.isPending}
          onClear={clearSelection}
        />
      )}

      <SessionsTable
        rows={rows}
        picker={picker}
        loading={loading}
        filtered={filtered}
        selectedIds={selectedIds}
        selectionDisabled={sessionsQuery.isPlaceholderData}
        onToggleRow={toggleRow}
        onToggleAllVisible={toggleAllVisible}
        onRowCategoryChange={handleRowCategoryChange}
        onAcceptAiLabel={acceptAiLabel}
      />

      <PaginationControls
        className="pt-3"
        page={page}
        pageCount={pageCount}
        label={`${total} sessions`}
        onPage={goToPage}
      />
    </div>
  );
}
