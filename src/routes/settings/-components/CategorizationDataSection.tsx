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
import { invalidateCategorizationQueries } from "@/queries";

export function CategorizationDataSection() {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const resetMutation = useMutation({
    mutationFn: resetCategorization,
    onSuccess: (outcome) => {
      invalidateCategorizationQueries(queryClient);
      setOpen(false);
      toast.success("Categorization data reset", {
        description: `${outcome.sessions_cleared} session labels were cleared.`,
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
          Clear all manual categorizations so you can start over. Categories and
          tracking history are kept.
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
              tracking history, but delete all manual categorizations.
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
