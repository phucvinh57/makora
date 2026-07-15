import { useEffect, useState } from "react";
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
  clearSessionCategory,
  listCategories,
  listSessions,
  setSessionCategory,
  setSessionsCategory,
} from "./api";
import { formatDuration, formatTimestamp } from "./duration";
import type { Category, SessionRow } from "./types";

const PAGE_SIZE = 50;
const UNCATEGORIZED = "uncategorized";
const ALL_CATEGORIES = "all";
const SEARCH_DEBOUNCE_MS = 300;

export default function HistoryPage() {
  const [page, setPage] = useState(0);
  const [rows, setRows] = useState<SessionRow[]>([]);
  const [total, setTotal] = useState(0);
  const [categories, setCategories] = useState<Category[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  function refreshSessions() {
    let cancelled = false;
    setLoading(true);
    listSessions(
      page,
      PAGE_SIZE,
      search || undefined,
      categoryFilter === ALL_CATEGORIES ? undefined : categoryFilter,
    )
      .then((data) => {
        if (cancelled) return;
        setRows(data.rows);
        setTotal(data.total);
        setError(null);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: the React Compiler stabilizes refreshSessions's identity across renders when page/search/categoryFilter don't change
  useEffect(() => refreshSessions(), [refreshSessions]);

  useEffect(() => {
    listCategories()
      .then(setCategories)
      .catch((err) => setError(String(err)));
  }, []);

  // A category picked here applies to every session sharing this row's
  // app/site (see `set_session_category`), so the whole page is re-fetched
  // rather than patching just this one row in place.
  async function handleCategoryChange(row: SessionRow, value: string) {
    try {
      if (value === UNCATEGORIZED) {
        await clearSessionCategory(row.id);
      } else {
        await setSessionCategory(row.id, Number(value));
      }
      refreshSessions();
    } catch (err) {
      setError(String(err));
    }
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

  async function handleBulkApply() {
    if (!bulkCategory || selectedIds.size === 0) return;
    try {
      await setSessionsCategory([...selectedIds], Number(bulkCategory));
      setSelectedIds(new Set());
      setBulkCategory("");
      refreshSessions();
    } catch (err) {
      setError(String(err));
    }
  }

  const allVisibleSelected = rows.length > 0 && rows.every((r) => selectedIds.has(r.id));
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
          <Button size="sm" disabled={!bulkCategory} onClick={handleBulkApply}>
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

      <div className="flex-1 overflow-y-auto rounded-md border">
        <Table className="table-fixed">
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
              <TableHead>Window title</TableHead>
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
                <TableCell className="truncate" title={row.app_name}>
                  {row.app_name}
                </TableCell>
                <TableCell>
                  <select
                    className="w-full rounded-md border bg-transparent px-1.5 py-0.5 text-xs"
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
                    onChange={(e) => handleCategoryChange(row, e.target.value)}
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
