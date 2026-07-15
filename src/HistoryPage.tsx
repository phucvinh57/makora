import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  listCategories,
  listRecategorizationKeys,
  listSessions,
  previewSessionCategory,
  recategorizeKeys,
  setSessionCategory,
  setSessionRowCategory,
  setSessionsCategory,
} from "./api";
import { formatDuration, formatTimestamp } from "./duration";
import type { SessionRow } from "./types";

// The exe file name (lowercased) is exactly what an "app" category rule's
// pattern must match — showing it as a tooltip saves a trip to Task Manager
// when the user wants to categorize an app from the history table.
function exeFileName(exePath: string): string {
  const segments = exePath.split(/[\\/]/);
  return (segments[segments.length - 1] || exePath).toLowerCase();
}

const PAGE_SIZE = 50;
const UNCATEGORIZED = "uncategorized";
const ALL_CATEGORIES = "all";
const SEARCH_DEBOUNCE_MS = 300;
const RECATEGORIZE_CHUNK_SIZE = 25;

export default function HistoryPage() {
  const queryClient = useQueryClient();

  const [page, setPage] = useState(0);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [categoryFilter, setCategoryFilter] = useState(ALL_CATEGORIES);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [bulkCategory, setBulkCategory] = useState("");

  // Debounce the search box so typing doesn't fire a query per keystroke. A
  // change lands the user back on page 0 and clears the (now possibly
  // stale) selection, same as changing the category filter does below.
  useEffect(() => {
    const handle = setTimeout(() => {
      setSearch(searchInput.trim());
      setPage(0);
      setSelectedIds(new Set());
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [searchInput]);

  function handleCategoryFilterChange(value: string) {
    setCategoryFilter(value);
    setPage(0);
    setSelectedIds(new Set());
  }

  const sessionsQuery = useQuery({
    queryKey: [
      "sessions",
      page,
      PAGE_SIZE,
      search,
      categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
    ],
    queryFn: () =>
      listSessions(
        page,
        PAGE_SIZE,
        search || undefined,
        categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
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
  }

  // Applies a category to every session sharing a key (from the toast
  // action below, or the multi-select bulk bar) — a wider, deliberate
  // choice, so it gets its own success toast rather than the row-level one.
  const applyToAllMutation = useMutation({
    mutationFn: ({
      sessionId,
      categoryId,
    }: {
      sessionId: number;
      categoryId: number;
      label: string;
    }) => setSessionCategory(sessionId, categoryId),
    onSuccess: (_, { label }) => {
      invalidateSessions();
      toast.success(`Applied to all ${label} activities`);
    },
    onError: (err) => toast.error(String(err)),
  });

  // Sets (or clears) the category on just this one activity — no other
  // session sharing its app/site is touched. On success, a toast offers a
  // one-click way to widen the choice to every activity sharing the same
  // key instead (via `applyToAllMutation`) — replacing an earlier
  // confirm-before-committing dialog, which had the problem that canceling
  // it silently left the row uncategorized.
  const categoryChangeMutation = useMutation({
    mutationFn: async ({ row, value }: { row: SessionRow; value: string }) => {
      const categoryId = value === UNCATEGORIZED ? null : Number(value);
      await setSessionRowCategory(row.id, categoryId);
      const preview =
        categoryId === null ? null : await previewSessionCategory(row.id);
      return { row, categoryId, preview };
    },
    onSuccess: ({ row, categoryId, preview }) => {
      invalidateSessions();
      if (categoryId === null) {
        toast.success("Cleared this activity's category");
        return;
      }
      const categoryName =
        categories.find((c) => c.id === categoryId)?.name ?? "category";
      toast.success(`Set to ${categoryName}`, {
        description: row.window_title,
        action:
          preview && preview.affected_count > 1
            ? {
                label: `Apply to all ${preview.affected_count} ${preview.key} activities`,
                onClick: () =>
                  applyToAllMutation.mutate({
                    sessionId: row.id,
                    categoryId,
                    label: preview.key,
                  }),
              }
            : undefined,
      });
    },
    onError: (err) => toast.error(String(err)),
  });

  const bulkApplyMutation = useMutation({
    mutationFn: () =>
      setSessionsCategory([...selectedIds], Number(bulkCategory)),
    onSuccess: () => {
      setSelectedIds(new Set());
      setBulkCategory("");
      invalidateSessions();
    },
  });

  // Re-evaluates every app/site key in history against the current rule
  // set by clearing any manual categorization (key-wide override or
  // per-session pin) — after that, the read-time join in `list_sessions`
  // picks the category back up from the live rules (or "Uncategorized")
  // instead of a frozen manual choice. Keys are fetched once, then cleared
  // in chunks so the toast can show real progress instead of blocking
  // behind one opaque round trip.
  async function runRecategorize() {
    const toastId = toast.loading("Finding activities to recategorize…");
    try {
      const keys = await listRecategorizationKeys(
        undefined,
        undefined,
        false,
        false,
      );
      if (keys.length === 0) {
        toast.success("Nothing to recategorize.", { id: toastId });
        return;
      }

      let processed = 0;
      let changed = 0;
      for (let i = 0; i < keys.length; i += RECATEGORIZE_CHUNK_SIZE) {
        const chunk = keys.slice(i, i + RECATEGORIZE_CHUNK_SIZE);
        changed += await recategorizeKeys(chunk);
        processed += chunk.length;
        toast.loading(`Recategorizing… ${processed} / ${keys.length}`, {
          id: toastId,
        });
      }

      invalidateSessions();
      toast.success(
        `Recategorized ${changed} of ${keys.length} app/site groups.`,
        { id: toastId },
      );
    } catch (err) {
      toast.error(String(err), { id: toastId });
    }
  }

  function handleRecategorizeClick() {
    toast("Recategorize all history with current rules?", {
      description:
        'Clears any manual categorization (per-activity or per-app/site) so your current rules — and "Uncategorized" where none match — decide instead.',
      action: { label: "Apply", onClick: runRecategorize },
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

  const allVisibleSelected =
    rows.length > 0 && rows.every((r) => selectedIds.has(r.id));
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4 flex items-center justify-between gap-3">
        <p className="m-0 text-sm text-muted-foreground">App-focus history</p>
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder="Search app, title, or site…"
            className="h-7 w-64 rounded-md border bg-transparent px-2 text-sm"
          />
          <select
            value={categoryFilter}
            onChange={(e) => handleCategoryFilterChange(e.target.value)}
            className="h-7 rounded-md border bg-transparent px-1.5 text-sm"
          >
            <option value={ALL_CATEGORIES}>All categories</option>
            <option value={UNCATEGORIZED}>Uncategorized</option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.name}
              </option>
            ))}
          </select>
          <Button
            variant="outline"
            size="sm"
            disabled={loading}
            onClick={() => sessionsQuery.refetch()}
            aria-label="Refresh history"
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} />
          </Button>
          <Button variant="outline" size="sm" onClick={handleRecategorizeClick}>
            Recategorize
          </Button>
        </div>
      </header>

      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load sessions: {error}
        </div>
      )}

      {selectedIds.size > 0 && (
        <div className="mb-3 flex items-center gap-2 rounded-md border bg-muted/50 px-3 py-2 text-sm">
          <span>{selectedIds.size} selected</span>
          <select
            value={bulkCategory}
            onChange={(e) => setBulkCategory(e.target.value)}
            className="h-7 rounded-md border bg-transparent px-1.5 text-xs"
          >
            <option value="" disabled>
              Set category…
            </option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.name}
              </option>
            ))}
          </select>
          <Button
            size="sm"
            disabled={!bulkCategory || bulkApplyMutation.isPending}
            onClick={() => bulkApplyMutation.mutate()}
          >
            Apply
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setSelectedIds(new Set())}
          >
            Clear selection
          </Button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto no-scrollbar rounded-md border">
        <Table className="table-fixed min-w-270">
          <TableHeader className="sticky top-0 z-10 bg-background">
            <TableRow>
              <TableHead className="w-8">
                <input
                  type="checkbox"
                  checked={allVisibleSelected}
                  onChange={(e) => toggleAllVisible(e.target.checked)}
                  aria-label="Select all visible rows"
                  style={{ colorScheme: "light" }}
                />
              </TableHead>
              <TableHead className="w-40">App</TableHead>
              <TableHead className="w-36">Category</TableHead>
              <TableHead className="w-56">Site</TableHead>
              <TableHead className="min-w-56">Window title</TableHead>
              <TableHead className="w-44">Start</TableHead>
              <TableHead className="w-20 text-right">Duration</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => (
              <TableRow key={row.id}>
                <TableCell>
                  <input
                    type="checkbox"
                    checked={selectedIds.has(row.id)}
                    onChange={(e) => toggleRow(row.id, e.target.checked)}
                    aria-label={`Select session ${row.id}`}
                    style={{ colorScheme: "light" }}
                  />
                </TableCell>
                <TableCell className="truncate">
                  <Tooltip>
                    <TooltipTrigger className="truncate text-left">
                      {row.app_name}
                    </TooltipTrigger>
                    <TooltipContent>{exeFileName(row.exe_path)}</TooltipContent>
                  </Tooltip>
                </TableCell>
                <TableCell>
                  <select
                    className="w-full min-w-0 rounded-md border bg-transparent px-1.5 py-0.5 text-xs"
                    style={{
                      color: row.category_color ?? undefined,
                      borderColor: row.category_color ?? undefined,
                    }}
                    value={row.category_id ?? UNCATEGORIZED}
                    title={
                      row.category_is_override
                        ? "Manually categorized"
                        : undefined
                    }
                    onChange={(e) =>
                      categoryChangeMutation.mutate({
                        row,
                        value: e.target.value,
                      })
                    }
                  >
                    <option value={UNCATEGORIZED}>Uncategorized</option>
                    {categories.map((category) => (
                      <option key={category.id} value={category.id}>
                        {category.name}
                      </option>
                    ))}
                  </select>
                </TableCell>
                <TableCell
                  className="truncate font-mono text-xs"
                  title={row.url ?? undefined}
                >
                  {row.url ?? ""}
                </TableCell>
                <TableCell className="truncate" title={row.window_title}>
                  {row.window_title}
                </TableCell>
                <TableCell className="text-muted-foreground tabular-nums">
                  {formatTimestamp(row.start_time)}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatDuration(row.start_time, row.end_time)}
                </TableCell>
              </TableRow>
            ))}
            {!loading && rows.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={7}
                  className="py-8 text-center text-muted-foreground"
                >
                  {search || categoryFilter !== ALL_CATEGORIES
                    ? "No sessions match this filter."
                    : "No sessions recorded yet."}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>

      <footer className="flex items-center justify-center gap-4 pt-3 text-sm">
        <Button
          variant="outline"
          size="sm"
          disabled={page === 0}
          onClick={() => setPage((p) => Math.max(0, p - 1))}
        >
          Prev
        </Button>
        <span>
          Page {page + 1} of {pageCount} ({total} sessions)
        </span>
        <Button
          variant="outline"
          size="sm"
          disabled={page + 1 >= pageCount}
          onClick={() => setPage((p) => p + 1)}
        >
          Next
        </Button>
      </footer>
    </div>
  );
}
