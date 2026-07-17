import { Card, CardContent } from "@/components/ui/card";
import { formatSeconds } from "@/duration";
import type { ActivityStats } from "@/types";
import { formatDay } from "./date";

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

export function SummaryTiles({ stats }: { stats: ActivityStats }) {
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
