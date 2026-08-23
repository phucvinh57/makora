import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import { resetCategorization } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invalidateLabelingQueries } from "@/queries";

export function CategorizationDataSection() {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const resetMutation = useMutation({
    mutationFn: resetCategorization,
    onSuccess: (outcome) => {
      invalidateLabelingQueries(queryClient);
      setOpen(false);
      toast.success("Categorization data reset", {
        description: `${outcome.rules_deleted} rules and ${outcome.sessions_cleared} session labels were cleared. Go to Labeling → AI and click Run now for a fresh run.`,
      });
    },
  });

  function setDialogOpen(next: boolean) {
    setOpen(next);
    if (next) resetMutation.reset();
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Categorization data</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <p className="m-0 text-muted-foreground">
          Clear previous categorization evidence and rules so you can review a
          fresh AI run yourself. Categories and tracking history are kept.
        </p>
        <div>
          <Button variant="destructive" onClick={() => setDialogOpen(true)}>
            Reset categorization data
          </Button>
        </div>
      </CardContent>

      <Dialog open={open} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset categorization data?</DialogTitle>
            <DialogDescription>
              This action cannot be undone. It will keep your categories and
              tracking history, but delete all rules, manual categorizations, AI
              results, suggestions, derived AI detections, and accuracy history.
              AI scheduling will be changed to manual, and nothing will be
              seeded or run automatically.
            </DialogDescription>
            {resetMutation.isError && (
              <p className="m-0 text-xs text-destructive">
                {String(resetMutation.error)}
              </p>
            )}
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              disabled={resetMutation.isPending}
              onClick={() => setDialogOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={resetMutation.isPending}
              onClick={() => resetMutation.mutate()}
            >
              {resetMutation.isPending ? "Resetting…" : "Reset data"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
