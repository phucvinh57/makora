import { useMemo } from "react";
import { Bar, BarChart, CartesianGrid, XAxis } from "recharts";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/components/ui/chart";
import { formatSeconds } from "@/duration";
import type { ActivityStats } from "@/types";
import { formatDay } from "./date";

const chartConfig: ChartConfig = {
  total_seconds: { label: "Tracked time", color: "var(--chart-1)" },
};

export function DailyTrend({ stats }: { stats: ActivityStats }) {
  const data = useMemo(
    () =>
      stats.daily.map((d) => ({
        label: formatDay(d.day),
        total_seconds: d.total_seconds,
      })),
    [stats.daily],
  );

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
