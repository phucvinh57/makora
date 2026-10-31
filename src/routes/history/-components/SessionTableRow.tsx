import { openUrl } from "@tauri-apps/plugin-opener";
import { Check } from "lucide-react";
import { memo } from "react";
import { toast } from "sonner";
import type {
  CategoryChoice,
  CategoryPicker,
} from "@/components/CategoryOptions";
import { CreatableCategoryCombobox } from "@/components/CreatableCategoryCombobox";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { TableCell, TableRow } from "@/components/ui/table";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatDuration, formatTimestamp } from "@/duration";
import { UNCATEGORIZED, UNCATEGORIZED_LABEL } from "@/categories";
import type { SessionRow } from "@/types";

const UNCATEGORIZED_CHOICES: CategoryChoice[] = [
  { value: UNCATEGORIZED, label: UNCATEGORIZED_LABEL },
];

function openSite(url: string) {
  void openUrl(url).catch((error) => {
    toast.error("Could not open site", { description: String(error) });
  });
}

/**
 * Memoized: a page is 50 of these, each with a dropdown, so a render that changed nothing
 * about a row (a column drag, another row's checkbox) must not touch it. Column widths come
 * from the table's CSS variables rather than props for the same reason.
 */
export const SessionTableRow = memo(function SessionTableRow({
  row,
  selected,
  picker,
  onToggleRow,
  onCategoryChange,
  onAcceptAiLabel,
}: {
  row: SessionRow;
  selected: boolean;
  picker: CategoryPicker;
  onToggleRow: (id: number, checked: boolean) => void;
  onCategoryChange: (row: SessionRow, value: string) => void;
  onAcceptAiLabel: (row: SessionRow) => void;
}) {
  const path =
    (row.category_id !== null && picker.pathById.get(row.category_id)) ||
    UNCATEGORIZED_LABEL;
  const siteUrl = row.url;

  return (
    <TableRow>
      <TableCell>
        <Checkbox
          checked={selected}
          onCheckedChange={(checked) => onToggleRow(row.id, checked === true)}
          aria-label={`Select session ${row.id}`}
        />
      </TableCell>
      <TableCell className="truncate">
        <Tooltip>
          <TooltipTrigger className="truncate text-left">
            {row.app_name}
          </TooltipTrigger>
          <TooltipContent className="cursor-text">
            {row.source_key}
          </TooltipContent>
        </Tooltip>
      </TableCell>
      <TableCell>
        <div className="flex min-w-0 items-center gap-1">
          <CreatableCategoryCombobox
            picker={picker}
            extraChoices={UNCATEGORIZED_CHOICES}
            value={String(row.category_id ?? UNCATEGORIZED)}
            onValueChange={(value) => onCategoryChange(row, value)}
            ariaLabel="Category"
            size="sm"
            className="h-6 min-w-0 flex-1 px-1.5 text-xs"
            title={
              row.category_is_pinned
                ? `${path} — pinned to this activity`
                : row.category_source === "rule"
                  ? `${path} — matched by a rule`
                  : row.category_source === "ai"
                    ? `${path} — suggested by AI`
                    : path
            }
          >
            <span className="flex min-w-0 flex-1 items-center gap-1.5 text-left">
              <span
                className="size-2 shrink-0 rounded-full"
                style={{
                  backgroundColor:
                    row.category_color ?? "var(--muted-foreground)",
                }}
              />
              <span className="truncate">
                {row.category_name ?? UNCATEGORIZED_LABEL}
              </span>
            </span>
          </CreatableCategoryCombobox>
          {row.category_source === "ai" && (
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label="Accept AI label"
              title="Accept this AI label"
              onClick={() => onAcceptAiLabel(row)}
            >
              <Check />
            </Button>
          )}
        </div>
      </TableCell>
      <TableCell className="hidden truncate font-mono text-xs @4xl:table-cell">
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
            {row.title || row.url || row.app_name}
          </TooltipTrigger>
          <TooltipContent className="max-w-lg cursor-text whitespace-pre-wrap wrap-break-word">
            {[row.title, row.url].filter(Boolean).join("\n") || row.app_name}
          </TooltipContent>
        </Tooltip>
      </TableCell>
      <TableCell className="hidden text-muted-foreground tabular-nums @2xl:table-cell">
        {formatTimestamp(row.start_time)}
      </TableCell>
      <TableCell className="text-right tabular-nums">
        {formatDuration(row.start_time, row.end_time)}
      </TableCell>
    </TableRow>
  );
});
