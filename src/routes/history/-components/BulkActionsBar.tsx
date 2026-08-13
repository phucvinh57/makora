import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { Category } from "@/types";

export function BulkActionsBar({
  selectedCount,
  categories,
  bulkCategory,
  onBulkCategoryChange,
  applyToAll,
  onApplyToAllChange,
  onApply,
  applyPending,
  onClear,
}: {
  selectedCount: number;
  categories: Category[];
  bulkCategory: string;
  onBulkCategoryChange: (value: string) => void;
  applyToAll: boolean;
  onApplyToAllChange: (value: boolean) => void;
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
        items={Object.fromEntries(
          categories.map((category) => [String(category.id), category.name]),
        )}
      >
        <SelectTrigger size="sm" className="w-40">
          <SelectValue placeholder="Set category…" />
        </SelectTrigger>
        <SelectContent>
          {categories.map((category) => (
            <SelectItem key={category.id} value={String(category.id)}>
              {category.name}
            </SelectItem>
          ))}
        </SelectContent>
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
      {/* Off by default: a selection made under a search/time filter means
        * "these activities", not "this app, forever" — the same site can
        * serve a different purpose an hour later. Turning it on is the
        * key-wide behavior, which also writes a rule that keeps applying to
        * future activity. */}
      <div className="ml-auto flex items-center gap-2 text-muted-foreground">
        <Checkbox
          id="bulk-apply-to-all"
          checked={applyToAll}
          onCheckedChange={(checked) => onApplyToAllChange(checked === true)}
        />
        <label htmlFor="bulk-apply-to-all" className="cursor-pointer">
          Also apply to all other activity from the same apps/sites
        </label>
      </div>
    </div>
  );
}
