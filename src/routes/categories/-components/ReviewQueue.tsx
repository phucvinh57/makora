import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { ChevronDown, ChevronRight } from "lucide-react";
import { Fragment, useMemo, useState } from "react";
import { toast } from "sonner";
import { activity, aiReview } from "@/api";
import { CategoryCombobox } from "@/components/CategoryCombobox";
import {
  type CategoryChoice,
  useCategoryPicker,
} from "@/components/CategoryOptions";
import { PaginationControls } from "@/components/PaginationControls";
import { Button } from "@/components/ui/button";
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
import { useCategories } from "@/hooks/useCategories";
import { formatSeconds } from "@/duration";
import { invalidateCategorizationQueries, queryKeys } from "@/queries";
import type { ReviewFilter } from "@/types";
import { GroupMembers } from "./GroupMembers";
import {
  CategorySelect,
  ConfidenceBadge,
  DecisionButtons,
  type Decide,
  groupId,
  sourceName,
} from "./reviewShared";

const GROUP_PAGE_SIZE = 20;
const ALL = "all";
const ALL_CHOICES: CategoryChoice[] = [
  { value: ALL, label: "All suggestions" },
];

function toggle(set: Set<string>, id: string, on: boolean): Set<string> {
  const next = new Set(set);
  if (on) next.add(id);
  else next.delete(id);
  return next;
}

