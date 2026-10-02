import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { categorization } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useDialog } from "@/hooks/useDialog";
import { invalidateCategorizationQueries } from "@/queries";

export function ResetCategorizationDialog() {
  const queryClient = useQueryClient();
  const { open, setOpen } = useDialog("reset-categorization");

  const resetMutation = useMutation({
    mutationFn: categorization.reset,
    onSuccess: (outcome) => {
      setOpen(false);
      toast.success("Categorization data reset", {
        description: `${outcome.labels_cleared} labels were cleared.`,
      });
      return invalidateCategorizationQueries(queryClient);
    },
  });

  function handleOpenChange(next: boolean) {
    // A failed attempt's message should not greet the next time the dialog opens.
    if (!next) resetMutation.reset();
    setOpen(next);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Reset categorization data?</DialogTitle>
          <DialogDescription>
            This action cannot be undone. It will keep your categories, rules
            and tracking history, but delete all manual and AI categorizations.
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
            onClick={() => handleOpenChange(false)}
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
  );
}
