import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
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
import { Badge } from "@/components/ui/badge";
import { formatDuration, formatTimestamp } from "@/duration";
import type { Category, SessionRow } from "@/types";
import { ALL_CATEGORIES, ALL_SOURCES, UNCATEGORIZED } from "./constants";

function exeFileName(exePath: string): string {
  const segments = exePath.split(/[\\/]/);
  return (segments[segments.length - 1] || exePath).toLowerCase();
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
  const allVisibleSelected =
    rows.length > 0 && rows.every((r) => selectedIds.has(r.id));
  const someVisibleSelected =
    !allVisibleSelected && rows.some((r) => selectedIds.has(r.id));

  return (
    <Card className="@container flex-1 overflow-hidden p-0">
      <div className="h-full overflow-y-auto subtle-scrollbar">
        <Table className="table-fixed w-full">
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
              <TableHead className="w-36">App</TableHead>
              <TableHead className="w-36">Category</TableHead>
              <TableHead className="hidden w-52 @4xl:table-cell">
                Site
              </TableHead>
              <TableHead>Window title</TableHead>
              <TableHead className="hidden w-40 @2xl:table-cell">
                Start
              </TableHead>
              <TableHead className="w-20 text-right">Duration</TableHead>
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
                  <TableCell className="truncate">
                    <Tooltip>
                      <TooltipTrigger className="truncate text-left">
                        {row.app_name}
                      </TooltipTrigger>
                      <TooltipContent>
                        {exeFileName(row.exe_path)}
                      </TooltipContent>
                    </Tooltip>
                  </TableCell>
                  <TableCell>
                    <Select
                      value={String(row.category_id ?? UNCATEGORIZED)}
                      onValueChange={(value) =>
                        onRowCategoryChange(row, value ?? UNCATEGORIZED)
                      }
                    >
                      <SelectTrigger
                        size="sm"
                        className="h-6 w-full min-w-0 px-1.5 text-xs"
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
                            {row.category_source === "ai" && (
                              <Badge
                                variant="secondary"
                                className="h-4 px-1 text-[9px]"
                              >
                                AI{" "}
                                {Math.round(
                                  (row.category_confidence ?? 0) * 100,
                                )}
                                %
                              </Badge>
                            )}
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
                  </TableCell>
                  <TableCell
                    className="hidden truncate font-mono text-xs @4xl:table-cell"
                    title={row.url ?? undefined}
                  >
                    {row.url ?? ""}
                  </TableCell>
                  <TableCell
                    className="truncate"
                    title={
                      row.url
                        ? `${row.window_title}\n${row.url}`
                        : row.window_title
                    }
                  >
                    {row.window_title}
                  </TableCell>
                  <TableCell className="hidden text-muted-foreground tabular-nums @2xl:table-cell">
                    {formatTimestamp(row.start_time)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
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
