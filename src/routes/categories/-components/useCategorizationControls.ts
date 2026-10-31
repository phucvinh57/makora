import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { aiModel, categorization, settings } from "@/api";
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
import type { InstalledModel, ModelRole, ProviderConfig } from "@/types";

const NO_MODELS: InstalledModel[] = [];
const NO_PROVIDERS: ProviderConfig[] = [];

/** The AI section's state and actions: models, provider choice and runs. */
export function useCategorizationControls() {
  const queryClient = useQueryClient();

  const statsQuery = useQuery(aiFeedbackStatsQueryOptions);
  const loadedQuery = useQuery({
    queryKey: queryKeys.aiModelLoaded,
    queryFn: aiModel.isLoaded,
    refetchInterval: 15_000,
  });
  const installedModelsQuery = useQuery(installedModelsQueryOptions);
  const settingsQuery = useQuery(settingsQueryOptions);
  const { categories, isLoading: categoriesLoading } = useCategories();
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

  const downloading = downloadStatus?.running ?? false;

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

  // A download keeps going in the backend across a page reload. Reopen the dialog when one
  // starts, so its progress bar and Stop button are reachable again.
  const wasDownloading = useRef(downloading);
  useEffect(() => {
    if (downloading && !wasDownloading.current) {
      openDialog("add-model", { onDownloaded: applyModel });
    }
    wasDownloading.current = downloading;
  }, [downloading, applyModel]);

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
    openDialog("add-model", { role: "llm", onDownloaded: applyModel });
  }

  /** `null` is the local runtime. */
  function chooseProvider(provider: string | null) {
    if (provider === null) {
      switchToLocal();
      return;
    }
    setPendingProviderId(provider === savedProviderId ? null : provider);
  }

  function applyRemoteModel(provider: string, model: string) {
    setModelMutation.mutate({ role: "llm", model, provider });
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
  const runLabel = (idle: string, active: boolean) =>
    active
      ? progress
        ? `Categorizing ${progress.done}/${progress.total}…`
        : savedProvider
          ? "Starting…"
          : "Loading models…"
      : idle;

  const choosingProvider =
    shownProvider !== undefined && shownProvider.id !== savedProviderId;
  const canRun =
    !running &&
    !downloading &&
    modelsReady &&
    !categoriesLoading &&
    categories.length > 0 &&
    !choosingProvider;
  const blocker =
    choosingProvider && shownProvider
      ? `Choose a model to switch to ${shownProvider.name}.`
      : !modelsReady
        ? modelsBlocker
        : !categoriesLoading && categories.length === 0
          ? "Create a category to begin."
          : null;

  return {
    embeddingModel,
    llmModel,
    installedEmbeddings,
    installedLlms,
    providers,
    shownProvider,
    savedProvider,
    modelsDisabled: running || downloading || setModelMutation.isPending,
    applyModel,
    applyRemoteModel,
    chooseProvider,
    running,
    rechecking,
    runLabel,
    canRun,
    canRecheck: canRun && statsQuery.data?.pending_review !== 0,
    blocker,
    runError: categorizeMutation.error,
    run: (recheck: boolean) => categorizeMutation.mutate(recheck),
    stop: () => stopMutation.mutate(),
    stopPending: stopMutation.isPending,
    modelsLoaded: loadedQuery.data === true,
    unload: () => unloadMutation.mutate(),
    unloadPending: unloadMutation.isPending,
  };
}
