import { useMutation, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { categorizeWithAi } from "@/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invalidateCategorizationQueries } from "@/queries";
import type { AiCategorizeProgress } from "@/types";

export function AiCategorizationSection() {
  const queryClient = useQueryClient();
  const [progress, setProgress] = useState<AiCategorizeProgress | null>(null);
  const mutation = useMutation({
    mutationFn: () => categorizeWithAi(),
    onSuccess: (outcome) => {
      invalidateCategorizationQueries(queryClient);
      toast.success("AI categorization finished", {
        description: `${outcome.labeled} activities labeled, ${outcome.skipped} left uncategorized.`,
      });
    },
    onSettled: () => setProgress(null),
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
          precedence.
        </p>
        <div className="flex items-center gap-3">
          <Button
            disabled={mutation.isPending}
            onClick={() => mutation.mutate()}
          >
            {mutation.isPending
              ? progress
                ? `Categorizing ${progress.done}/${progress.total}…`
                : "Loading model…"
              : "Categorize with AI"}
          </Button>
        </div>
        {mutation.isError && (
          <p className="m-0 text-xs text-destructive">
            {String(mutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
