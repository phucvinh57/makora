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
import type { Category } from "@/types";
import { ALL_CATEGORIES, UNCATEGORIZED } from "./constants";

export function HistoryHeader({
  searchInput,
  onSearchInputChange,
  categoryFilter,
  onCategoryFilterChange,
  categories,
  loading,
  onRefresh,
  onRecategorizeClick,
}: {
  searchInput: string;
  onSearchInputChange: (value: string) => void;
  categoryFilter: string;
  onCategoryFilterChange: (value: string) => void;
  categories: Category[];
  loading: boolean;
  onRefresh: () => void;
  onRecategorizeClick: () => void;
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
          ...Object.fromEntries(
            categories.map((category) => [String(category.id), category.name]),
          ),
        }}
      >
        <SelectTrigger size="sm" className="w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={ALL_CATEGORIES}>All categories</SelectItem>
          <SelectItem value={UNCATEGORIZED}>Uncategorized</SelectItem>
          {categories.map((category) => (
            <SelectItem key={category.id} value={String(category.id)}>
              {category.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button
        variant="outline"
        size="sm"
        disabled={loading}
        onClick={onRefresh}
        aria-label="Refresh history"
      >
        <RefreshCw className={loading ? "animate-spin" : undefined} />
      </Button>
      <Button variant="outline" size="sm" onClick={onRecategorizeClick}>
        Recategorize
      </Button>
    </header>
  );
}
