import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckIcon, CircleIcon, PlusIcon } from "lucide-react";
import { useCallback, useEffect } from "react";
import { toast } from "sonner";
import { aiModel, categorization, settings } from "@/api";
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
import { useCategories } from "@/hooks/useCategories";
import { openDialog } from "@/hooks/useDialog";
import {
  aiFeedbackStatsQueryOptions,
  categorizeStatusQueryOptions,
  downloadStatusQueryOptions,
  installedModelsQueryOptions,
  invalidateCategorizationQueries,
  markCategorizeStarted,
  queryKeys,
  settingsQueryOptions,
} from "@/queries";
import type { ModelRole } from "@/types";

const ADD_MODEL_VALUE = "__add_model__";
const NO_MODELS: string[] = [];

function ModelSelect({
  id,
  label,
  hint,
  modelRole,
  value,
  installed,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  hint: string;
  modelRole: ModelRole;
  value: string;
  installed: string[];
  disabled: boolean;
  onChange: (model: string) => void;
}) {
  const names = installed.includes(value) ? installed : [value, ...installed];
  const nameLabel = (name: string) =>
    installed.includes(name) ? name : `${name} (not installed)`;
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-xs font-medium">
        {label}
      </label>
      <Select
        value={value}
        onValueChange={(next) => {
          if (!next) return;
          if (next === ADD_MODEL_VALUE) {
            openDialog<AddModelDialogData>("add-model", {
              role: modelRole,
              onDownloaded: (_, model) => onChange(model),
            });
            return;
          }
          onChange(next);
        }}
        items={{
          ...Object.fromEntries(names.map((name) => [name, nameLabel(name)])),
          [ADD_MODEL_VALUE]: "Add model",
        }}
        disabled={disabled}
      >
        <SelectTrigger id={id} size="sm" className="w-72">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {names.map((name) => (
            <SelectItem key={name} value={name}>
              {nameLabel(name)}
            </SelectItem>
          ))}
          <SelectSeparator className={"p-0"} />
          <SelectItem
            value={ADD_MODEL_VALUE}
            className={"flex gap-2 items-center hover:rounded-none"}
          >
            <PlusIcon /> Add model
          </SelectItem>
        </SelectContent>
      </Select>
      <p className="m-0 text-xs text-muted-foreground">{hint}</p>
    </div>
  );
}

function ChecklistItem({
  done,
  title,
  children,
}: {
  done: boolean;
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <li className="flex gap-2">
      {done ? (
        <CheckIcon className="mt-0.5 size-3.5 shrink-0 text-primary" />
      ) : (
        <CircleIcon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      )}
      <div className="flex min-w-0 flex-col gap-1">
        <span className="font-medium">{title}</span>
        {children}
      </div>
    </li>
  );
}

