import type { CSSProperties, ReactNode } from "react";
import type { CategoryPicker } from "@/components/CategoryOptions";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import type { SessionRow } from "@/types";
import { SessionTableRow } from "./SessionTableRow";
import { type ResizableColumn, useColumnResize } from "./useColumnResize";

function ResizableHead({
  column,
  handleProps,
  className,
  sized = true,
  children,
}: {
  column: ResizableColumn;
  /** Whether the header carries the column's width (see the Window title column). */
  sized?: boolean;
  handleProps: ReturnType<ReturnType<typeof useColumnResize>["handleProps"]>;
  className?: string;
  children: ReactNode;
}) {
  return (
    <TableHead
      className={cn("relative pr-3", className)}
      style={sized ? { width: `var(--history-${column}-width)` } : undefined}
    >
      {children}
      <button
        type="button"
        title="Drag to resize. Double-click to reset."
        className="absolute top-0 right-0 h-full w-2 cursor-col-resize touch-none select-none outline-none after:absolute after:inset-y-2 after:left-1/2 after:w-px after:bg-border after:opacity-0 hover:after:opacity-100 focus-visible:after:opacity-100 data-resizing:after:bg-primary data-resizing:after:opacity-100"
        {...handleProps}
      />
    </TableHead>
  );
}

export function SessionsTable({
  rows,
  picker,
  loading,
  filtered,
  selectedIds,
  onToggleRow,
  onToggleAllVisible,
  onRowCategoryChange,
  onAcceptAiLabel,
}: {
  rows: SessionRow[];
  picker: CategoryPicker;
  loading: boolean;
  /** Whether a search or filter is narrowing the list, for the empty message. */
  filtered: boolean;
  selectedIds: ReadonlySet<number>;
  onToggleRow: (id: number, checked: boolean) => void;
  onToggleAllVisible: (checked: boolean) => void;
  onRowCategoryChange: (row: SessionRow, value: string) => void;
  onAcceptAiLabel: (row: SessionRow) => void;
}) {
  const { widths, handleProps } = useColumnResize();
  const allVisibleSelected =
    rows.length > 0 && rows.every((r) => selectedIds.has(r.id));
  const someVisibleSelected =
    !allVisibleSelected && rows.some((r) => selectedIds.has(r.id));

  return (
    <Card className="@container flex-1 overflow-hidden p-0">
      <div className="h-full overflow-y-auto subtle-scrollbar">
        <Table
          className="table-fixed min-w-[calc(32px+var(--history-app-width)+var(--history-category-width)+var(--history-title-width)+var(--history-duration-width))] @2xl:min-w-[calc(32px+var(--history-app-width)+var(--history-category-width)+var(--history-title-width)+var(--history-start-width)+var(--history-duration-width))] @4xl:min-w-[calc(32px+var(--history-app-width)+var(--history-category-width)+var(--history-site-width)+var(--history-title-width)+var(--history-start-width)+var(--history-duration-width))]"
          style={
            {
              "--history-app-width": `${widths.app}px`,
              "--history-category-width": `${widths.category}px`,
              "--history-site-width": `${widths.site}px`,
              "--history-title-width": `${widths.title}px`,
              "--history-start-width": `${widths.start}px`,
              "--history-duration-width": `${widths.duration}px`,
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
              <ResizableHead column="app" handleProps={handleProps("app")}>
                App
              </ResizableHead>
              <ResizableHead
                column="category"
                handleProps={handleProps("category")}
              >
                Category
              </ResizableHead>
              <ResizableHead
                column="site"
                handleProps={handleProps("site")}
                className="hidden @4xl:table-cell"
              >
                Site
              </ResizableHead>
              {/* No width, deliberately: under `table-fixed` a table wider than the sum of
                  its columns hands the slack to every sized column in proportion, so App and
                  Category would creep wider on a big window. Leaving one column auto sends all
                  of it here instead, and the table's min-width keeps this column from dropping
                  below its configured (resizable) width. */}
              <ResizableHead
                column="title"
                handleProps={handleProps("title")}
                sized={false}
              >
                Window title
              </ResizableHead>
              <ResizableHead
                column="start"
                handleProps={handleProps("start")}
                className="hidden @2xl:table-cell"
              >
                Start
              </ResizableHead>
              <ResizableHead
                column="duration"
                handleProps={handleProps("duration")}
                className="text-right"
              >
                Duration
              </ResizableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => (
              <SessionTableRow
                key={row.id}
                row={row}
                selected={selectedIds.has(row.id)}
                picker={picker}
                onToggleRow={onToggleRow}
                onCategoryChange={onRowCategoryChange}
                onAcceptAiLabel={onAcceptAiLabel}
              />
            ))}
            {!loading && rows.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={7}
                  className="py-8 text-center text-muted-foreground"
                >
                  {filtered
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
