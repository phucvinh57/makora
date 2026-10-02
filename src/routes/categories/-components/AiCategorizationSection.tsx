import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useEffect } from "react";
import { toast } from "sonner";
import { aiModel, categorization } from "@/api";
import type { AddModelDialogData } from "@/components/dialogs/AddModelDialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { openDialog } from "@/hooks/useDialog";
import { STRING_CODEC, usePersistentState } from "@/hooks/usePersistentState";
import {
  aiFeedbackStatsQueryOptions,
  categorizeStatusQueryOptions,
  downloadStatusQueryOptions,
  installedModelsQueryOptions,
  invalidateCategorizationQueries,
  markCategorizeStarted,
  queryKeys,
} from "@/queries";

const MODEL_STORAGE_KEY = "makora.aiModel";
const ADD_MODEL_VALUE = "__add_model__";
const NO_MODELS: string[] = [];

export function AiCategorizationSection() {
  const queryClient = useQueryClient();
  const [model, setModel] = usePersistentState(
    MODEL_STORAGE_KEY,
    "",
    STRING_CODEC,
  );

  const statsQuery = useQuery(aiFeedbackStatsQueryOptions);
  const loadedQuery = useQuery({
    queryKey: queryKeys.aiModelLoaded,
    queryFn: aiModel.isLoaded,
    refetchInterval: 15_000,
  });
  const installedModelsQuery = useQuery(installedModelsQueryOptions);
  const categorizeStatus = useQuery(categorizeStatusQueryOptions).data;
  const downloadStatus = useQuery(downloadStatusQueryOptions).data;

  const installedModels = installedModelsQuery.data ?? NO_MODELS;
  // The remembered or just-picked model while it is still installed, else the first one, so
  // the selection stays valid as the installed list loads or changes.
  const effectiveModel = installedModels.includes(model)
    ? model
    : (installedModels[0] ?? "");

  // A download keeps going in the backend across a page reload. Reopen the dialog when one is
  // found, so its progress bar and Stop button are reachable again.
  const backendDownloading = downloadStatus?.running ?? false;
  useEffect(() => {
    if (backendDownloading) {
      openDialog<AddModelDialogData>("add-model", { onDownloaded: setModel });
    }
  }, [backendDownloading, setModel]);

  const categorizeMutation = useMutation({
    mutationFn: (recheck: boolean) =>
      categorization.run(recheck, effectiveModel || undefined),
    onMutate: (recheck) => markCategorizeStarted(queryClient, recheck),
    onSuccess: (outcome, recheck) => {
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
    onSettled: () =>
      Promise.all([
        invalidateCategorizationQueries(queryClient),
        queryClient.invalidateQueries({
          queryKey: queryKeys.aiCategorizeStatus,
        }),
        queryClient.invalidateQueries({ queryKey: queryKeys.aiModelLoaded }),
      ]),
  });
  const stopMutation = useMutation({
    mutationFn: categorization.cancel,
    onError: (err) => toast.error(String(err)),
  });
  const unloadMutation = useMutation({
    mutationFn: aiModel.unload,
    onSettled: () =>
      queryClient.invalidateQueries({ queryKey: queryKeys.aiModelLoaded }),
  });

  const downloading = backendDownloading;
  const running = categorizeMutation.isPending || !!categorizeStatus?.running;
  const rechecking =
    running &&
    (categorizeMutation.isPending
      ? categorizeMutation.variables === true
      : categorizeStatus?.recheck === true);
  const progress =
    categorizeStatus?.running && categorizeStatus.total > 0
      ? categorizeStatus
      : null;
  const label = (idle: string, active: boolean) =>
    active
      ? progress
        ? `Categorizing ${progress.done}/${progress.total}…`
        : "Loading model…"
      : idle;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Configurations</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <div className="flex flex-col gap-1">
          <label htmlFor="ai-model" className="text-xs font-medium">
            Model
          </label>
          <Select
            value={effectiveModel}
            onValueChange={(value) => {
              if (!value) return;
              if (value === ADD_MODEL_VALUE) {
                openDialog<AddModelDialogData>("add-model", {
                  onDownloaded: setModel,
                });
                return;
              }
              setModel(value);
            }}
            items={{
              ...Object.fromEntries(
                installedModels.map((name) => [name, name]),
              ),
              [ADD_MODEL_VALUE]: "Add model",
            }}
            disabled={running || downloading}
          >
            <SelectTrigger id="ai-model" size="sm" className="w-56">
              <SelectValue placeholder="No models installed" />
            </SelectTrigger>
            <SelectContent>
              {installedModels.map((name) => (
                <SelectItem
                  key={name}
                  value={name}
                  className={"flex gap-2 items-center hover:rounded-none"}
                >
                  {name}
                </SelectItem>
              ))}
              {installedModels.length > 0 && (
                <SelectSeparator className={"p-0"} />
              )}
              <SelectItem
                value={ADD_MODEL_VALUE}
                className={"flex gap-2 items-center hover:rounded-none"}
              >
                <PlusIcon /> Add model
              </SelectItem>
            </SelectContent>
          </Select>
          {installedModelsQuery.isSuccess && installedModels.length === 0 && (
            <p className="m-0 text-xs text-muted-foreground">
              No models downloaded yet. Pick “Add model” above to get started.
            </p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            disabled={running || downloading || effectiveModel === ""}
            onClick={() => categorizeMutation.mutate(false)}
          >
            {label("Categorize", running && !rechecking)}
          </Button>
          {running && (
            <Button
              variant="destructive"
              disabled={stopMutation.isPending}
              onClick={() => stopMutation.mutate()}
            >
              Stop
            </Button>
          )}
          <Button
            variant="outline"
            disabled={
              running ||
              downloading ||
              effectiveModel === "" ||
              statsQuery.data?.pending_review === 0
            }
            onClick={() => categorizeMutation.mutate(true)}
            title="Re-run the AI labels you have not confirmed, using your latest feedback"
          >
            {label("Re-check AI labels", rechecking)}
          </Button>
          <Button
            variant="destructive"
            className="ml-auto"
            onClick={() => openDialog("reset-categorization")}
          >
            Reset data
          </Button>
        </div>
        {loadedQuery.data && !running && (
          <p className="m-0 text-xs text-muted-foreground">
            Model loaded in memory (released after 10 idle minutes).{" "}
            <button
              type="button"
              className="underline hover:text-foreground"
              disabled={unloadMutation.isPending}
              onClick={() => unloadMutation.mutate()}
            >
              Unload
            </button>
          </p>
        )}
        {categorizeMutation.isError && (
          <p className="m-0 text-xs text-destructive">
            {String(categorizeMutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
