import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
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
import { categoryOptions } from "@/components/CategoryOptions";
import { categoryPath } from "@/categories";
import { formatDuration, formatTimestamp } from "@/duration";
import type { Category, SessionRow } from "@/types";
import { useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { toast } from "sonner";
import { ALL_CATEGORIES, ALL_SOURCES, UNCATEGORIZED } from "./constants";

const DEFAULT_COLUMN_WIDTHS = {
  app: 120,
  category: 224,
  site: 208,
  title: 280,
  start: 160,
  duration: 80,
} as const;

const MIN_COLUMN_WIDTHS: Record<keyof typeof DEFAULT_COLUMN_WIDTHS, number> = {
  app: 96,
  category: 144,
  site: 128,
  title: 160,
  start: 128,
  duration: 64,
};

type ResizableColumn = keyof typeof DEFAULT_COLUMN_WIDTHS;

function exeFileName(exePath: string): string {
  const segments = exePath.split(/[\\/]/);
  return (segments[segments.length - 1] || exePath).toLowerCase();
}

function openSite(url: string) {
  void openUrl(url).catch((error) => {
    toast.error("Could not open site", { description: String(error) });
  });
}

export function SessionsTable({
  rows,
  categories,
  loading,
  search,
  categoryFilter,
  sourceFilter,
  selectedIds,
  onToggleRow,
  onToggleAllVisible,
  onRowCategoryChange,
}: {
  rows: SessionRow[];
  categories: Category[];
  loading: boolean;
  search: string;
  categoryFilter: string;
  sourceFilter: string;
  selectedIds: Set<number>;
  onToggleRow: (id: number, checked: boolean) => void;
  onToggleAllVisible: (checked: boolean) => void;
  onRowCategoryChange: (row: SessionRow, value: string) => void;
}) {
  const [columnWidths, setColumnWidths] = useState(DEFAULT_COLUMN_WIDTHS);
  const [resizingColumn, setResizingColumn] = useState<ResizableColumn | null>(
    null,
  );
  const resizeState = useRef<{
    column: ResizableColumn;
    pointerId: number;
    startX: number;
    startWidth: number;
  } | null>(null);
  const allVisibleSelected =
    rows.length > 0 && rows.every((r) => selectedIds.has(r.id));
  const someVisibleSelected =
    !allVisibleSelected && rows.some((r) => selectedIds.has(r.id));

  function startColumnResize(
    column: ResizableColumn,
    event: PointerEvent<HTMLButtonElement>,
  ) {
    if (event.button !== 0) return;

    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    resizeState.current = {
      column,
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: columnWidths[column],
    };
    setResizingColumn(column);
  }

  function resizeColumn(event: PointerEvent<HTMLButtonElement>) {
    const resize = resizeState.current;
    if (!resize || resize.pointerId !== event.pointerId) return;

    event.preventDefault();
    setColumnWidths((widths) => ({
      ...widths,
      [resize.column]: Math.max(
        MIN_COLUMN_WIDTHS[resize.column],
        resize.startWidth + event.clientX - resize.startX,
      ),
    }));
  }

  function finishColumnResize(event: PointerEvent<HTMLButtonElement>) {
    if (resizeState.current?.pointerId !== event.pointerId) return;

    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    resizeState.current = null;
    setResizingColumn(null);
  }

  function resetColumnWidth(column: ResizableColumn) {
    setColumnWidths((widths) => ({
      ...widths,
      [column]: DEFAULT_COLUMN_WIDTHS[column],
    }));
  }

  function resizeHandle(column: ResizableColumn) {
    return (
      <button
        type="button"
        aria-label={`Resize ${column} column`}
        title="Drag to resize. Double-click to reset."
        data-resizing={resizingColumn === column || undefined}
        className="absolute top-0 right-0 h-full w-2 cursor-col-resize touch-none select-none outline-none after:absolute after:inset-y-2 after:left-1/2 after:w-px after:bg-border after:opacity-0 hover:after:opacity-100 focus-visible:after:opacity-100 data-resizing:after:bg-primary data-resizing:after:opacity-100"
        onPointerDown={(event) => startColumnResize(column, event)}
        onPointerMove={resizeColumn}
        onPointerUp={finishColumnResize}
        onPointerCancel={finishColumnResize}
        onDoubleClick={() => resetColumnWidth(column)}
      />
    );
  }

  return (
    <Card className="@container flex-1 overflow-hidden p-0">
      <div className="h-full overflow-y-auto subtle-scrollbar">
        <Table
          className="table-fixed min-w-[calc(32px+var(--history-app-width)+var(--history-category-width)+var(--history-title-width)+var(--history-duration-width))] @2xl:min-w-[calc(32px+var(--history-app-width)+var(--history-category-width)+var(--history-title-width)+var(--history-start-width)+var(--history-duration-width))] @4xl:min-w-[calc(32px+var(--history-app-width)+var(--history-category-width)+var(--history-site-width)+var(--history-title-width)+var(--history-start-width)+var(--history-duration-width))]"
          style={
            {
              "--history-app-width": `${columnWidths.app}px`,
              "--history-category-width": `${columnWidths.category}px`,
              "--history-site-width": `${columnWidths.site}px`,
              "--history-title-width": `${columnWidths.title}px`,
              "--history-start-width": `${columnWidths.start}px`,
              "--history-duration-width": `${columnWidths.duration}px`,
            } as CSSProperties
          }
        >
          <TableHeader className="sticky top-0 z-10 bg-card">
            <TableRow>
              <TableHead className="w-8">
                <Checkbox
                  checked={allVisibleSelected}
                  indeterminate={someVisibleSelected}
                  onCheckedChange={(checked) =>
                    onToggleAllVisible(checked === true)
                  }
                  aria-label="Select all visible rows"
                />
              </TableHead>
              <TableHead
                className="relative pr-3"
                style={{ width: columnWidths.app }}
              >
                App
                {resizeHandle("app")}
              </TableHead>
              <TableHead
                className="relative pr-3"
                style={{ width: columnWidths.category }}
              >
                Category
                {resizeHandle("category")}
              </TableHead>
              <TableHead
                className="relative hidden pr-3 @4xl:table-cell"
                style={{ width: columnWidths.site }}
              >
                Site
                {resizeHandle("site")}
              </TableHead>
              {/* No width, deliberately: under `table-fixed` a table wider than the sum of
                  its columns hands the slack to every sized column in proportion, so App and
                  Category would creep wider on a big window. Leaving one column auto sends all
                  of it here instead, and the table's min-width keeps this column from dropping
                  below its configured (resizable) width. */}
              <TableHead className="relative pr-3">
                Window title
                {resizeHandle("title")}
              </TableHead>
              <TableHead
                className="relative hidden pr-3 @2xl:table-cell"
                style={{ width: columnWidths.start }}
              >
                Start
                {resizeHandle("start")}
              </TableHead>
              <TableHead
                className="relative pr-3 text-right"
                style={{ width: columnWidths.duration }}
              >
                Duration
                {resizeHandle("duration")}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => {
              const category =
                row.category_id !== null
                  ? categories.find((c) => c.id === row.category_id)
                  : undefined;
              const path = category
                ? categoryPath(category, categories)
                : "Uncategorized";
              const siteUrl = row.url;
              return (
                <TableRow key={row.id}>
                  <TableCell>
                    <Checkbox
                      checked={selectedIds.has(row.id)}
                      onCheckedChange={(checked) =>
                        onToggleRow(row.id, checked === true)
                      }
                      aria-label={`Select session ${row.id}`}
                    />
                  </TableCell>
                  <TableCell
                    className="truncate"
                    style={{ width: columnWidths.app }}
                  >
                    <Tooltip>
                      <TooltipTrigger className="truncate text-left">
                        {row.app_name}
                      </TooltipTrigger>
                      <TooltipContent className="cursor-text">
                        {exeFileName(row.exe_path)}
                      </TooltipContent>
                    </Tooltip>
                  </TableCell>
                  <TableCell style={{ width: columnWidths.category }}>
                    <div className="flex min-w-0 items-center gap-1">
                      <Select
                        value={String(row.category_id ?? UNCATEGORIZED)}
                        onValueChange={(value) =>
                          onRowCategoryChange(row, value ?? UNCATEGORIZED)
                        }
                      >
                        <SelectTrigger
                          size="sm"
                          className="h-6 min-w-0 flex-1 px-1.5 text-xs"
                          title={
                            row.category_is_pinned
                              ? `${path} — pinned to this activity`
                              : path
                          }
                        >
                          <SelectValue>
                            <span className="flex min-w-0 items-center gap-1.5">
                              <span
                                className="size-2 shrink-0 rounded-full"
                                style={{
                                  backgroundColor:
                                    row.category_color ??
                                    "var(--muted-foreground)",
                                }}
                              />
                              <span className="truncate">
                                {row.category_name ?? "Uncategorized"}
                              </span>
                            </span>
                          </SelectValue>
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value={UNCATEGORIZED}>
                            Uncategorized
                          </SelectItem>
                          {categoryOptions(categories)}
                        </SelectContent>
                      </Select>
                    </div>
                  </TableCell>
                  <TableCell
                    className="hidden truncate font-mono text-xs @4xl:table-cell"
                    style={{ width: columnWidths.site }}
                  >
                    {siteUrl ? (
                      <Tooltip>
                        <TooltipTrigger
                          render={<a href={siteUrl} />}
                          className="block w-full truncate text-left text-primary underline-offset-4 hover:underline"
                          onClick={(event) => {
                            event.preventDefault();
                            openSite(siteUrl);
                          }}
                        >
                          {siteUrl}
                        </TooltipTrigger>
                        <TooltipContent className="max-w-lg cursor-text break-all">
                          {siteUrl}
                        </TooltipContent>
                      </Tooltip>
                    ) : null}
                  </TableCell>
                  <TableCell className="truncate">
                    <Tooltip>
                      <TooltipTrigger className="block w-full truncate text-left">
                        {row.window_title}
                      </TooltipTrigger>
                      <TooltipContent className="max-w-lg cursor-text whitespace-pre-wrap wrap-break-word">
                        {row.url
                          ? `${row.window_title}\n${row.url}`
                          : row.window_title}
                      </TooltipContent>
                    </Tooltip>
                  </TableCell>
                  <TableCell
                    className="hidden text-muted-foreground tabular-nums @2xl:table-cell"
                    style={{ width: columnWidths.start }}
                  >
                    {formatTimestamp(row.start_time)}
                  </TableCell>
                  <TableCell
                    className="text-right tabular-nums"
                    style={{ width: columnWidths.duration }}
                  >
                    {formatDuration(row.start_time, row.end_time)}
                  </TableCell>
                </TableRow>
              );
            })}
            {!loading && rows.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={7}
                  className="py-8 text-center text-muted-foreground"
                >
                  {search ||
                  categoryFilter !== ALL_CATEGORIES ||
                  sourceFilter !== ALL_SOURCES
                    ? "No sessions match this filter."
                    : "No sessions recorded yet."}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
    </Card>
  );
}
