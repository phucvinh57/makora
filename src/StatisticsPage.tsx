import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  XAxis,
} from "recharts";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import { Skeleton } from "@/components/ui/skeleton";
import { activityStats } from "./api";
import { formatSeconds } from "./duration";
import type { ActivityStats } from "./types";

type RangePreset = "today" | "7d" | "30d";

const RANGE_PRESETS: { value: RangePreset; label: string }[] = [
  { value: "today", label: "Today" },
  { value: "7d", label: "7 days" },
  { value: "30d", label: "30 days" },
];

// How many calendar days back each preset's range starts (inclusive of
// today), e.g. "7 days" spans today and the 6 days before it.
const RANGE_DAYS_BACK: Record<RangePreset, number> = {
  today: 0,
  "7d": 6,
  "30d": 29,
};

// Slice color for sessions with no resolved category — deliberately not one
// of the app's own category colors (those are user-assigned and arbitrary),
// just a neutral gray that reads on both light and dark surfaces.
const UNCATEGORIZED_COLOR = "#9ca3af";

function localMidnight(daysAgo: number): Date {
  const date = new Date();
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() - daysAgo);
  return date;
}

// `start`/`end` are RFC 3339 UTC bounds computed against local midnight, so
// "Today" lines up with the user's calendar day rather than the UTC one.
// The range is half-open — `end` is tomorrow's local midnight, matching the
// backend's `start_time >= start AND start_time < end` filter.
function computeRange(preset: RangePreset): { start: string; end: string } {
  return {
    start: localMidnight(RANGE_DAYS_BACK[preset]).toISOString(),
    end: localMidnight(-1).toISOString(),
  };
}

// `day` is a local-calendar date string like "2024-06-15" (see
// `date(start_time, 'localtime')` in `activity_stats`). Building the `Date`
// from explicit local components — rather than `new Date(day)`, which parses
// as UTC midnight — avoids the label shifting a day off in negative UTC
// offsets.
function formatDay(day: string): string {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

export default function StatisticsPage() {
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

function StatisticsSkeleton() {
  return (
    <div className="flex flex-col gap-6">
      <div className="grid grid-cols-2 gap-3 @3xl/main:grid-cols-4">
        {Array.from({ length: 4 }, (_, i) => (
          <Skeleton key={i} className="h-20 rounded-md" />
        ))}
      </div>
      <div className="grid gap-6 @4xl/main:grid-cols-2">
        <Skeleton className="h-64 rounded-md" />
        <Skeleton className="h-64 rounded-md" />
      </div>
      <Skeleton className="h-56 rounded-md" />
    </div>
  );
}

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <Card size="sm">
      <CardContent>
        <p className="m-0 mb-1 text-xs font-medium text-muted-foreground">
          {label}
        </p>
        <p className="m-0 text-2xl font-semibold tabular-nums">{value}</p>
      </CardContent>
    </Card>
  );
}

function SummaryTiles({ stats }: { stats: ActivityStats }) {
  const busiestApp = stats.apps[0];
  const busiestDay = stats.daily.reduce<ActivityStats["daily"][number] | null>(
    (max, day) => (!max || day.total_seconds > max.total_seconds ? day : max),
    null,
  );

  return (
    <div className="grid grid-cols-2 gap-3 @3xl/main:grid-cols-4">
      <StatTile
        label="Total tracked time"
        value={formatSeconds(stats.total_seconds)}
      />
      <StatTile label="Sessions" value={stats.session_count.toLocaleString()} />
      <StatTile
        label="Busiest app/site"
        value={busiestApp ? busiestApp.label : "—"}
      />
      <StatTile
        label="Busiest day"
        value={busiestDay ? formatDay(busiestDay.day) : "—"}
      />
    </div>
  );
}

