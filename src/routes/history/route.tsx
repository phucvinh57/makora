import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  acceptAiLabels,
  listCategories,
  listSessions,
  setSessionRowCategory,
  setSessionsCategory,
} from "@/api";
import { categoryPath } from "@/categories";
import type { SessionRow } from "@/types";
import { BulkActionsBar } from "./-components/BulkActionsBar";
import {
  ALL_CATEGORIES,
  ALL_SOURCES,
  PAGE_SIZE,
  SEARCH_DEBOUNCE_MS,
  UNCATEGORIZED,
} from "./-components/constants";
import { HistoryHeader } from "./-components/HistoryHeader";
import { HistoryPagination } from "./-components/HistoryPagination";
import { SessionsTable } from "./-components/SessionsTable";
import {
  ALL_TIME,
  resolveRange,
  type TimeRange,
  type TimeRangeKind,
} from "./-components/timeRange";

const RANGE_KINDS: TimeRangeKind[] = [
  "all",
  "today",
  "yesterday",
  "3d",
  "7d",
  "custom",
];

function validateSearch(search: Partial<TimeRange>): TimeRange {
  const kind = RANGE_KINDS.find((k) => k === search.kind);
  if (!kind || kind === "all") return ALL_TIME;
  if (kind === "custom") {
    const from = typeof search.from === "string" ? search.from : undefined;
    const to = typeof search.to === "string" ? search.to : undefined;
    return from || to ? { kind, from, to } : ALL_TIME;
  }
  return { kind };
}

export const Route = createFileRoute("/history")({
  component: HistoryPage,
  validateSearch,
});

function HistoryPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const range = Route.useSearch();

  const [page, setPage] = useState(0);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] = useState(ALL_CATEGORIES);
  const [sourceFilter, setSourceFilter] = useState(ALL_SOURCES);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [bulkCategory, setBulkCategory] = useState("");

  function clearSelection() {
    setSelectedIds(new Set());
    setBulkCategory("");
  }

  useEffect(() => {
    const handle = setTimeout(() => {
      setSearch(searchInput.trim());
      setPage(0);
      setSelectedIds(new Set());
      setBulkCategory("");
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [searchInput]);

  function handleCategoryFilterChange(value: string) {
    setCategoryFilter(value);
    setPage(0);
    clearSelection();
  }

  function handleSourceFilterChange(value: string) {
    setSourceFilter(value);
    setPage(0);
    clearSelection();
  }

  function handleRangeChange(next: TimeRange) {
    navigate({ search: next });
    setPage(0);
    clearSelection();
  }

  const { start, end } = useMemo(() => resolveRange(range), [range]);

  const sessionsQuery = useQuery({
    queryKey: [
      "sessions",
      page,
      PAGE_SIZE,
      search,
      categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
      sourceFilter === ALL_SOURCES ? undefined : sourceFilter,
      start,
      end,
    ],
    queryFn: () =>
      listSessions(
        page,
        PAGE_SIZE,
        search || undefined,
        categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
        sourceFilter === ALL_SOURCES ? undefined : "manual",
        start,
        end,
      ),
    placeholderData: keepPreviousData,
  });

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const rows = sessionsQuery.data?.rows ?? [];
  const total = sessionsQuery.data?.total ?? 0;
  const categories = categoriesQuery.data ?? [];
  const loading = sessionsQuery.isFetching;
  const error =
    (sessionsQuery.error && String(sessionsQuery.error)) ||
    (categoriesQuery.error && String(categoriesQuery.error)) ||
    null;

  function invalidateSessions() {
    queryClient.invalidateQueries({ queryKey: ["sessions"] });
    // The statistics page buckets the same rows, so it goes stale on the same edits.
    queryClient.invalidateQueries({ queryKey: ["activityStats"] });
  }

  const categoryChangeMutation = useMutation({
    mutationFn: async ({ row, value }: { row: SessionRow; value: string }) => {
      const categoryId = value === UNCATEGORIZED ? null : Number(value);
      await setSessionRowCategory(row.id, categoryId);
      return { row, categoryId };
    },
    onSuccess: ({ row, categoryId }) => {
      invalidateSessions();
      if (categoryId === null) {
        toast.success(
          row.category_source === "ai"
            ? "Rejected the AI suggestion"
            : "Cleared this activity's category",
        );
        return;
      }
      const category = categories.find((c) => c.id === categoryId);
      const categoryName = category
        ? categoryPath(category, categories)
        : "category";
      toast.success(`Set to ${categoryName}`, {
        description: row.title ?? row.url ?? row.app_name,
      });
    },
    onError: (err) => toast.error(String(err)),
  });

  const acceptAiMutation = useMutation({
    mutationFn: (row: SessionRow) => acceptAiLabels([row.activity_id]),
    onSuccess: invalidateSessions,
    onError: (err) => toast.error(String(err)),
  });

  const bulkApplyMutation = useMutation({
    mutationFn: () =>
      setSessionsCategory([...selectedIds], Number(bulkCategory)),
    onSuccess: () => {
      const count = selectedIds.size;
      clearSelection();
      invalidateSessions();
      toast.success(`Categorized ${count} activities`);
    },
    onError: (err) => toast.error(String(err)),
  });

  function toggleRow(id: number, checked: boolean) {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  }

  function toggleAllVisible(checked: boolean) {
    setSelectedIds(checked ? new Set(rows.map((r) => r.id)) : new Set());
  }

  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <div className="flex h-full flex-col box-border">
      <HistoryHeader
        searchInput={searchInput}
        onSearchInputChange={setSearchInput}
        categoryFilter={categoryFilter}
        onCategoryFilterChange={handleCategoryFilterChange}
        sourceFilter={sourceFilter}
        onSourceFilterChange={handleSourceFilterChange}
        categories={categories}
        range={range}
        onRangeChange={handleRangeChange}
        loading={loading}
        onRefresh={() => sessionsQuery.refetch()}
      />

      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load sessions: {error}
        </div>
      )}

      {selectedIds.size > 0 && (
        <BulkActionsBar
          selectedCount={selectedIds.size}
          categories={categories}
          bulkCategory={bulkCategory}
          onBulkCategoryChange={setBulkCategory}
          onApply={() => bulkApplyMutation.mutate()}
          applyPending={bulkApplyMutation.isPending}
          onClear={clearSelection}
        />
      )}

      <SessionsTable
        rows={rows}
        categories={categories}
        loading={loading}
        search={search}
        categoryFilter={categoryFilter}
        sourceFilter={sourceFilter}
        selectedIds={selectedIds}
        onToggleRow={toggleRow}
        onToggleAllVisible={toggleAllVisible}
        onRowCategoryChange={(row, value) =>
          categoryChangeMutation.mutate({ row, value })
        }
        onAcceptAiLabel={(row) => acceptAiMutation.mutate(row)}
      />

      <HistoryPagination
        page={page}
        pageCount={pageCount}
        total={total}
        onPrev={() => setPage((p) => p - 1)}
        onNext={() => setPage((p) => p + 1)}
      />
    </div>
  );
}
