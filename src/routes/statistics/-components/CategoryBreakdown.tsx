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
import { Skeleton } from "@/components/ui/skeleton";
import {
  buildCategoryTree,
  type CategoryNode,
  UNCATEGORIZED,
  UNCATEGORIZED_COLOR,
  UNCATEGORIZED_LABEL,
} from "@/categories";
import { formatSeconds } from "@/duration";
import type { ActivityStats, Category } from "@/types";

interface Slice {
  key: string;
  name: string;
  value: number;
  color: string;
  depth: number;
}

/**
 * One row per category in tree order, each carrying its own time *plus* everything its
 * subcategories spent — a parent that is only ever used through its children still shows
 * where that time went.
 */
function rollUp(
  nodes: CategoryNode[],
  secondsById: Map<number, number>,
): { rows: Slice[]; total: number } {
  const rows: Slice[] = [];
  let total = 0;
  for (const node of nodes) {
    const nested = rollUp(node.children, secondsById);
    const value = (secondsById.get(node.category.id) ?? 0) + nested.total;
    total += value;
    if (value === 0) continue;
    rows.push({
      key: String(node.category.id),
      name: node.category.name,
      value,
      color: node.category.color,
      depth: node.depth,
    });
    rows.push(...nested.rows);
  }
  return { rows, total };
}

export function CategoryBreakdown({
  stats,
  categories,
  categoriesLoading,
}: {
  stats: ActivityStats;
  categories: Category[];
  categoriesLoading: boolean;
}) {
  const { rows, topLevel } = useMemo(() => {
    const secondsById = new Map<number, number>();
    for (const c of stats.categories) {
      if (c.category_id !== null)
        secondsById.set(c.category_id, c.total_seconds);
    }
    const tree = buildCategoryTree(categories);
    const { rows } = rollUp(tree, secondsById);

    const uncategorized = stats.categories.find((c) => c.category_id === null);
    const untracked: Slice[] = uncategorized
      ? [
          {
            key: UNCATEGORIZED,
            name: UNCATEGORIZED_LABEL,
            value: uncategorized.total_seconds,
            color: UNCATEGORIZED_COLOR,
            depth: 0,
          },
        ]
      : [];

    // The pie splits the whole range once, so only the roots go in it — a subcategory's
    // time is already inside its parent's slice.
    return {
      rows: [...rows, ...untracked],
      topLevel: [...rows.filter((r) => r.depth === 0), ...untracked],
    };
  }, [stats.categories, categories]);

  const chartConfig = useMemo<ChartConfig>(
    () =>
      Object.fromEntries(
        topLevel.map((c) => [c.key, { label: c.name, color: c.color }]),
      ),
    [topLevel],
  );

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Time per category</CardTitle>
        <CardDescription>
          How tracked time splits across your categories; subcategory time
          counts towards its parent.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {categoriesLoading ? (
          <Skeleton className="h-52 rounded-md" />
        ) : rows.length === 0 ? (
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
                  data={topLevel}
                  dataKey="value"
                  nameKey="name"
                  innerRadius={55}
                  outerRadius={85}
                  paddingAngle={2}
                  cornerRadius={4}
                >
                  {topLevel.map((c) => (
                    <Cell key={c.key} fill={c.color} />
                  ))}
                </Pie>
              </PieChart>
            </ChartContainer>
            <ul className="flex min-w-0 flex-1 flex-col gap-1.5">
              {rows.map((c) => (
                <li
                  key={c.key}
                  className="flex items-center justify-between gap-3 text-sm"
                  style={{ paddingLeft: c.depth * 14 }}
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <span
                      className="size-2.5 shrink-0 rounded-full"
                      style={{ backgroundColor: c.color }}
                    />
                    <span
                      className={
                        c.depth > 0
                          ? "truncate text-muted-foreground"
                          : "truncate"
                      }
                    >
                      {c.name}
                    </span>
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
