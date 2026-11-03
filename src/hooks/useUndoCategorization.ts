import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { session } from "@/api";
import { invalidateCategorizationQueries } from "@/queries";
import type { ActivityClassification } from "@/types";

export function useUndoCategorization() {
  const queryClient = useQueryClient();
  const { mutate } = useMutation({
    mutationFn: session.undoCategory,
    onSuccess: () => {
      toast.success("Undone");
      return invalidateCategorizationQueries(queryClient);
    },
    onError: (err) => toast.error(String(err)),
  });

  return (prior: ActivityClassification[]) => ({
    label: "Undo",
    onClick: () => mutate(prior),
  });
}
