import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { aiModel, categorization, settings } from "@/api";
import type { AddModelDialogData } from "@/components/dialogs/AddModelDialog";
import { type Suggestion, SuggestInput } from "@/components/SuggestInput";
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
  providerModelsQueryOptions,
  queryKeys,
  settingsQueryOptions,
} from "@/queries";
import type { InstalledModel, ModelRole, ProviderConfig } from "@/types";
import { AutoCategorizeSetting } from "./AutoCategorizeSetting";

const ADD_MODEL_VALUE = "__add_model__";
const LOCAL_PROVIDER = "__local__";
const NO_MODELS: InstalledModel[] = [];
const NO_PROVIDERS: ProviderConfig[] = [];

function ProviderSelect({
  value,
  providers,
  disabled,
  onChange,
}: {
  value: string | null;
  providers: ProviderConfig[];
  disabled: boolean;
  onChange: (provider: string | null) => void;
}) {
  const items = {
    [LOCAL_PROVIDER]: "Local (on this machine)",
    ...Object.fromEntries(providers.map((p) => [p.id, p.name])),
  };
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor="ai-llm-provider" className="text-xs font-medium">
        Language model runs on
      </label>
      <Select
        value={value ?? LOCAL_PROVIDER}
        onValueChange={(next) => {
          if (!next) return;
          onChange(next === LOCAL_PROVIDER ? null : next);
        }}
        items={items}
        disabled={disabled}
      >
        <SelectTrigger id="ai-llm-provider" size="sm" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {Object.entries(items).map(([id, label]) => (
            <SelectItem key={id} value={id}>
              {label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

/** A remote model id: suggested from what the provider lists, typed freely when it lists none. */
function RemoteModelInput({
  provider,
  value,
  disabled,
  onApply,
}: {
  provider: ProviderConfig;
  value: string;
  disabled: boolean;
  onApply: (model: string) => void;
}) {
  const [typed, setTyped] = useState(value);
  const modelsQuery = useQuery(providerModelsQueryOptions(provider.id));
  const suggestions: Suggestion[] = (modelsQuery.data ?? []).map((model) => ({
    value: model,
  }));
  const trimmed = typed.trim();
  const canApply = !disabled && trimmed !== "" && trimmed !== value;
  return (
    <form
      className="flex flex-col gap-1"
      onSubmit={(event) => {
        event.preventDefault();
        if (canApply) onApply(trimmed);
      }}
    >
      <span className="text-xs font-medium">Language model</span>
      <div className="flex items-center gap-2">
        <div className="w-72">
          <SuggestInput
            value={typed}
            onValueChange={setTyped}
            suggestions={suggestions}
            placeholder={modelsQuery.isPending ? "Loading models…" : "Model id"}
            ariaLabel="Language model"
          />
        </div>
        <Button type="submit" size="sm" variant="outline" disabled={!canApply}>
          Use
        </Button>
      </div>
      {modelsQuery.isError && (
        <p className="m-0 text-xs text-destructive">
          Could not list {provider.name}'s models: {String(modelsQuery.error)}
        </p>
      )}
    </form>
  );
}

function ModelSelect({
  id,
  label,
  modelRole,
  value,
  installed,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
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
    </div>
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
  const installedLlms = installedModels
    .filter((m) => m.role === "llm")
    .map((m) => m.name);
  const installedEmbeddings = installedModels
    .filter((m) => m.role === "embedding")
    .map((m) => m.name);
  const providers = settingsQuery.data?.ai.providers ?? NO_PROVIDERS;
  const savedProviderId = settingsQuery.data?.ai.llm_provider ?? null;
  const llmModel = settingsQuery.data?.ai.llm_model;
  const embeddingModel = settingsQuery.data?.ai.embedding_model;
  // A remote provider picked here only takes effect once a model is chosen for it.
  const [pendingProviderId, setPendingProviderId] = useState<string | null>(
    null,
  );
  const shownProvider = providers.find(
    (p) => p.id === (pendingProviderId ?? savedProviderId),
  );
  const savedProvider = providers.find((p) => p.id === savedProviderId);
  const llmReady =
    llmModel !== undefined &&
    (savedProviderId !== null
      ? savedProvider !== undefined
      : installedLlms.includes(llmModel));
  const embeddingReady =
    embeddingModel !== undefined &&
    installedEmbeddings.includes(embeddingModel);
  const modelsReady = llmReady && embeddingReady;
  const modelsBlocker =
    settingsQuery.isPending || installedModelsQuery.isPending
      ? null
      : savedProviderId !== null && savedProvider === undefined
        ? "The selected provider no longer exists. Choose another."
        : !llmReady
          ? `Language model ${llmModel} is not installed. Add it from the menu above.`
          : !embeddingReady
            ? `Embedding model ${embeddingModel} is not installed. Add it from the menu above.`
            : null;

  // A download keeps going in the backend across a page reload. Reopen the dialog when one is
  // found, so its progress bar and Stop button are reachable again.
  const backendDownloading = downloadStatus?.running ?? false;

  const setModelMutation = useMutation({
    mutationFn: ({
      role,
      model,
      provider = null,
    }: {
      role: ModelRole;
      model: string;
      provider?: string | null;
    }) =>
      role === "llm"
        ? settings.setLlm(provider, model)
        : settings.setEmbeddingModel(model),
    onSuccess: (_, { role }) => {
      if (role === "llm") setPendingProviderId(null);
    },
    onError: (err) => toast.error(String(err)),
    onSettled: () =>
      queryClient.invalidateQueries({ queryKey: queryKeys.settings }),
  });
  const applyModel = useCallback(
    (role: ModelRole, model: string) =>
      setModelMutation.mutate({ role, model }),
    [setModelMutation.mutate],
  );

  const wasDownloading = useRef(backendDownloading);
  useEffect(() => {
    if (backendDownloading && !wasDownloading.current) {
      openDialog<AddModelDialogData>("add-model", { onDownloaded: applyModel });
    }
    wasDownloading.current = backendDownloading;
  }, [backendDownloading, applyModel]);

  function switchToLocal() {
    setPendingProviderId(null);
    if (savedProviderId === null) return;
    const model =
      llmModel !== undefined && installedLlms.includes(llmModel)
        ? llmModel
        : installedLlms[0];
    if (model !== undefined) {
      applyModel("llm", model);
      return;
    }
    openDialog<AddModelDialogData>("add-model", {
      role: "llm",
      onDownloaded: applyModel,
    });
  }

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
        : savedProvider
          ? "Starting…"
          : "Loading models…"
      : idle;

  const choosingProvider =
    shownProvider !== undefined && shownProvider.id !== savedProviderId;
  const canRun = modelsReady && categories.length > 0 && !choosingProvider;
  const blocker =
    choosingProvider && shownProvider
      ? `Choose a model to switch to ${shownProvider.name}.`
      : !modelsReady
        ? modelsBlocker
        : categories.length === 0
          ? "Create a category to begin."
          : null;
  const modelsDisabled = running || downloading || setModelMutation.isPending;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Configurations</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <div className="flex flex-wrap gap-4">
          {embeddingModel !== undefined && (
            <ModelSelect
              id="ai-embedding-model"
              label="Embedding model"
              modelRole="embedding"
              value={embeddingModel}
              installed={installedEmbeddings}
              disabled={modelsDisabled}
              onChange={(model) => applyModel("embedding", model)}
            />
          )}
        </div>
        <div className="flex flex-wrap items-end gap-4">
          <ProviderSelect
            value={shownProvider?.id ?? null}
            providers={providers}
            disabled={modelsDisabled}
            onChange={(provider) => {
              if (provider === null) {
                switchToLocal();
                return;
              }
              setPendingProviderId(
                provider === savedProviderId ? null : provider,
              );
            }}
          />
          {shownProvider
            ? llmModel !== undefined && (
                <RemoteModelInput
                  key={shownProvider.id}
                  provider={shownProvider}
                  value={shownProvider === savedProvider ? llmModel : ""}
                  disabled={modelsDisabled}
                  onApply={(model) =>
                    setModelMutation.mutate({
                      role: "llm",
                      model,
                      provider: shownProvider.id,
                    })
                  }
                />
              )
            : llmModel !== undefined && (
                <ModelSelect
                  id="ai-llm-model"
                  label="Language model"
                  modelRole="llm"
                  value={llmModel}
                  installed={installedLlms}
                  disabled={modelsDisabled}
                  onChange={(model) => applyModel("llm", model)}
                />
              )}
        </div>
        {shownProvider && (
          <p className="m-0 text-xs text-muted-foreground">
            Titles, URLs and category names are sent to {shownProvider.name}.
          </p>
        )}
        <AutoCategorizeSetting />
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
