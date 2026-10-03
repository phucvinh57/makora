import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { aiReview } from "@/api";
import type { CategoryPicker } from "@/components/CategoryOptions";
import { PaginationControls } from "@/components/PaginationControls";
import { Table, TableBody, TableCell, TableRow } from "@/components/ui/table";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatSeconds } from "@/duration";
import { queryKeys } from "@/queries";
import type { AiReviewGroup, ReviewFilter } from "@/types";
import {
  CategorySelect,
  DecisionButtons,
  type Decide,
  groupId,
} from "./reviewShared";

const MEMBER_PAGE_SIZE = 20;

export function GroupMembers({
  group,
  picker,
  decide,
}: {
  group: AiReviewGroup;
  picker: CategoryPicker;
  decide: Decide;
}) {
  const [page, setPage] = useState(0);
  const filter = useMemo<ReviewFilter>(
    () => ({ activities: group.activity_ids }),
    [group.activity_ids],
  );
  const membersQuery = useQuery({
    // The ids stay out of the key: a decision shrinks them, and a new key would drop the rows on screen.
    queryKey: [...queryKeys.aiReview, groupId(group), page],
    queryFn: () => aiReview.list(page, MEMBER_PAGE_SIZE, filter),
    placeholderData: keepPreviousData,
  });
  const rows = membersQuery.data?.rows ?? [];
  const total = membersQuery.data?.total ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / MEMBER_PAGE_SIZE));

  // Deciding the last row of a page empties it, so settle on the last page that exists.
  if (
    membersQuery.data &&
    !membersQuery.isPlaceholderData &&
    page > pageCount - 1
  ) {
    setPage(pageCount - 1);
  }

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
                  picker={picker}
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
        <PaginationControls
          page={page}
          pageCount={pageCount}
          label={`${total} activities`}
          onPage={setPage}
        />
      )}
    </div>
  );
}
