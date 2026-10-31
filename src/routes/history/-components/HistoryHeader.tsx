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
import { CategoryCombobox } from "@/components/CategoryCombobox";
import type {
  CategoryChoice,
  CategoryPicker,
} from "@/components/CategoryOptions";
import { ALL_CATEGORIES, ALL_SOURCES, UNCATEGORIZED } from "./constants";
import {
  type HistoryFilters,
  parseSourceFilter,
  SOURCE_CHOICE_LABELS,
} from "./filters";
import { TimeRangeFilter } from "./TimeRangeFilter";

const CATEGORY_FILTER_CHOICES: CategoryChoice[] = [
  { value: ALL_CATEGORIES, label: "All categories" },
  { value: UNCATEGORIZED, label: "Uncategorized" },
];

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
      <CategoryCombobox
        picker={picker}
        extraChoices={CATEGORY_FILTER_CHOICES}
        value={filters.category}
        onValueChange={(category) => onFiltersChange({ category })}
        ariaLabel="Category"
        size="sm"
        className="w-40"
        title="Picking a category also shows its subcategories"
      />
      <Select
        value={filters.source}
        onValueChange={(value) =>
          onFiltersChange({ source: parseSourceFilter(value) ?? ALL_SOURCES })
        }
        items={SOURCE_CHOICE_LABELS}
      >
        <SelectTrigger size="sm" className="w-32" aria-label="Category source">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {Object.entries(SOURCE_CHOICE_LABELS).map(([value, label]) => (
            <SelectItem key={value} value={value}>
              {label}
            </SelectItem>
          ))}
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
