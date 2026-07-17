import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { activityStats } from "@/api";
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

function StatisticsPage() {
  const [range, setRange] = useState<RangePreset>("7d");
  const { start, end } = useMemo(() => computeRange(range), [range]);

  const statsQuery = useQuery({
    queryKey: ["activityStats", start, end],
    queryFn: () => activityStats(start, end),
  });

  const stats = statsQuery.data;
  const loading = statsQuery.isFetching;
  const error = statsQuery.error ? String(statsQuery.error) : null;

  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto subtle-scrollbar box-border">
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

      {error && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load statistics: {error}
        </div>
      )}

      {!stats && loading ? (
        <StatisticsSkeleton />
      ) : stats && stats.session_count === 0 ? (
        <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
          No activity tracked in this range yet.
        </div>
      ) : stats ? (
        <>
          <SummaryTiles stats={stats} />
          <div className="grid gap-6 @4xl/main:grid-cols-2">
            <CategoryBreakdown stats={stats} />
            <TopAppsBreakdown stats={stats} />
          </div>
          <DailyTrend stats={stats} />
        </>
      ) : null}
    </div>
  );
}
