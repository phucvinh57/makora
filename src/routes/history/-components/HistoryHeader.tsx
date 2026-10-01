import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useMemo } from "react";
import type { CategoryPicker } from "@/components/CategoryOptions";
import {
  ALL_CATEGORIES,
  ALL_SOURCES,
  MANUAL_SOURCE,
  UNCATEGORIZED,
} from "./constants";
import type { HistoryFilters } from "./filters";
import { TimeRangeFilter } from "./TimeRangeFilter";

export function HistoryHeader({
  searchInput,
  onSearchInputChange,
  filters,
  onFiltersChange,
  picker,
  loading,
  onRefresh,
}: {
  searchInput: string;
  onSearchInputChange: (value: string) => void;
  filters: HistoryFilters;
  onFiltersChange: (patch: Partial<HistoryFilters>) => void;
  picker: CategoryPicker;
  loading: boolean;
  onRefresh: () => void;
}) {
  const categoryItems = useMemo(
    () => ({
      [ALL_CATEGORIES]: "All categories",
      [UNCATEGORIZED]: "Uncategorized",
      ...picker.valueLabels,
    }),
    [picker.valueLabels],
  );

  return (
    <header className="mb-4 flex flex-wrap items-center gap-2">
      <div className="mr-auto">
        <h1 className="m-0 text-lg font-semibold">History</h1>
        <p className="m-0 text-sm text-muted-foreground">App-focus history</p>
      </div>
      <Input
        type="text"
        value={searchInput}
        onChange={(e) => onSearchInputChange(e.target.value)}
        placeholder="Search app, title, or site…"
        className="h-8 min-w-40 flex-1 basis-48 sm:max-w-64"
      />
      <Select
        value={filters.category}
        onValueChange={(value) =>
          onFiltersChange({ category: value ?? ALL_CATEGORIES })
        }
        items={categoryItems}
      >
        <SelectTrigger
          size="sm"
          className="w-40"
          title="Picking a category also shows its subcategories"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={ALL_CATEGORIES}>All categories</SelectItem>
          <SelectItem value={UNCATEGORIZED}>Uncategorized</SelectItem>
          {picker.options}
        </SelectContent>
      </Select>
      <Select
        value={filters.source}
        onValueChange={(value) =>
          onFiltersChange({ source: value ?? ALL_SOURCES })
        }
        items={{
          [ALL_SOURCES]: "All sources",
          [MANUAL_SOURCE]: "Manual",
        }}
      >
        <SelectTrigger size="sm" className="w-32" aria-label="Category source">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={ALL_SOURCES}>All sources</SelectItem>
          <SelectItem value={MANUAL_SOURCE}>Manual</SelectItem>
        </SelectContent>
      </Select>
      <TimeRangeFilter
        range={filters.range}
        onChange={(range) => onFiltersChange({ range })}
      />
      <Button
        variant="outline"
        size="sm"
        disabled={loading}
        onClick={onRefresh}
        aria-label="Refresh history"
      >
        <RefreshCw className={loading ? "animate-spin" : undefined} />
      </Button>
    </header>
  );
}
