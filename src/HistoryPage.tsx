import { useCallback, useEffect, useState } from "react";
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
} from "./api";
import { formatDuration, formatTimestamp } from "./duration";
import type { Category, SessionRow } from "./types";

const PAGE_SIZE = 50;
const UNCATEGORIZED = "uncategorized";

export default function HistoryPage() {
  const [page, setPage] = useState(0);
  const [rows, setRows] = useState<SessionRow[]>([]);
  const [total, setTotal] = useState(0);
  const [categories, setCategories] = useState<Category[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshSessions = useCallback(() => {
    let cancelled = false;
    setLoading(true);
    listSessions(page, PAGE_SIZE)
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
  }, [page]);

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

  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <div className="flex h-screen flex-col box-border p-6">
      <header className="mb-4">
        <h1 className="m-0 text-xl font-semibold">makora</h1>
        <p className="m-0 text-sm text-muted-foreground">App-focus history</p>
      </header>

      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load sessions: {error}
        </div>
      )}

      <div className="flex-1 overflow-y-auto rounded-md border">
        <Table className="table-fixed">
          <TableHeader className="sticky top-0 z-10 bg-background">
            <TableRow>
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
                  colSpan={6}
                  className="py-8 text-center text-muted-foreground"
                >
                  No sessions recorded yet.
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