export function AiCategorizationSection() {
  const queryClient = useQueryClient();

  const statsQuery = useQuery(aiFeedbackStatsQueryOptions);
  const loadedQuery = useQuery({
    queryKey: queryKeys.aiModelLoaded,
    queryFn: aiModel.isLoaded,
    refetchInterval: 15_000,
  });
  const installedModelsQuery = useQuery(installedModelsQueryOptions);
  const settingsQuery = useQuery(settingsQueryOptions);
  const { categories } = useCategories();
  const categorizeStatus = useQuery(categorizeStatusQueryOptions).data;
  const downloadStatus = useQuery(downloadStatusQueryOptions).data;

  const installedModels = installedModelsQuery.data ?? NO_MODELS;
  const llmModel = settingsQuery.data?.ai.llm_model;
  const embeddingModel = settingsQuery.data?.ai.embedding_model;
  const modelsReady =
    llmModel !== undefined &&
    embeddingModel !== undefined &&
    installedModels.includes(llmModel) &&
    installedModels.includes(embeddingModel);
  const missingModels: { role: ModelRole; model: string }[] = [];
  if (installedModelsQuery.isSuccess && llmModel !== undefined) {
    if (!installedModels.includes(llmModel)) {
      missingModels.push({ role: "llm", model: llmModel });
    }
  }
  if (installedModelsQuery.isSuccess && embeddingModel !== undefined) {
    if (!installedModels.includes(embeddingModel)) {
      missingModels.push({ role: "embedding", model: embeddingModel });
    }
  }

  // A download keeps going in the backend across a page reload. Reopen the dialog when one is
  // found, so its progress bar and Stop button are reachable again.
  const backendDownloading = downloadStatus?.running ?? false;

  const setModelMutation = useMutation({
    mutationFn: ({ role, model }: { role: ModelRole; model: string }) =>
      role === "llm"
        ? settings.setLlmModel(model)
        : settings.setEmbeddingModel(model),
    onError: (err) => toast.error(String(err)),
    onSettled: () =>
      queryClient.invalidateQueries({ queryKey: queryKeys.settings }),
  });
  const applyModel = useCallback(
    (role: ModelRole, model: string) =>
      setModelMutation.mutate({ role, model }),
    [setModelMutation.mutate],
  );

  useEffect(() => {
    if (backendDownloading) {
      openDialog<AddModelDialogData>("add-model", { onDownloaded: applyModel });
    }
  }, [backendDownloading, applyModel]);

  const categorizeMutation = useMutation({
    mutationFn: (recheck: boolean) => categorization.run(recheck),
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
        : "Loading models…"
      : idle;

  const canRun = modelsReady && categories.length > 0;
  const blocker =
    modelsReady && categories.length === 0
      ? "Create a category to begin."
      : null;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Configurations</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <ul className="m-0 flex list-none flex-col gap-2 p-0 text-xs">
          <ChecklistItem
            done={categories.length > 0}
            title="Create your categories"
          >
            {categories.length === 0 && (
              <span className="text-muted-foreground">
                Add the categories you want your time sorted into in the
                Taxonomy tab.
              </span>
            )}
          </ChecklistItem>
          <ChecklistItem done={modelsReady} title="Install the models">
            {missingModels.map(({ role, model }) => (
              <span key={role} className="text-destructive">
                {model} is not downloaded yet.{" "}
                <button
                  type="button"
                  className="underline hover:text-destructive/80"
                  disabled={downloading}
                  onClick={() =>
                    openDialog<AddModelDialogData>("add-model", {
                      role,
                      model,
                      onDownloaded: applyModel,
                    })
                  }
                >
                  Download it
                </button>
              </span>
            ))}
          </ChecklistItem>
        </ul>
        <div className="flex flex-wrap gap-4">
          {llmModel !== undefined && (
            <ModelSelect
              id="ai-llm-model"
              label="Language model"
              hint="Labels each activity, shown the filed ones most like it."
              modelRole="llm"
              value={llmModel}
              installed={installedModels}
              disabled={running || downloading || setModelMutation.isPending}
              onChange={(model) => applyModel("llm", model)}
            />
          )}
          {embeddingModel !== undefined && (
            <ModelSelect
              id="ai-embedding-model"
              label="Embedding model"
              hint="Finds the filed activities most like a new one. Small and fast."
              modelRole="embedding"
              value={embeddingModel}
              installed={installedModels}
              disabled={running || downloading || setModelMutation.isPending}
              onChange={(model) => applyModel("embedding", model)}
            />
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            disabled={running || downloading || !canRun}
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
              !canRun ||
              statsQuery.data?.pending_review === 0
            }
            onClick={() => categorizeMutation.mutate(true)}
            title="Re-run the AI labels you have not confirmed, using your latest pins and rules"
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
        {blocker && (
          <p className="m-0 text-xs text-muted-foreground">{blocker}</p>
        )}
        {loadedQuery.data && !running && (
          <p className="m-0 text-xs text-muted-foreground">
            Models loaded in memory (released after 10 idle minutes).{" "}
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
