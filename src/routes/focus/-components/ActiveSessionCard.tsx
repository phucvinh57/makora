import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { differenceInSeconds } from "date-fns";
import { Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { stopWorkSession, workSessionStats } from "@/api";
import { formatSeconds } from "@/duration";
import type { WorkSession } from "@/types";
import { WORK_SESSIONS_KEY, workSessionStatsKey } from "./constants";

export function ActiveSessionCard({ session }: { session: WorkSession }) {
  const queryClient = useQueryClient();
  const [now, setNow] = useState(() => new Date());
  const endedRef = useRef(false);

  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);

  const end = new Date(session.end_time);
  const remaining = Math.max(0, differenceInSeconds(end, now));
  const elapsed = Math.max(
    0,
    differenceInSeconds(now, new Date(session.start_time)),
  );
  const total = Math.max(1, differenceInSeconds(end, new Date(session.start_time)));

  // The auto-end is just this instant arriving — announce it and refetch so
  // the session moves from "active" to the history list.
  useEffect(() => {
    if (remaining > 0 || endedRef.current) return;
    endedRef.current = true;
    toast.success(`Focus session "${session.name}" is complete`);
    queryClient.invalidateQueries({ queryKey: WORK_SESSIONS_KEY });
    queryClient.invalidateQueries({
      queryKey: workSessionStatsKey(session.id),
    });
  }, [remaining, session, queryClient]);

  const statsQuery = useQuery({
    queryKey: workSessionStatsKey(session.id),
    queryFn: () => workSessionStats(session.id),
    refetchInterval: 15_000,
  });
  const stats = statsQuery.data;

  const stopMutation = useMutation({
    mutationFn: () => stopWorkSession(session.id),
    onSuccess: () => {
      toast.success("Session stopped");
      queryClient.invalidateQueries({ queryKey: WORK_SESSIONS_KEY });
      queryClient.invalidateQueries({
        queryKey: workSessionStatsKey(session.id),
      });
    },
    onError: (error) => toast.error(String(error)),
  });

  return (
    <Card size="sm" className="border-primary/40">
      <CardHeader>
        <div className="flex items-start justify-between gap-3">
          <div>
            <CardTitle>{session.name}</CardTitle>
            <CardDescription>
              {session.note ?? "Focus session in progress"}
            </CardDescription>
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={stopMutation.isPending}
            onClick={() => stopMutation.mutate()}
          >
            <Square className="size-3.5" /> Stop
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-baseline justify-between gap-3">
          <span className="text-3xl font-semibold tabular-nums">
            {formatSeconds(remaining)}
          </span>
          <span className="text-sm text-muted-foreground">
            {stats
              ? `${formatSeconds(stats.on_task_seconds)} on task · ${Math.round(stats.focus_percent)}% focus`
              : "remaining"}
          </span>
        </div>
        <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-1000"
            style={{ width: `${Math.min(100, (elapsed / total) * 100)}%` }}
          />
        </div>
      </CardContent>
    </Card>
  );
}
