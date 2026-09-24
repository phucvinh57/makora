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
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import type { Category, WorkSession } from "@/types";
import {
  ALL_CATEGORIES,
  ALL_SOURCES,
  MANUAL_SOURCE,
  UNCATEGORIZED,
} from "./constants";
import type { TimeRange } from "./timeRange";
import { TimeRangeFilter } from "./TimeRangeFilter";

export function HistoryHeader({
  searchInput,
  onSearchInputChange,
  categoryFilter,
  onCategoryFilterChange,
  sourceFilter,
  onSourceFilterChange,
  categories,
  range,
  rangeSession,
  onRangeChange,
  loading,
  onRefresh,
}: {
  searchInput: string;
  onSearchInputChange: (value: string) => void;
  categoryFilter: string;
  onCategoryFilterChange: (value: string) => void;
  sourceFilter: string;
  onSourceFilterChange: (value: string) => void;
  categories: Category[];
  range: TimeRange;
  rangeSession?: WorkSession | null;
  onRangeChange: (range: TimeRange) => void;
  loading: boolean;
  onRefresh: () => void;
}) {
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
        value={categoryFilter}
        onValueChange={(value) =>
          onCategoryFilterChange(value ?? ALL_CATEGORIES)
        }
        items={{
          [ALL_CATEGORIES]: "All categories",
          [UNCATEGORIZED]: "Uncategorized",
          ...categoryValueLabels(categories),
        }}
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
          {categoryOptions(categories)}
        </SelectContent>
      </Select>
      <Select
        value={sourceFilter}
        onValueChange={(value) => onSourceFilterChange(value ?? ALL_SOURCES)}
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
        range={range}
        session={rangeSession}
        onChange={onRangeChange}
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
