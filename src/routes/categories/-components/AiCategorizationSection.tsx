import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  aiFeedbackStats,
  aiModelLoaded,
  cancelCategorize,
  categorizeWithAi,
  unloadAiModel,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invalidateCategorizationQueries } from "@/queries";
import type { AiCategorizeProgress, FeedbackStats } from "@/types";

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function learningSummary(stats: FeedbackStats): string {
  const total =
    stats.corrected + stats.rejected + stats.confirmed + stats.pinned;
  if (total === 0) {
    return "No feedback yet. Accept, change or reject AI labels below and the model will learn from them.";
  }
  return `Learning from ${plural(stats.corrected, "correction")}, ${plural(
    stats.rejected,
    "rejection",
  )} and ${plural(stats.confirmed + stats.pinned, "confirmed label")}.`;
}

export function AiCategorizationSection() {
  const queryClient = useQueryClient();
  const [progress, setProgress] = useState<AiCategorizeProgress | null>(null);
  const statsQuery = useQuery({
    queryKey: ["aiFeedbackStats"],
    queryFn: aiFeedbackStats,
  });
  const loadedQuery = useQuery({
    queryKey: ["aiModelLoaded"],
    queryFn: aiModelLoaded,
    refetchInterval: 15_000,
  });
  const unload = useMutation({
    mutationFn: unloadAiModel,
    onSettled: () => void loadedQuery.refetch(),
  });
  const mutation = useMutation({
    mutationFn: (recheck: boolean) => categorizeWithAi(recheck),
    onSuccess: (outcome, recheck) => {
      invalidateCategorizationQueries(queryClient);
      toast.success(
        outcome.stopped
          ? "AI categorization stopped"
          : recheck
            ? "AI labels re-checked"
            : "AI categorization finished",
        {
          description: recheck
            ? `${outcome.changed} labels changed, ${outcome.skipped} cleared as uncertain.`
            : `${outcome.labeled} activities labeled, ${outcome.skipped} left uncategorized.`,
        },
      );
    },
    onError: () => invalidateCategorizationQueries(queryClient),
    onSettled: () => {
      setProgress(null);
      void loadedQuery.refetch();
    },
  });

  useEffect(() => {
    const unlisten = listen<AiCategorizeProgress>(
      "ai-categorize-progress",
      (event) => setProgress(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const running = mutation.isPending;
  const rechecking = running && mutation.variables === true;
  const label = (idle: string, active: boolean) =>
    active
      ? progress
        ? `Categorizing ${progress.done}/${progress.total}…`
        : "Loading model…"
      : idle;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>AI categorization</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <p className="m-0 text-muted-foreground">
          Label uncategorized activities with a local Llama 3.2 model. It runs
          inside Makora, needs an Ollama-pulled <code>llama3.2</code>, and
          nothing leaves your machine. Your own categories always take
          precedence, and your corrections are shown to the model as examples.
        </p>
        {statsQuery.data && (
          <p className="m-0 text-xs text-muted-foreground">
            {learningSummary(statsQuery.data)}
          </p>
        )}
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={running} onClick={() => mutation.mutate(false)}>
            {label("Categorize uncategorized", running && !rechecking)}
          </Button>
          {running && (
            <Button
              variant="destructive"
              onClick={() => void cancelCategorize()}
            >
              Stop
            </Button>
          )}
          <Button
            variant="outline"
            disabled={running || statsQuery.data?.pending_review === 0}
            onClick={() => mutation.mutate(true)}
            title="Re-run the AI labels you have not confirmed, using your latest feedback"
          >
            {label("Re-check AI labels", rechecking)}
          </Button>
        </div>
        {loadedQuery.data && !running && (
          <p className="m-0 text-xs text-muted-foreground">
            Model loaded in memory (released after 10 idle minutes).{" "}
            <button
              type="button"
              className="underline hover:text-foreground"
              disabled={unload.isPending}
              onClick={() => unload.mutate()}
            >
              Unload
            </button>
          </p>
        )}
        {mutation.isError && (
          <p className="m-0 text-xs text-destructive">
            {String(mutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
