import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { formatSeconds } from "@/duration";
import type { ActivityStats } from "@/types";
import { UNCATEGORIZED_COLOR } from "@/categories";

export function TopAppsBreakdown({ stats }: { stats: ActivityStats }) {
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
