import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { format } from "date-fns";
import { Pencil, Play, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { deleteWorkSession, listCategories, listWorkSessions } from "@/api";
import { formatSeconds } from "@/duration";
import { openDialog } from "@/hooks/useDialog";
import { cn } from "@/lib/utils";
import type { WorkSession } from "@/types";
import { ActiveSessionCard } from "./-components/ActiveSessionCard";
import { WORK_SESSIONS_KEY } from "./-components/constants";
import { EditSessionDialog } from "./-components/EditSessionDialog";
import { SessionSummary } from "./-components/SessionSummary";
import { StartSessionDialog } from "./-components/StartSessionDialog";

export const Route = createFileRoute("/focus")({
  component: FocusPage,
});

function isActive(session: WorkSession, now: Date) {
  return new Date(session.start_time) <= now && now < new Date(session.end_time);
}

function FocusPage() {
  const queryClient = useQueryClient();
  const [selectedId, setSelectedId] = useState<number | null>(null);
  // A coarse clock so a session flips between active and finished without a
  // reload; the active card runs its own 1s countdown.
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 10_000);
    return () => clearInterval(timer);
  }, []);

  const sessionsQuery = useQuery({
    queryKey: WORK_SESSIONS_KEY,
    queryFn: listWorkSessions,
  });
  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const categoryColor = new Map(
    (categoriesQuery.data ?? []).map((c) => [c.id, c.color]),
  );

  const sessions = sessionsQuery.data ?? [];
  const active = sessions.find((s) => isActive(s, now)) ?? null;
  const selected =
    sessions.find((s) => s.id === selectedId) ?? active ?? sessions[0] ?? null;

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteWorkSession(id),
    onSuccess: (_, id) => {
      toast.success("Session deleted");
      if (selectedId === id) setSelectedId(null);
      queryClient.invalidateQueries({ queryKey: WORK_SESSIONS_KEY });
    },
    onError: (error) => toast.error(String(error)),
  });

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="mr-auto">
          <h1 className="m-0 text-lg font-semibold">Focus</h1>
          <p className="m-0 text-sm text-muted-foreground">
            Plan a work block, then see how focused it really was
          </p>
        </div>
        <Button
          size="sm"
          disabled={active !== null}
          onClick={() => openDialog("start-work-session")}
        >
          <Play className="size-3.5" />
          {active ? "Session in progress" : "Start session"}
        </Button>
      </header>

      {sessionsQuery.error && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load sessions: {String(sessionsQuery.error)}
        </div>
      )}

      {active && <ActiveSessionCard key={active.id} session={active} />}

      {sessionsQuery.isLoading ? (
        <Skeleton className="h-48 w-full" />
      ) : sessions.length === 0 ? (
        <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
          No focus sessions yet. Start one to see how your time holds up
          against your plan.
        </div>
      ) : (
        <div className="grid items-start gap-6 @4xl/main:grid-cols-[2fr_3fr]">
          <Card size="sm">
            <CardContent className="p-2">
              <ul className="flex flex-col gap-1">
                {sessions.map((session) => (
                  <li key={session.id}>
                    <div
                      className={cn(
                        "group flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors",
                        selected?.id === session.id
                          ? "bg-accent"
                          : "hover:bg-accent/50",
                      )}
                    >
                      <button
                        type="button"
                        className="flex min-w-0 flex-1 flex-col gap-0.5 text-left"
                        onClick={() => setSelectedId(session.id)}
                      >
                        <span className="flex items-center gap-1.5">
                          <span className="truncate text-sm font-medium">
                            {session.name}
                          </span>
                          {session.category_ids.map((id) => (
                            <span
                              key={id}
                              className="size-2 shrink-0 rounded-full"
                              style={{
                                backgroundColor:
                                  categoryColor.get(id) ?? "var(--muted)",
                              }}
                            />
                          ))}
                          {isActive(session, now) && (
                            <span className="rounded-full bg-primary/10 px-1.5 text-[10px] font-medium text-primary">
                              live
                            </span>
                          )}
                        </span>
                        <span className="text-xs tabular-nums text-muted-foreground">
                          {format(new Date(session.start_time), "MMM d, HH:mm")}{" "}
                          · planned {formatSeconds(session.target_seconds)}
                        </span>
                      </button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label="Edit session"
                        className="opacity-0 transition-opacity group-hover:opacity-100"
                        onClick={() =>
                          openDialog<WorkSession>("edit-work-session", session)
                        }
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label="Delete session"
                        className="opacity-0 transition-opacity group-hover:opacity-100"
                        disabled={deleteMutation.isPending}
                        onClick={() => deleteMutation.mutate(session.id)}
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    </div>
                  </li>
                ))}
              </ul>
            </CardContent>
          </Card>

          {selected && <SessionSummary session={selected} />}
        </div>
      )}

      <StartSessionDialog />
      <EditSessionDialog />
    </div>
  );
}
