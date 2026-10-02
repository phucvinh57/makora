import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useState } from "react";
import { activity } from "@/api";
import { Button } from "@/components/ui/button";
import { useCategories } from "@/hooks/useCategories";
import { cn } from "@/lib/utils";
import { queryKeys } from "@/queries";
import type { ActivityStats, Category } from "@/types";
import { CategoryBreakdown } from "./-components/CategoryBreakdown";
import { RANGE_PRESETS } from "./-components/constants";
import { DailyTrend } from "./-components/DailyTrend";
import { computeRange, type RangePreset } from "./-components/date";
import { StatisticsSkeleton } from "./-components/StatisticsSkeleton";
import { SummaryTiles } from "./-components/SummaryTiles";
import { TopAppsBreakdown } from "./-components/TopAppsBreakdown";

export const Route = createFileRoute("/statistics")({
  component: StatisticsPage,
});

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {message}
    </div>
  );
}

function StatisticsBody({
  stats,
  categories,
}: {
  stats: ActivityStats;
  categories: Category[];
}) {
  if (stats.session_count === 0) {
    return (
      <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
        No activity tracked in this range yet.
      </div>
    );
  }

  return (
    <>
      <SummaryTiles stats={stats} />
      <div className="grid gap-6 @4xl/main:grid-cols-2">
        <CategoryBreakdown stats={stats} categories={categories} />
        <TopAppsBreakdown stats={stats} />
      </div>
      <DailyTrend stats={stats} />
    </>
  );
}

function StatisticsPage() {
  const [range, setRange] = useState<RangePreset>("7d");

  const statsQuery = useQuery({
    // The preset is the key and the dates are worked out per fetch: the app stays open for
    // days, so "Today" must mean the day it is now, not the day this page first rendered.
    queryKey: [...queryKeys.activityStats, range],
    queryFn: () => {
      const { start, end } = computeRange(range);
      return activity.stats(start, end);
    },
    placeholderData: keepPreviousData,
  });
  const { categories, error: categoriesError } = useCategories();

  const stats = statsQuery.data;
  const loading = statsQuery.isFetching;

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="mr-auto">
          <h1 className="m-0 text-lg font-semibold">Statistics</h1>
          <p className="m-0 text-sm text-muted-foreground">
            Where your time went
          </p>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1 rounded-md border p-0.5">
            {RANGE_PRESETS.map((preset) => (
              <Button
                key={preset.value}
                type="button"
                variant={range === preset.value ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setRange(preset.value)}
              >
                {preset.label}
              </Button>
            ))}
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={loading}
            onClick={() => statsQuery.refetch()}
            aria-label="Refresh statistics"
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} />
          </Button>
        </div>
      </header>

      {statsQuery.error && (
        <ErrorBanner
          message={`Failed to load statistics: ${String(statsQuery.error)}`}
        />
      )}
      {categoriesError && (
        <ErrorBanner
          message={`Failed to load categories: ${String(categoriesError)}`}
        />
      )}

      {stats ? (
        <div
          className={cn(
            "flex flex-col gap-6 transition-opacity",
            statsQuery.isPlaceholderData && "opacity-60",
          )}
        >
          <StatisticsBody stats={stats} categories={categories} />
        </div>
      ) : statsQuery.isPending ? (
        <StatisticsSkeleton />
      ) : null}
    </div>
  );
}
