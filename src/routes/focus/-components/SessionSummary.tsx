import { useQuery } from "@tanstack/react-query";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { workSessionStats } from "@/api";
import { formatSeconds } from "@/duration";
import type { WorkSession, WorkSessionStats } from "@/types";
import {
  OFF_TASK_COLOR,
  ON_TASK_COLOR,
  workSessionStatsKey,
} from "./constants";
import { TimelineStrip } from "./TimelineStrip";

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

function LegendEntry({
  color,
  label,
  seconds,
}: {
  color: string | null;
  label: string;
  seconds: number;
}) {
  return (
    <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
      <span
        className="size-2.5 shrink-0 rounded-full"
        style={{ backgroundColor: color ?? "var(--muted)" }}
      />
      {label}
      <span className="tabular-nums">{formatSeconds(seconds)}</span>
    </span>
  );
}

/** On/off/idle share of the window as one stacked bar — the aggregate the
 * timeline strip details chronologically, sharing its colors and legend. */
function ProportionBar({ stats }: { stats: WorkSessionStats }) {
  const parts = [
    { key: "on", seconds: stats.on_task_seconds, color: ON_TASK_COLOR },
    { key: "off", seconds: stats.off_task_seconds, color: OFF_TASK_COLOR },
    { key: "idle", seconds: stats.idle_seconds, color: "var(--muted)" },
  ].filter((part) => part.seconds > 0);
  if (parts.length === 0) return null;

  return (
    <div className="flex h-2 w-full gap-0.5 overflow-hidden rounded-full">
      {parts.map((part) => (
        <div
          key={part.key}
          className="rounded-[2px]"
          style={{
            flexGrow: part.seconds,
            flexBasis: 0,
            backgroundColor: part.color,
          }}
        />
      ))}
    </div>
  );
}

export function SessionSummary({ session }: { session: WorkSession }) {
  const statsQuery = useQuery({
    queryKey: workSessionStatsKey(session.id),
    queryFn: () => workSessionStats(session.id),
    // Live view while the session is running; static afterwards.
    refetchInterval: (query) =>
      query.state.data?.in_progress ? 15_000 : false,
  });
  const stats = statsQuery.data;

  if (statsQuery.error) {
    return (
      <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
        Failed to load session stats: {String(statsQuery.error)}
      </div>
    );
  }
  if (!stats) {
    return <Skeleton className="h-64 w-full" />;
  }

  const actualSeconds =
    (new Date(session.end_time).getTime() -
      new Date(session.start_time).getTime()) /
    1000;
  const maxAppSeconds = stats.apps.length
    ? Math.max(...stats.apps.map((a) => a.total_seconds))
    : 0;

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-3 @3xl/main:grid-cols-4">
        <StatTile
          label="On task"
          value={formatSeconds(stats.on_task_seconds)}
        />
        <StatTile
          label="Off task"
          value={formatSeconds(stats.off_task_seconds)}
        />
        <StatTile label="Focus" value={`${Math.round(stats.focus_percent)}%`} />
        <StatTile label="Switches away" value={String(stats.switch_count)} />
      </div>

      <Card size="sm">
        <CardHeader>
          <CardTitle>Timeline</CardTitle>
          <CardDescription>
            Planned {formatSeconds(session.target_seconds)} · actual{" "}
            {formatSeconds(actualSeconds)}
            {stats.in_progress ? " so far" : ""}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex flex-wrap gap-4">
            <LegendEntry
              color={ON_TASK_COLOR}
              label="On task"
              seconds={stats.on_task_seconds}
            />
            <LegendEntry
              color={OFF_TASK_COLOR}
              label="Off task"
              seconds={stats.off_task_seconds}
            />
            <LegendEntry
              color={null}
              label="Idle"
              seconds={stats.idle_seconds}
            />
          </div>
          <ProportionBar stats={stats} />
          <TimelineStrip stats={stats} />
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle>Where the time went</CardTitle>
          <CardDescription>
            Apps and sites during the session, colored by verdict.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {stats.apps.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">
              Nothing was tracked during this session.
            </p>
          ) : (
            <ul className="flex flex-col gap-2.5">
              {stats.apps.map((app) => (
                <li
                  key={`${app.kind}:${app.label}:${app.on_task}`}
                  className="flex items-center gap-3"
                >
                  <span
                    className="w-32 shrink-0 truncate text-sm sm:w-40"
                    title={`${app.label} · ${app.on_task ? "on task" : "off task"}`}
                  >
                    {app.label}
                  </span>
                  <span className="relative h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
                    <span
                      className="absolute inset-y-0 left-0 rounded-full"
                      style={{
                        width: `${maxAppSeconds > 0 ? (app.total_seconds / maxAppSeconds) * 100 : 0}%`,
                        backgroundColor: app.on_task
                          ? ON_TASK_COLOR
                          : OFF_TASK_COLOR,
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
    </div>
  );
}
