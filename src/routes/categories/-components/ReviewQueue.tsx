import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { Check, ChevronDown, ChevronRight, X } from "lucide-react";
import { Fragment, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  acceptAiLabels,
  listAiReview,
  listAiReviewGroups,
  listCategories,
  setActivitiesCategory,
} from "@/api";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import { Badge } from "@/components/ui/badge";
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
import { Switch } from "@/components/ui/switch";
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
import type { AiReviewGroup, Category, ReviewFilter } from "@/types";

const GROUP_PAGE_SIZE = 20;
const MEMBER_PAGE_SIZE = 20;
const ALL = "all";
/** Groups tracked for less than this are not worth a decision. */
const MIN_SECONDS = 60;

function groupId(group: AiReviewGroup): string {
  return `${group.key_kind}:${group.key}:${group.category_id}`;
}

function sourceName(group: AiReviewGroup): string {
  return group.key_kind === "domain" ? group.key : group.app_name;
}

type Decide = {
  busy: boolean;
  accept: (ids: number[]) => void;
  setCategory: (ids: number[], categoryId: number | null) => void;
};

function PageControls({
  page,
  pageCount,
  label,
  onPage,
}: {
  page: number;
  pageCount: number;
  label: string;
  onPage: (page: number) => void;
}) {
  return (
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
              if (page > 0) onPage(page - 1);
            }}
          />
        </PaginationItem>
        <PaginationItem>
          <span className="px-2 text-sm text-muted-foreground">
            Page {page + 1} of {pageCount} ({label})
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
              if (page + 1 < pageCount) onPage(page + 1);
            }}
          />
        </PaginationItem>
      </PaginationContent>
    </Pagination>
  );
}

function CategorySelect({
  categories,
  value,
  disabled,
  onChange,
}: {
  categories: Category[];
  value: number;
  disabled: boolean;
  onChange: (categoryId: number) => void;
}) {
  return (
    <Select
      value={String(value)}
      disabled={disabled}
      onValueChange={(next) => {
        if (next && Number(next) !== value) onChange(Number(next));
      }}
      items={categoryValueLabels(categories)}
    >
      <SelectTrigger size="sm" className="h-6 w-full px-1.5 text-xs">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>{categoryOptions(categories)}</SelectContent>
    </Select>
  );
}

function DecisionButtons({
  ids,
  decide,
  noun,
}: {
  ids: number[];
  decide: Decide;
  noun: string;
}) {
  return (
    <div className="flex justify-end gap-1">
      <Button
        size="icon-sm"
        variant="ghost"
        disabled={decide.busy}
        aria-label={`Accept ${noun}`}
        title={`Accept ${noun}`}
        onClick={() => decide.accept(ids)}
      >
        <Check />
      </Button>
      <Button
        size="icon-sm"
        variant="ghost"
        disabled={decide.busy}
        aria-label={`Reject ${noun}`}
        title={`Reject ${noun}: leave uncategorized`}
        onClick={() => decide.setCategory(ids, null)}
      >
        <X />
      </Button>
    </div>
  );
}

