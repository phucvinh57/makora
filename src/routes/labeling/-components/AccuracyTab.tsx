import { useQuery } from "@tanstack/react-query";
import { Line, LineChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { useMemo, useState } from "react";
import { aiAccuracyReport } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { AiAccuracyReport, ConfidenceBucket } from "@/types";
import { RANGE_PRESETS } from "../../statistics/-components/constants";
import {
  computeRange,
  formatDay,
  type RangePreset,
} from "../../statistics/-components/date";

function percent(value: number | null): string {
  return value == null ? "—" : `${Math.round(value * 100)}%`;
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

function bucketLabel(bucket: ConfidenceBucket) {
  return `${Math.round(bucket.lower * 100)}–${Math.round(bucket.upper * 100)}%`;
}

function thresholdBucket(
  buckets: ConfidenceBucket[],
  threshold: number,
): ConfidenceBucket | undefined {
  return buckets.find(
    (bucket) =>
      threshold >= bucket.lower &&
      (threshold < bucket.upper || bucket.upper === 1),
  );
}

function ConfidenceTable({ report }: { report: AiAccuracyReport }) {
  const threshold = report.current_confidence_threshold;
  const marked = thresholdBucket(report.by_confidence, threshold);
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Precision by confidence</CardTitle>
        <CardDescription>
          The current {Math.round(threshold * 100)}% threshold is marked. These
          buckets show where predictions become dependable.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Confidence</TableHead>
              <TableHead>Predictions</TableHead>
              <TableHead>Precision</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {report.by_confidence.map((bucket) => (
              <TableRow key={`${bucket.lower}-${bucket.upper}`}>
                <TableCell className="font-medium">
                  {bucketLabel(bucket)}
                  {bucket === marked ? (
                    <span className="ml-2 text-xs text-primary">current</span>
                  ) : null}
                </TableCell>
                <TableCell>{bucket.count.toLocaleString()}</TableCell>
                <TableCell>{percent(bucket.precision)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {marked?.precision != null ? (
          <p className="m-0 text-sm text-muted-foreground">
            Predictions between {Math.round(marked.lower * 100)}% and{" "}
            {Math.round(marked.upper * 100)}% are right{" "}
            {percent(marked.precision)}. Consider raising the threshold if this
            band is not dependable.
          </p>
        ) : (
          <p className="m-0 text-sm text-muted-foreground">
            There are not enough reviewed predictions in the current threshold
            band to make a recommendation yet.
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function TrendChart({ report }: { report: AiAccuracyReport }) {
  const latestRun = report.runs[report.runs.length - 1];
  const data = useMemo(() => {
    const points = [
      ...report.daily.map((day) => ({
        key: `day:${day.day}`,
        timestamp: day.day,
        label: formatDay(day.day),
        reviewed_precision: day.precision == null ? null : day.precision * 100,
        holdout_accuracy: null,
      })),
      ...report.runs.map((run) => {
        const day = run.recorded_at.slice(0, 10);
        return {
          key: `run:${run.recorded_at}`,
          timestamp: run.recorded_at,
          label: formatDay(day),
          reviewed_precision: null,
          holdout_accuracy:
            run.holdout_accuracy == null ? null : run.holdout_accuracy * 100,
        };
      }),
    ];
    return points.sort((left, right) =>
      left.timestamp.localeCompare(right.timestamp),
    );
  }, [report.daily, report.runs]);

  const chartConfig: ChartConfig = {
    reviewed_precision: {
      label: "Reviewed precision",
      color: "var(--chart-1)",
    },
    holdout_accuracy: {
      label: "Holdout accuracy",
      color: "var(--chart-2)",
    },
  };

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Accuracy over time</CardTitle>
        <CardDescription>
          Reviewed precision is a lower bound because corrections are what get
          reviewed. Holdout accuracy is measured on training data held out one
          key at a time.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Nothing to show for this range.
          </p>
        ) : (
          <ChartContainer config={chartConfig} className="h-64 w-full">
            <LineChart data={data}>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="label"
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                fontSize={11}
              />
              <YAxis
                domain={[0, 100]}
                tickLine={false}
                axisLine={false}
                tickFormatter={(value) => `${value}%`}
                fontSize={11}
              />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    formatter={(value) => (
                      <span className="font-mono font-medium tabular-nums text-foreground">
                        {Number(value).toFixed(0)}%
                      </span>
                    )}
                  />
                }
              />
              <Line
                dataKey="reviewed_precision"
                type="monotone"
                connectNulls={false}
                stroke="var(--color-reviewed_precision)"
                strokeWidth={2}
                dot={{ r: 3 }}
              />
              <Line
                dataKey="holdout_accuracy"
                type="monotone"
                connectNulls={false}
                stroke="var(--color-holdout_accuracy)"
                strokeWidth={2}
                dot={{ r: 3 }}
              />
            </LineChart>
          </ChartContainer>
        )}
        {latestRun ? (
          <p className="m-0 mt-3 text-xs text-muted-foreground">
            Latest run: holdout {percent(latestRun.holdout_accuracy)}
            {latestRun.manual_holdout_accuracy != null
              ? ` · manual-labels-only ${percent(latestRun.manual_holdout_accuracy)}`
              : " · no manual-only sample yet"}
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}

function Confusions({ report }: { report: AiAccuracyReport }) {
  const maxCount = Math.max(...report.confusions.map((item) => item.count), 0);
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Top confusions</CardTitle>
        <CardDescription>
          Categories the model most often confuses.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {report.confusions.length === 0 ? (
          <p className="py-8 text-center text-sm text-muted-foreground">
            No corrected category pairs in this range.
          </p>
        ) : (
          <ul className="flex flex-col gap-2.5">
            {report.confusions.slice(0, 10).map((item) => (
              <li
                key={`${item.predicted_category_name}:${item.chosen_category_name}`}
                className="flex items-center gap-3"
              >
                <span
                  className="w-44 shrink-0 truncate text-sm"
                  title={`${item.predicted_category_name} → ${item.chosen_category_name}`}
                >
                  {item.predicted_category_name} → {item.chosen_category_name}
                </span>
                <span className="relative h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
                  <span
                    className="absolute inset-y-0 left-0 rounded-full bg-chart-1"
                    style={{
                      width: `${maxCount > 0 ? (item.count / maxCount) * 100 : 0}%`,
                    }}
                  />
                </span>
                <span className="w-12 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
                  {item.count.toLocaleString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function SurfaceBreakdown({ report }: { report: AiAccuracyReport }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Precision by surface</CardTitle>
        <CardDescription>
          Session pins, rule suggestions, and cluster proposals.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Surface</TableHead>
              <TableHead>Reviewed</TableHead>
              <TableHead>Precision</TableHead>
              <TableHead>Coverage</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {report.by_surface.map((surface) => (
              <TableRow key={surface.surface}>
                <TableCell className="capitalize">{surface.surface}</TableCell>
                <TableCell>
                  {(
                    surface.totals.agree_count +
                    surface.totals.corrected_count +
                    surface.totals.rejected_count
                  ).toLocaleString()}
                </TableCell>
                <TableCell>{percent(surface.precision)}</TableCell>
                <TableCell>{percent(surface.coverage)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

export function AccuracyTab() {
  const [range, setRange] = useState<RangePreset>("7d");
  const [includeBulk, setIncludeBulk] = useState(false);
  const { start, end } = useMemo(() => computeRange(range), [range]);
  const reportQuery = useQuery({
    queryKey: ["aiAccuracyReport", start, end, includeBulk],
    queryFn: () => aiAccuracyReport(start, end, includeBulk),
  });
  const report = reportQuery.data;
  const loading = reportQuery.isFetching;
  const error = reportQuery.error ? String(reportQuery.error) : null;

  return (
    <div className="flex flex-col gap-6 pb-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="mr-auto">
          <h2 className="m-0 text-base font-semibold">AI accuracy</h2>
          <p className="m-0 text-sm text-muted-foreground">
            Forward-looking evidence about your categorization flow.
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
            onClick={() => reportQuery.refetch()}
          >
            Refresh
          </Button>
        </div>
      </header>

      <label
        htmlFor="include-bulk"
        className="flex items-center gap-2 text-sm text-muted-foreground"
      >
        <Checkbox
          id="include-bulk"
          checked={includeBulk}
          onCheckedChange={(checked) => setIncludeBulk(checked === true)}
        />
        Include bulk accepts
        {report
          ? ` (${report.excluded_bulk_count.toLocaleString()} excluded by default)`
          : ""}
      </label>

      {error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load AI accuracy: {error}
        </div>
      ) : null}

      {!report && loading ? (
        <p className="py-8 text-center text-sm text-muted-foreground">
          Loading accuracy…
        </p>
      ) : report && report.totals.total_count === 0 ? (
        <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
          No verdicts recorded yet. Precision is measured from here forward.
        </div>
      ) : report ? (
        <>
          <div className="grid grid-cols-2 gap-3 @3xl/main:grid-cols-4">
            <StatTile
              label="Reviewed precision"
              value={percent(report.precision)}
            />
            <StatTile
              label="Time-weighted precision"
              value={percent(report.time_weighted_precision)}
            />
            <StatTile
              label="Reviewed coverage"
              value={percent(report.coverage)}
            />
            <StatTile
              label="Verdicts recorded"
              value={report.totals.total_count.toLocaleString()}
            />
          </div>
          <TrendChart report={report} />
          <ConfidenceTable report={report} />
          <div className="grid gap-6 @4xl/main:grid-cols-2">
            <Confusions report={report} />
            <SurfaceBreakdown report={report} />
          </div>
        </>
      ) : null}
    </div>
  );
}