export function ReviewQueue() {
  const queryClient = useQueryClient();
  const [page, setPage] = useState(0);
  const [category, setCategory] = useState(ALL);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const filter = useMemo<ReviewFilter>(
    () => ({
      category: category === ALL ? undefined : Number(category),
    }),
    [category],
  );

  const { categories } = useCategories();
  const picker = useCategoryPicker(categories);
  const groupsQuery = useQuery({
    queryKey: [...queryKeys.aiReviewGroups, page, filter],
    queryFn: () => aiReview.listGroups(page, GROUP_PAGE_SIZE, filter),
    placeholderData: keepPreviousData,
  });

  const groups = groupsQuery.data?.groups ?? [];
  const total = groupsQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / GROUP_PAGE_SIZE));
  const selectedGroups = groups.filter((g) => selected.has(groupId(g)));
  const selectedIds = selectedGroups.flatMap((g) => g.activity_ids);
  const allSelected =
    groups.length > 0 && selectedGroups.length === groups.length;
  const someSelected = !allSelected && selectedGroups.length > 0;

  // A decided group leaves the queue, so a page that emptied has to be given up.
  if (
    groupsQuery.data &&
    !groupsQuery.isPlaceholderData &&
    page > pageCount - 1
  ) {
    setPage(pageCount - 1);
  }

  function resetView() {
    setPage(0);
    setSelected(new Set());
    setExpanded(new Set());
  }

  const acceptMutation = useMutation({
    mutationFn: (ids: number[]) => aiReview.accept(ids),
    onSuccess: () => {
      setSelected(new Set());
      return invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });
  const setCategoryMutation = useMutation({
    mutationFn: ({
      ids,
      categoryId,
    }: {
      ids: number[];
      categoryId: number | null;
    }) => activity.setCategory(ids, categoryId),
    onSuccess: () => {
      setSelected(new Set());
      return invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });
  const decide: Decide = {
    busy: acceptMutation.isPending || setCategoryMutation.isPending,
    accept: (ids) => acceptMutation.mutate(ids),
    setCategory: (ids, categoryId) =>
      setCategoryMutation.mutate({ ids, categoryId }),
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <h2 className="m-0 text-sm font-semibold">Review AI labels</h2>
        <CategoryCombobox
          picker={picker}
          extraChoices={ALL_CHOICES}
          value={category}
          onValueChange={(value) => {
            setCategory(value);
            resetView();
          }}
          ariaLabel="Category"
          size="sm"
          className="ml-auto w-52"
        />
      </div>

      {selectedGroups.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border bg-muted/50 px-3 py-2 text-sm">
          <span>
            {selectedGroups.length} selected ({selectedIds.length} activities)
          </span>
          <Button
            size="sm"
            disabled={decide.busy}
            onClick={() => decide.accept(selectedIds)}
          >
            Accept selected
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={decide.busy}
            onClick={() => decide.setCategory(selectedIds, null)}
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
              <TableHead className="w-14">
                <Checkbox
                  checked={allSelected}
                  indeterminate={someSelected}
                  onCheckedChange={(checked) =>
                    setSelected(
                      checked === true
                        ? new Set(groups.map(groupId))
                        : new Set(),
                    )
                  }
                  aria-label="Select all visible groups"
                />
              </TableHead>
              <TableHead>App or site</TableHead>
              <TableHead className="w-24 text-right">Activities</TableHead>
              <TableHead className="w-20 text-right">Time</TableHead>
              <TableHead className="w-16 text-right">Sure</TableHead>
              <TableHead className="w-56">Suggested category</TableHead>
              <TableHead className="w-20" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {groups.map((group) => {
              const id = groupId(group);
              const open = expanded.has(id);
              return (
                <Fragment key={id}>
                  <TableRow
                    className="cursor-pointer"
                    onClick={() =>
                      setExpanded((prev) => toggle(prev, id, !open))
                    }
                  >
                    <TableCell onClick={(e) => e.stopPropagation()}>
                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          className="flex size-7 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground"
                          aria-label={
                            open ? "Hide activities" : "Show activities"
                          }
                          aria-expanded={open}
                          onClick={() =>
                            setExpanded((prev) => toggle(prev, id, !open))
                          }
                        >
                          {open ? (
                            <ChevronDown className="size-4" />
                          ) : (
                            <ChevronRight className="size-4" />
                          )}
                        </button>
                        {/* The pseudo-element widens the checkbox's click target. */}
                        <Checkbox
                          className="relative m-1.5 before:absolute before:-inset-2 before:content-['']"
                          checked={selected.has(id)}
                          onCheckedChange={(checked) =>
                            setSelected((prev) =>
                              toggle(prev, id, checked === true),
                            )
                          }
                          aria-label={`Select ${sourceName(group)}`}
                        />
                      </div>
                    </TableCell>
                    <TableCell className="truncate font-medium">
                      {sourceName(group)}
                    </TableCell>
                    <TableCell className="text-right text-muted-foreground tabular-nums">
                      {group.activity_count}
                    </TableCell>
                    <TableCell className="text-right text-muted-foreground tabular-nums">
                      {formatSeconds(group.total_seconds)}
                    </TableCell>
                    <TableCell className="text-right">
                      <ConfidenceBadge value={group.min_confidence} />
                    </TableCell>
                    {/* Portaled select popups bubble React events, so stop them here. */}
                    <TableCell onClick={(e) => e.stopPropagation()}>
                      <CategorySelect
                        picker={picker}
                        value={group.category_id}
                        disabled={decide.busy}
                        onChange={(next) =>
                          decide.setCategory(group.activity_ids, next)
                        }
                      />
                    </TableCell>
                    <TableCell onClick={(e) => e.stopPropagation()}>
                      <DecisionButtons
                        ids={group.activity_ids}
                        decide={decide}
                        noun={`all ${group.activity_count} in ${sourceName(group)}`}
                      />
                    </TableCell>
                  </TableRow>
                  {open && (
                    <TableRow className="bg-muted/30 hover:bg-muted/30">
                      <TableCell colSpan={7} className="p-0">
                        <GroupMembers
                          group={group}
                          picker={picker}
                          decide={decide}
                        />
                      </TableCell>
                    </TableRow>
                  )}
                </Fragment>
              );
            })}
            {!groupsQuery.isFetching && groups.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={7}
                  className="py-8 text-center text-muted-foreground"
                >
                  {category === ALL
                    ? "Nothing to review. Run “Categorize” to get AI suggestions."
                    : "No pending suggestions in this category."}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </Card>

      {total > GROUP_PAGE_SIZE && (
        <PaginationControls
          page={page}
          pageCount={pageCount}
          label={`${total} groups`}
          onPage={(next) => {
            setPage(next);
            setSelected(new Set());
          }}
        />
      )}
    </div>
  );
}
