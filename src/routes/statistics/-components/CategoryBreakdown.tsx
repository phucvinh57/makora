import { useMemo } from "react";
import { Cell, Pie, PieChart } from "recharts";
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
import { UNCATEGORIZED_COLOR } from "./constants";

export function CategoryBreakdown({ stats }: { stats: ActivityStats }) {
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