/** The activities behind one group, for correcting the outliers before accepting the rest. */
function GroupMembers({
  group,
  categories,
  decide,
}: {
  group: AiReviewGroup;
  categories: Category[];
  decide: Decide;
}) {
  const [page, setPage] = useState(0);
  const filter = useMemo<ReviewFilter>(
    () => ({ activities: group.activity_ids }),
    [group.activity_ids],
  );
  const membersQuery = useQuery({
    // The ids stay out of the key: a decision shrinks them, and a new key would drop the rows on screen.
    queryKey: ["aiReview", groupId(group), page],
    queryFn: () => listAiReview(page, MEMBER_PAGE_SIZE, filter),
    placeholderData: keepPreviousData,
  });
  const rows = membersQuery.data?.rows ?? [];
  const total = membersQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / MEMBER_PAGE_SIZE));

  useEffect(() => {
    if (page > 0 && membersQuery.data && rows.length === 0) setPage(page - 1);
  }, [page, membersQuery.data, rows.length]);

  return (
    <div className="flex flex-col gap-2 py-2 pl-8">
      <Table className="table-fixed">
        <TableBody>
          {rows.map((row) => (
            <TableRow key={row.activity_id}>
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
              <TableCell className="w-16 text-right text-muted-foreground tabular-nums">
                {formatSeconds(row.total_seconds)}
              </TableCell>
              <TableCell className="w-56">
                <CategorySelect
                  categories={categories}
                  value={row.category_id}
                  disabled={decide.busy}
                  onChange={(id) => decide.setCategory([row.activity_id], id)}
                />
              </TableCell>
              <TableCell className="w-20">
                <DecisionButtons
                  ids={[row.activity_id]}
                  decide={decide}
                  noun="label"
                />
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {total > MEMBER_PAGE_SIZE && (
        <PageControls
          page={page}
          pageCount={pageCount}
          label={`${total} activities`}
          onPage={setPage}
        />
      )}
    </div>
  );
}

function ConfidenceBadge({ value }: { value: number | null }) {
  if (value === null) {
    return <span className="text-xs text-muted-foreground">—</span>;
  }
  return (
    <Badge variant={value < 0.6 ? "destructive" : "outline"}>
      {Math.round(value * 100)}%
    </Badge>
  );
}

export function ReviewQueue() {
  const queryClient = useQueryClient();
  const [page, setPage] = useState(0);
  const [category, setCategory] = useState(ALL);
  const [uncertainOnly, setUncertainOnly] = useState(true);
  const [hideShort, setHideShort] = useState(true);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const filter = useMemo<ReviewFilter>(
    () => ({
      category: category === ALL ? undefined : Number(category),
      uncertain_only: uncertainOnly,
      min_seconds: hideShort ? MIN_SECONDS : 0,
    }),
    [category, uncertainOnly, hideShort],
  );

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const groupsQuery = useQuery({
    queryKey: ["aiReviewGroups", page, filter],
    queryFn: () => listAiReviewGroups(page, GROUP_PAGE_SIZE, filter),
    placeholderData: keepPreviousData,
  });

  const categories = categoriesQuery.data ?? [];
  const groups = groupsQuery.data?.groups ?? [];
  const total = groupsQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / GROUP_PAGE_SIZE));
  const selectedGroups = groups.filter((g) => selected.has(groupId(g)));
  const selectedIds = selectedGroups.flatMap((g) => g.activity_ids);
  const allSelected =
    groups.length > 0 && selectedGroups.length === groups.length;
  const someSelected = !allSelected && selectedGroups.length > 0;

  // A decided group leaves the queue, so a page that emptied has to be given up.
  useEffect(() => {
    if (page > 0 && groupsQuery.data && groups.length === 0) setPage(page - 1);
  }, [page, groupsQuery.data, groups.length]);

  function resetView() {
    setPage(0);
    setSelected(new Set());
    setExpanded(new Set());
  }

  const acceptMutation = useMutation({
    mutationFn: (ids: number[]) => acceptAiLabels(ids),
    onSuccess: () => {
      setSelected(new Set());
      invalidateCategorizationQueries(queryClient);
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
    }) => setActivitiesCategory(ids, categoryId),
    onSuccess: () => {
      setSelected(new Set());
      invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });
  const decide: Decide = {
    busy: acceptMutation.isPending || setCategoryMutation.isPending,
    accept: (ids) => acceptMutation.mutate(ids),
    setCategory: (ids, categoryId) =>
      setCategoryMutation.mutate({ ids, categoryId }),
  };

  function toggle(set: Set<string>, id: string, on: boolean): Set<string> {
    const next = new Set(set);
    if (on) next.add(id);
    else next.delete(id);
    return next;
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div>
          <h2 className="m-0 text-sm font-semibold">Review AI labels</h2>
          <p className="m-0 text-xs text-muted-foreground">
            Grouped by app or site, longest-tracked first. One decision covers a
            whole group, and it teaches the model. Suggestions already count in
            your reports.
          </p>
        </div>
        <div className="ml-auto flex items-center gap-2 text-xs">
          <Switch
            id="review-uncertain"
            checked={uncertainOnly}
            onCheckedChange={(on) => {
              setUncertainOnly(on);
              resetView();
            }}
          />
          <label htmlFor="review-uncertain">Only uncertain</label>
        </div>
        <div className="flex items-center gap-2 text-xs">
          <Switch
            id="review-hide-short"
            checked={hideShort}
            onCheckedChange={(on) => {
              setHideShort(on);
              resetView();
            }}
          />
          <label htmlFor="review-hide-short">Hide under 1 min</label>
        </div>
        <Select
          value={category}
          onValueChange={(value) => {
            setCategory(value ?? ALL);
            resetView();
          }}
          items={{
            [ALL]: "All suggestions",
            ...categoryValueLabels(categories),
          }}
        >
          <SelectTrigger size="sm" className="w-52">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL}>All suggestions</SelectItem>
            {categoryOptions(categories)}
          </SelectContent>
        </Select>
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
                        categories={categories}
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
                          categories={categories}
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
                  {uncertainOnly || hideShort
                    ? "Nothing left to review with these filters. Turn off “Only uncertain” or “Hide under 1 min” to see more."
                    : category === ALL
                      ? "Nothing to review. Run “Categorize uncategorized” to get AI suggestions."
                      : "No pending suggestions in this category."}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </Card>

      {total > GROUP_PAGE_SIZE && (
        <PageControls
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
