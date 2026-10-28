import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { aiProvider } from "@/api";
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
import { queryKeys, settingsQueryOptions } from "@/queries";
import type { ProviderConfig } from "@/types";

export interface RemoveProviderDialogData {
  provider: ProviderConfig;
}

export function RemoveProviderDialog() {
  const queryClient = useQueryClient();
  const { open, setOpen, data } =
    useDialog<RemoveProviderDialogData>("remove-provider");
  const settingsQuery = useQuery(settingsQueryOptions);
  const provider = data?.provider;
  const inUse =
    provider !== undefined &&
    settingsQuery.data?.ai.llm_provider === provider.id;

  const removeMutation = useMutation({
    mutationFn: (target: ProviderConfig) => aiProvider.remove(target.id),
    onSuccess: (saved, target) => {
      queryClient.setQueryData(queryKeys.settings, saved);
      queryClient.removeQueries({
        queryKey: [...queryKeys.aiProviderModels, target.id],
      });
      queryClient.removeQueries({
        queryKey: [...queryKeys.aiProviderHasKey, target.id],
      });
    },
    onError: (err) => toast.error(String(err)),
  });

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Remove {provider?.name}?</DialogTitle>
          <DialogDescription>
            Its API key is deleted from your system keychain.
            {inUse &&
              " AI categorization uses this provider now, so it goes back to the local language model."}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => {
              if (!provider) return;
              removeMutation.mutate(provider);
              setOpen(false);
            }}
          >
            Remove
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
