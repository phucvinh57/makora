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
  getWorkSession,
  listCategories,
  listSessions,
  recategorizeAll,
  setSessionRowCategory,
  setSessionsCategory,
} from "@/api";
import { categoryPath } from "@/categories";
import type { SessionRow } from "@/types";
import { BulkActionsBar } from "./-components/BulkActionsBar";
import {
  ALL_CATEGORIES,
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
  "session",
];

function validateSearch(search: Partial<TimeRange>): TimeRange {
  const kind = RANGE_KINDS.find((k) => k === search.kind);
  if (!kind || kind === "all") return ALL_TIME;
  if (kind === "session") {
    const sessionId = Number(search.sessionId);
    return Number.isInteger(sessionId) ? { kind, sessionId } : ALL_TIME;
  }
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

  function handleRangeChange(next: TimeRange) {
    navigate({ search: next });
    setPage(0);
    clearSelection();
  }

  const rangeSessionQuery = useQuery({
    queryKey: ["workSession", range.sessionId],
    queryFn: () => getWorkSession(range.sessionId as number),
    enabled: range.kind === "session" && range.sessionId !== undefined,
  });
  const rangeSession =
    range.kind === "session" ? (rangeSessionQuery.data ?? null) : null;
  const rangeSessionMissing =
    range.kind === "session" && rangeSessionQuery.isSuccess && !rangeSession;

  const { start, end } = useMemo(
    () => resolveRange(range, rangeSession),
    [range, rangeSession],
  );

  const sessionsQuery = useQuery({
    queryKey: [
      "sessions",
      page,
      PAGE_SIZE,
      search,
      categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
      start,
      end,
    ],
    queryFn: () =>
      listSessions(
        page,
        PAGE_SIZE,
        search || undefined,
        categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
        start,
        end,
      ),
    enabled: range.kind !== "session" || rangeSession !== null,
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
        toast.success("Cleared this activity's category");
        return;
      }
      const category = categories.find((c) => c.id === categoryId);
      const categoryName = category
        ? categoryPath(category, categories)
        : "category";
      toast.success(`Set to ${categoryName}`, {
        description: row.window_title,
      });
    },
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

  async function runRecategorize() {
    const toastId = toast.loading("Recategorizing all activities…");
    try {
      const { pins_cleared } = await recategorizeAll();
      invalidateSessions();
      toast.success(
        pins_cleared === 0
          ? "Recategorized all activities — nothing pinned by hand, so your rules already decided everything."
          : `Recategorized all activities — cleared ${pins_cleared} manual activity pin${pins_cleared === 1 ? "" : "s"}.`,
        { id: toastId },
      );
    } catch (err) {
      toast.error(String(err), { id: toastId });
    }
  }

  function handleRecategorizeClick() {
    toast("Recategorize all history from scratch?", {
      description:
        'Clears every per-activity category you picked by hand, so only your rules, and "Uncategorized" where none match, decide. Your rules are kept. This cannot be undone.',
      action: {
        label: "Clear pins and recategorize",
        onClick: runRecategorize,
      },
      duration: 10000,
    });
  }

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
        categories={categories}
        range={range}
        rangeSession={rangeSession}
        onRangeChange={handleRangeChange}
        loading={loading}
        onRefresh={() => sessionsQuery.refetch()}
        onRecategorizeClick={handleRecategorizeClick}
      />

      {rangeSessionMissing && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          That focus session no longer exists — clear the time filter to see
          your history again.
        </div>
      )}

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
        selectedIds={selectedIds}
        onToggleRow={toggleRow}
        onToggleAllVisible={toggleAllVisible}
        onRowCategoryChange={(row, value) =>
          categoryChangeMutation.mutate({ row, value })
        }
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
