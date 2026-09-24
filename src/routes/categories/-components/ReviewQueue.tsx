import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  acceptAiLabels,
  listAiReview,
  listCategories,
  setActivitiesCategory,
} from "@/api";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from "@/components/ui/pagination";
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
import { formatSeconds } from "@/duration";
import { invalidateCategorizationQueries } from "@/queries";

const PAGE_SIZE = 25;
const ALL = "all";

export function ReviewQueue() {
  const queryClient = useQueryClient();
  const [page, setPage] = useState(0);
  const [filter, setFilter] = useState(ALL);
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const reviewQuery = useQuery({
    queryKey: ["aiReview", page, filter],
    queryFn: () =>
      listAiReview(
        page,
        PAGE_SIZE,
        filter === ALL ? undefined : Number(filter),
      ),
  });

  const categories = categoriesQuery.data ?? [];
  const rows = reviewQuery.data?.rows ?? [];
  const total = reviewQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const allSelected =
    rows.length > 0 && rows.every((r) => selected.has(r.activity_id));
  const someSelected =
    !allSelected && rows.some((r) => selected.has(r.activity_id));

  // A decided row leaves the queue, so a stale page or selection has to be dropped with it.
  function afterDecision(ids: number[]) {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const id of ids) next.delete(id);
      return next;
    });
    if (page > 0 && rows.length === ids.length) setPage(page - 1);
    invalidateCategorizationQueries(queryClient);
  }

  const acceptMutation = useMutation({
    mutationFn: (ids: number[]) => acceptAiLabels(ids),
    onSuccess: (_, ids) => afterDecision(ids),
    onError: (err) => toast.error(String(err)),
  });
  const setCategoryMutation = useMutation({
    mutationFn: ({
      ids,
      categoryId,
    }: {
      ids: number[];
      categoryId: number | null;
    }) => setActivitiesCategory(ids, categoryId),
    onSuccess: (_, { ids }) => afterDecision(ids),
    onError: (err) => toast.error(String(err)),
  });
  const busy = acceptMutation.isPending || setCategoryMutation.isPending;
  const selectedIds = [...selected];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <div>
          <h2 className="m-0 text-sm font-semibold">Review AI labels</h2>
          <p className="m-0 text-xs text-muted-foreground">
            Longest-tracked first. Accepting, changing or rejecting a label
            teaches the model.
          </p>
        </div>
        <Select
          value={filter}
          onValueChange={(value) => {
            setFilter(value ?? ALL);
            setPage(0);
            setSelected(new Set());
          }}
          items={{
            [ALL]: "All suggestions",
            ...categoryValueLabels(categories),
          }}
        >
          <SelectTrigger size="sm" className="ml-auto w-52">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL}>All suggestions</SelectItem>
            {categoryOptions(categories)}
          </SelectContent>
        </Select>
      </div>

      {selected.size > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border bg-muted/50 px-3 py-2 text-sm">
          <span>{selected.size} selected</span>
          <Button
            size="sm"
            disabled={busy}
            onClick={() => acceptMutation.mutate(selectedIds)}
          >
            Accept selected
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() =>
              setCategoryMutation.mutate({ ids: selectedIds, categoryId: null })
            }
          >
            Reject selected
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setSelected(new Set())}
          >
            Clear selection
          </Button>
        </div>
      )}

      <Card className="overflow-hidden p-0">
        <Table className="table-fixed">
          <TableHeader>
            <TableRow>
              <TableHead className="w-8">
                <Checkbox
                  checked={allSelected}
                  indeterminate={someSelected}
                  onCheckedChange={(checked) =>
                    setSelected(
                      checked === true
                        ? new Set(rows.map((r) => r.activity_id))
                        : new Set(),
                    )
                  }
                  aria-label="Select all visible rows"
                />
              </TableHead>
              <TableHead className="w-32">App</TableHead>
              <TableHead>Activity</TableHead>
              <TableHead className="w-16 text-right">Time</TableHead>
              <TableHead className="w-56">Suggested category</TableHead>
              <TableHead className="w-20" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => (
              <TableRow key={row.activity_id}>
                <TableCell>
                  <Checkbox
                    checked={selected.has(row.activity_id)}
                    onCheckedChange={(checked) =>
                      setSelected((prev) => {
                        const next = new Set(prev);
                        if (checked === true) next.add(row.activity_id);
                        else next.delete(row.activity_id);
                        return next;
                      })
                    }
                    aria-label={`Select ${row.title}`}
                  />
                </TableCell>
                <TableCell className="truncate">{row.app_name}</TableCell>
                <TableCell className="truncate">
                  <Tooltip>
                    <TooltipTrigger className="block w-full truncate text-left">
                      {row.title}
                      {row.url && (
                        <span className="ml-2 font-mono text-xs text-muted-foreground">
                          {row.url}
                        </span>
                      )}
                    </TooltipTrigger>
                    <TooltipContent className="max-w-lg cursor-text whitespace-pre-wrap break-all">
                      {row.url ? `${row.title}\n${row.url}` : row.title}
                    </TooltipContent>
                  </Tooltip>
                </TableCell>
                <TableCell className="text-right text-muted-foreground tabular-nums">
                  {formatSeconds(row.total_seconds)}
                </TableCell>
                <TableCell>
                  <Select
                    value={String(row.category_id)}
                    disabled={busy}
                    onValueChange={(value) => {
                      if (value && Number(value) !== row.category_id) {
                        setCategoryMutation.mutate({
                          ids: [row.activity_id],
                          categoryId: Number(value),
                        });
                      }
                    }}
                    items={categoryValueLabels(categories)}
                  >
                    <SelectTrigger
                      size="sm"
                      className="h-6 w-full px-1.5 text-xs"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>{categoryOptions(categories)}</SelectContent>
                  </Select>
                </TableCell>
                <TableCell>
                  <div className="flex justify-end gap-1">
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      disabled={busy}
                      aria-label="Accept label"
                      title="Accept this label"
                      onClick={() => acceptMutation.mutate([row.activity_id])}
                    >
                      <Check />
                    </Button>
                    <Button
                      size="icon-sm"
                      variant="ghost"
                      disabled={busy}
                      aria-label="Reject label"
                      title="Reject: leave uncategorized"
                      onClick={() =>
                        setCategoryMutation.mutate({
                          ids: [row.activity_id],
                          categoryId: null,
                        })
                      }
                    >
                      <X />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
            {!reviewQuery.isFetching && rows.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={6}
                  className="py-8 text-center text-muted-foreground"
                >
                  {filter === ALL
                    ? "Nothing to review. Run “Categorize uncategorized” to get AI suggestions."
                    : "No pending suggestions in this category."}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </Card>

      {total > PAGE_SIZE && (
        <Pagination className="w-auto">
          <PaginationContent>
            <PaginationItem>
              <PaginationPrevious
                href="#"
                aria-disabled={page === 0}
                className={
                  page === 0 ? "pointer-events-none opacity-50" : undefined
                }
                onClick={(e) => {
                  e.preventDefault();
                  if (page > 0) setPage(page - 1);
                }}
              />
            </PaginationItem>
            <PaginationItem>
              <span className="px-2 text-sm text-muted-foreground">
                Page {page + 1} of {pageCount} ({total} suggestions)
              </span>
            </PaginationItem>
            <PaginationItem>
              <PaginationNext
                href="#"
                aria-disabled={page + 1 >= pageCount}
                className={
                  page + 1 >= pageCount
                    ? "pointer-events-none opacity-50"
                    : undefined
                }
                onClick={(e) => {
                  e.preventDefault();
                  if (page + 1 < pageCount) setPage(page + 1);
                }}
              />
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      )}
    </div>
  );
}
