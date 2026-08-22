import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import type { Category } from "@/types";

export function BulkActionsBar({
  selectedCount,
  categories,
  bulkCategory,
  onBulkCategoryChange,
  onApply,
  applyPending,
  onClear,
}: {
  selectedCount: number;
  categories: Category[];
  bulkCategory: string;
  onBulkCategoryChange: (value: string) => void;
  onApply: () => void;
  applyPending: boolean;
  onClear: () => void;
}) {
  return (
    <div className="mb-3 flex flex-wrap items-center gap-2 rounded-md border bg-muted/50 px-3 py-2 text-sm">
      <span>{selectedCount} selected</span>
      <Select
        value={bulkCategory}
        onValueChange={(value) => onBulkCategoryChange(value ?? "")}
        items={categoryValueLabels(categories)}
      >
        <SelectTrigger size="sm" className="w-40">
          <SelectValue placeholder="Set category…" />
        </SelectTrigger>
        <SelectContent>{categoryOptions(categories)}</SelectContent>
      </Select>
      <Button
        size="sm"
        disabled={!bulkCategory || applyPending}
        onClick={onApply}
      >
        Apply
      </Button>
      <Button variant="outline" size="sm" onClick={onClear}>
        Clear selection
      </Button>
      <span className="ml-auto text-muted-foreground">
        Applies to the selected activities only
      </span>
    </div>
  );
}