function CategoryBreakdown({ stats }: { stats: ActivityStats }) {
  const data = stats.categories.map((c) => ({
    key: c.category_id !== null ? String(c.category_id) : "uncategorized",
    name: c.name ?? "Uncategorized",
    value: c.total_seconds,
    color: c.color ?? UNCATEGORIZED_COLOR,
  }));

  const chartConfig = useMemo<ChartConfig>(
    () =>
      Object.fromEntries(
        data.map((c) => [c.key, { label: c.name, color: c.color }]),
      ),
    [data.map],
  );

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Time per category</CardTitle>
        <CardDescription>
          How tracked time splits across your categories.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Nothing to show for this range.
          </p>
        ) : (
          <div className="flex flex-col gap-4 @2xl:flex-row @2xl:items-center">
            <ChartContainer
              config={chartConfig}
              className="mx-auto size-52 shrink-0"
            >
              <PieChart>
                <ChartTooltip
                  content={
                    <ChartTooltipContent
                      nameKey="name"
                      formatter={(value, name) => (
                        <div className="flex w-full items-center justify-between gap-3">
                          <span className="text-muted-foreground">{name}</span>
                          <span className="font-mono font-medium tabular-nums text-foreground">
                            {formatSeconds(Number(value))}
                          </span>
                        </div>
                      )}
                    />
                  }
                />
                <Pie
                  data={data}
                  dataKey="value"
                  nameKey="name"
                  innerRadius={55}
                  outerRadius={85}
                  paddingAngle={2}
                  cornerRadius={4}
                >
                  {data.map((c) => (
                    <Cell key={c.key} fill={c.color} />
                  ))}
                </Pie>
              </PieChart>
            </ChartContainer>
            <ul className="flex min-w-0 flex-1 flex-col gap-1.5">
              {data.map((c) => (
                <li
                  key={c.key}
                  className="flex items-center justify-between gap-3 text-sm"
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <span
                      className="size-2.5 shrink-0 rounded-full"
                      style={{ backgroundColor: c.color }}
                    />
                    <span className="truncate">{c.name}</span>
                  </span>
                  <span className="shrink-0 tabular-nums text-muted-foreground">
                    {formatSeconds(c.value)}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function TopAppsBreakdown({ stats }: { stats: ActivityStats }) {
  const apps = stats.apps;
  const maxSeconds = apps.length
    ? Math.max(...apps.map((a) => a.total_seconds))
    : 0;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Top apps & sites</CardTitle>
        <CardDescription>
          Ranked by time spent, tinted by category.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {apps.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Nothing to show for this range.
          </p>
        ) : (
          <ul className="flex flex-col gap-2.5">
            {apps.map((app) => (
              <li
                key={`${app.kind}:${app.label}`}
                className="flex items-center gap-3"
              >
                <span
                  className="w-32 shrink-0 truncate text-sm sm:w-40"
                  title={app.label}
                >
                  {app.label}
                </span>
                <span className="relative h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
                  <span
                    className="absolute inset-y-0 left-0 rounded-full"
                    style={{
                      width: `${maxSeconds > 0 ? (app.total_seconds / maxSeconds) * 100 : 0}%`,
                      backgroundColor:
                        app.category_color ?? UNCATEGORIZED_COLOR,
                    }}
                  />
                </span>
                <span className="w-16 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
                  {formatSeconds(app.total_seconds)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function DailyTrend({ stats }: { stats: ActivityStats }) {
  const data = stats.daily.map((d) => ({
    label: formatDay(d.day),
    total_seconds: d.total_seconds,
  }));

  const chartConfig: ChartConfig = {
    total_seconds: { label: "Tracked time", color: "var(--chart-1)" },
  };

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Daily activity</CardTitle>
        <CardDescription>Total tracked time per day.</CardDescription>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Nothing to show for this range.
          </p>
        ) : (
          <ChartContainer config={chartConfig} className="h-56 w-full">
            <BarChart data={data}>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="label"
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                fontSize={11}
              />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    hideLabel={false}
                    formatter={(value) => (
                      <span className="font-mono font-medium tabular-nums text-foreground">
                        {formatSeconds(Number(value))}
                      </span>
                    )}
                  />
                }
              />
              <Bar
                dataKey="total_seconds"
                fill="var(--color-total_seconds)"
                radius={[4, 4, 0, 0]}
                maxBarSize={28}
              />
            </BarChart>
          </ChartContainer>
        )}
      </CardContent>
    </Card>
  );
}
