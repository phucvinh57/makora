import { addDays, format, parseISO, startOfDay, subDays } from "date-fns";
import { useMemo } from "react";
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import {
  estimateCost,
  findPrice,
  formatCost,
  formatCostExact,
} from "@/aiUsage";
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
} from "@/components/ui/chart";
import type { DailyModelTokens, ModelPrice } from "@/types";

const chartConfig: ChartConfig = {
  cost: { label: "Estimated cost", color: "var(--chart-1)" },
};

type Point = {
  label: string;
  cost: number;
  models: { name: string; cost: number }[];
};

function formatTick(value: number): string {
  if (value === 0) return "$0";
  return value < 0.01 ? `$${value.toFixed(3)}` : formatCost(value);
}

/** Every day from the period's first to today, so days without runs show as gaps. */
function dayKeys(daily: DailyModelTokens[], days: number | null): string[] {
  const today = startOfDay(new Date());
  const first =
    days !== null
      ? startOfDay(subDays(new Date(), days))
      : daily.length > 0
        ? parseISO(daily[0].day)
        : today;
  const keys: string[] = [];
  for (let day = first; day <= today; day = addDays(day, 1)) {
    keys.push(format(day, "yyyy-MM-dd"));
  }
  return keys;
}

function CostTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: { payload: Point }[];
}) {
  const point = payload?.[0]?.payload;
  if (!active || !point) return null;
  return (
    <div className="grid min-w-36 gap-1 rounded-lg border bg-background px-2.5 py-1.5 text-xs shadow-xl">
      <div className="flex justify-between gap-4 font-medium">
        <span>{point.label}</span>
        <span className="tabular-nums" title={formatCostExact(point.cost)}>
          {formatCost(point.cost)}
        </span>
      </div>
      {point.models.map(({ name, cost }) => (
        <div
          key={name}
          className="flex justify-between gap-4 text-muted-foreground"
        >
          <span className="max-w-48 truncate">{name}</span>
          <span className="tabular-nums text-foreground">
            {formatCost(cost)}
          </span>
        </div>
      ))}
    </div>
  );
}

export function AiCostChart({
  daily,
  prices,
  days,
}: {
  daily: DailyModelTokens[];
  prices: ModelPrice[];
  days: number | null;
}) {
  const data = useMemo(() => {
    const byDay = new Map<string, Map<string, number>>();
    for (const row of daily) {
      const price = findPrice(prices, row.provider, row.model);
      if (!price) continue;
      const models = byDay.get(row.day) ?? new Map<string, number>();
      models.set(
        row.model,
        (models.get(row.model) ?? 0) + estimateCost(row, price),
      );
      byDay.set(row.day, models);
    }
    return dayKeys(daily, days).map((day): Point => {
      const models = [...(byDay.get(day) ?? new Map<string, number>())]
        .map(([name, cost]) => ({ name, cost }))
        .sort((a, b) => b.cost - a.cost);
      return {
        label: format(parseISO(day), "MMM d"),
        cost: models.reduce((sum, model) => sum + model.cost, 0),
        models,
      };
    });
  }, [daily, prices, days]);

  return (
    <ChartContainer config={chartConfig} className="h-44 w-full">
      <BarChart data={data} margin={{ left: 0, right: 0 }}>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="label"
          tickLine={false}
          axisLine={false}
          tickMargin={8}
          minTickGap={16}
          fontSize={11}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          width={48}
          fontSize={11}
          tickFormatter={formatTick}
        />
        <ChartTooltip cursor={{ fillOpacity: 0.4 }} content={<CostTooltip />} />
        <Bar
          dataKey="cost"
          fill="var(--color-cost)"
          radius={[4, 4, 0, 0]}
          maxBarSize={24}
        />
      </BarChart>
    </ChartContainer>
  );
}
