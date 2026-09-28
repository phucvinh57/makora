import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  aiFeedbackStats,
  aiModelInstalled,
  aiModelLoaded,
  cancelCategorize,
  cancelModelDownload,
  categorizeStatus,
  categorizeWithAi,
  downloadAiModel,
  listInstalledAiModels,
  modelDownloadStatus,
  resetCategorization,
  unloadAiModel,
} from "@/api";
import { PlusIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { invalidateCategorizationQueries } from "@/queries";
import type { AiCategorizeProgress } from "@/types";

const CATEGORIZE_STATUS_POLL_MS = 1_000;

const MODEL_STORAGE_KEY = "makora.aiModel";
const DEFAULT_MODEL = "qwen2.5:7b-instruct";
const SUGGESTED_MODELS = ["llama3.2", "llama3.2:1b", DEFAULT_MODEL];
const ADD_MODEL_VALUE = "__add_model__";

function storedModel(): string {
  try {
    return localStorage.getItem(MODEL_STORAGE_KEY) || "";
  } catch {
    return "";
  }
}

function rememberModel(model: string) {
  try {
    localStorage.setItem(MODEL_STORAGE_KEY, model);
  } catch {
    // Persisting the choice is a convenience only.
  }
}

interface DownloadProgress {
  downloaded: number;
  total: number;
}

function formatBytes(bytes: number): string {
  return bytes >= 1e9
    ? `${(bytes / 1e9).toFixed(2)} GB`
    : `${Math.round(bytes / 1e6)} MB`;
}

export function AiCategorizationSection() {
  const queryClient = useQueryClient();
  const [model, setModel] = useState(storedModel);
  const [addModelOpen, setAddModelOpen] = useState(false);
  const [newModelName, setNewModelName] = useState("");
  const [resetDialogOpen, setResetDialogOpen] = useState(false);
  const [progress, setProgress] = useState<AiCategorizeProgress | null>(null);
  const statsQuery = useQuery({
    queryKey: ["aiFeedbackStats"],
    queryFn: aiFeedbackStats,
  });
  const loadedQuery = useQuery({
    queryKey: ["aiModelLoaded"],
    queryFn: aiModelLoaded,
    refetchInterval: 15_000,
  });
  const installedModelsQuery = useQuery({
    queryKey: ["aiInstalledModels"],
    queryFn: listInstalledAiModels,
  });
  const installedModels = installedModelsQuery.data ?? [];

  // The remembered or just-picked model, falling back to whatever was last remembered (if it is
  // still installed) or else the first installed model, so the selection stays valid as the
  // installed list loads or changes.
  const effectiveModel = installedModels.includes(model)
    ? model
    : installedModels.includes(storedModel())
      ? storedModel()
      : (installedModels[0] ?? "");

  const trimmedNewModel = newModelName.trim();
  const newModelInstalledQuery = useQuery({
    queryKey: ["aiModelInstalled", trimmedNewModel],
    queryFn: () => aiModelInstalled(trimmedNewModel),
    enabled: trimmedNewModel !== "",
  });
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const downloadMutation = useMutation({
    mutationFn: downloadAiModel,
    onSuccess: async (finished, name) => {
      if (!finished) return;
      toast.success(`Downloaded ${name}`);
      await installedModelsQuery.refetch();
      rememberModel(name);
      setModel(name);
      setNewModelName("");
      setAddModelOpen(false);
    },
    onSettled: () => {
      setDownload(null);
      void newModelInstalledQuery.refetch();
    },
  });
  const unload = useMutation({
    mutationFn: unloadAiModel,
    onSettled: () => void loadedQuery.refetch(),
  });
  const resetMutation = useMutation({
    mutationFn: resetCategorization,
    onSuccess: (outcome) => {
      invalidateCategorizationQueries(queryClient);
      setResetDialogOpen(false);
      toast.success("Categorization data reset", {
        description: `${outcome.sessions_cleared} session labels were cleared.`,
      });
    },
  });

  function setResetDialogOpenState(next: boolean) {
    setResetDialogOpen(next);
    if (next) resetMutation.reset();
  }

  function setAddModelOpenState(next: boolean) {
    setAddModelOpen(next);
    if (next) setNewModelName("");
  }

  // A download or categorization keeps running in the backend across a page reload — these
  // let a freshly mounted component notice one is already going and resync instead of just
  // losing track of it. Polling only while the backend reports it running keeps this idle
  // otherwise.
  const downloadStatusQuery = useQuery({
    queryKey: ["aiModelDownloadStatus"],
    queryFn: modelDownloadStatus,
    refetchInterval: (query) =>
      query.state.data?.running ? CATEGORIZE_STATUS_POLL_MS : false,
  });
  const categorizeStatusQuery = useQuery({
    queryKey: ["aiCategorizeStatus"],
    queryFn: categorizeStatus,
    refetchInterval: (query) =>
      query.state.data?.running ? CATEGORIZE_STATUS_POLL_MS : false,
  });
  const backendDownloading = downloadStatusQuery.data?.running ?? false;
  const backendRunning = categorizeStatusQuery.data?.running ?? false;

  // Reopen the "add model" dialog so a resumed download's progress bar and Stop button are
  // reachable, even though `addModelOpen` itself did not survive the reload.
  useEffect(() => {
    if (backendDownloading) {
      setAddModelOpen(true);
      setNewModelName(
        (current) => current || downloadStatusQuery.data?.model || current,
      );
    }
  }, [backendDownloading, downloadStatusQuery.data?.model]);

  // Detect a resumed run finishing while this page had no mutation tracking it, and refresh
  // the data it would otherwise have refreshed on completion.
  const wasBackendDownloading = useRef(false);
  useEffect(() => {
    if (
      wasBackendDownloading.current &&
      !backendDownloading &&
      !downloadMutation.isPending
    ) {
      void installedModelsQuery.refetch();
      void newModelInstalledQuery.refetch();
    }
    wasBackendDownloading.current = backendDownloading;
  }, [
    backendDownloading,
    downloadMutation.isPending,
    installedModelsQuery.refetch,
    newModelInstalledQuery.refetch,
  ]);

  const mutation = useMutation({
    mutationFn: (recheck: boolean) =>
      categorizeWithAi(recheck, effectiveModel || undefined),
    onSuccess: (outcome, recheck) => {
      invalidateCategorizationQueries(queryClient);
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
    onError: () => invalidateCategorizationQueries(queryClient),
    onSettled: () => {
      setProgress(null);
      void loadedQuery.refetch();
    },
  });
  const wasBackendRunning = useRef(false);
  useEffect(() => {
    if (wasBackendRunning.current && !backendRunning && !mutation.isPending) {
      invalidateCategorizationQueries(queryClient);
    }
    wasBackendRunning.current = backendRunning;
  }, [backendRunning, mutation.isPending, queryClient]);

  useEffect(() => {
    const unlisten = listen<AiCategorizeProgress>(
      "ai-categorize-progress",
      (event) => setProgress(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<DownloadProgress>(
      "ai-model-download-progress",
      (event) => setDownload(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const downloading = downloadMutation.isPending || backendDownloading;
  const running = mutation.isPending || backendRunning;
  const rechecking =
    running &&
    (mutation.isPending
      ? mutation.variables === true
      : categorizeStatusQuery.data?.recheck === true);
  // Falls back to the backend's own status until an event arrives, so a page that reloaded
  // mid-run shows real numbers immediately instead of a blank "Loading model…".
  const effectiveProgress =
    progress ??
    (backendRunning && categorizeStatusQuery.data
      ? {
          done: categorizeStatusQuery.data.done,
          total: categorizeStatusQuery.data.total,
        }
      : null);
  const effectiveDownload =
    download ??
    (backendDownloading && downloadStatusQuery.data
      ? {
          downloaded: downloadStatusQuery.data.downloaded,
          total: downloadStatusQuery.data.total,
        }
      : null);
  const label = (idle: string, active: boolean) =>
    active
      ? effectiveProgress
        ? `Categorizing ${effectiveProgress.done}/${effectiveProgress.total}…`
        : "Loading model…"
      : idle;
  const canDownloadNewModel =
    trimmedNewModel !== "" &&
    newModelInstalledQuery.isSuccess &&
    !newModelInstalledQuery.data;

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
                setAddModelOpenState(true);
                return;
              }
              setModel(value);
              rememberModel(value);
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
            onClick={() => mutation.mutate(false)}
          >
            {label("Categorize", running && !rechecking)}
          </Button>
          {running && (
            <Button
              variant="destructive"
              onClick={() => void cancelCategorize()}
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
            onClick={() => mutation.mutate(true)}
            title="Re-run the AI labels you have not confirmed, using your latest feedback"
          >
            {label("Re-check AI labels", rechecking)}
          </Button>
          <Button
            variant="destructive"
            className="ml-auto"
            onClick={() => setResetDialogOpenState(true)}
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
              disabled={unload.isPending}
              onClick={() => unload.mutate()}
            >
              Unload
            </button>
          </p>
        )}
        {mutation.isError && (
          <p className="m-0 text-xs text-destructive">
            {String(mutation.error)}
          </p>
        )}
      </CardContent>

      <Dialog open={resetDialogOpen} onOpenChange={setResetDialogOpenState}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset categorization data?</DialogTitle>
            <DialogDescription>
              This action cannot be undone. It will keep your categories and
              tracking history, but delete all manual and AI categorizations.
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
              onClick={() => setResetDialogOpenState(false)}
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

      <Dialog open={addModelOpen} onOpenChange={setAddModelOpenState}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add a model</DialogTitle>
            <DialogDescription>
              Download a model from the{" "}
              <a
                href="https://ollama.com/library"
                target="_blank"
                rel="noreferrer"
                className="underline hover:text-foreground"
              >
                Ollama library
              </a>{" "}
              to use for AI categorization, e.g. <code>llama3.1:8b</code>.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            <Input
              list="ai-model-suggestions"
              value={newModelName}
              disabled={downloading}
              placeholder={DEFAULT_MODEL}
              autoFocus
              onChange={(event) => setNewModelName(event.target.value)}
            />
            <datalist id="ai-model-suggestions">
              {SUGGESTED_MODELS.map((name) => (
                <option key={name} value={name} />
              ))}
            </datalist>
            {trimmedNewModel !== "" && newModelInstalledQuery.data && (
              <p className="m-0 text-xs text-muted-foreground">
                Already installed.
              </p>
            )}
            {downloading &&
              effectiveDownload &&
              effectiveDownload.total > 0 && (
                <div className="flex flex-col gap-1">
                  <div
                    className="h-1.5 overflow-hidden rounded-full bg-muted"
                    role="progressbar"
                    aria-valuenow={Math.round(
                      (effectiveDownload.downloaded / effectiveDownload.total) *
                        100,
                    )}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-label="Model download progress"
                  >
                    <div
                      className="h-full rounded-full bg-primary transition-all"
                      style={{
                        width: `${(effectiveDownload.downloaded / effectiveDownload.total) * 100}%`,
                      }}
                    />
                  </div>
                  <p className="m-0 text-xs text-muted-foreground">
                    {formatBytes(effectiveDownload.downloaded)} of{" "}
                    {formatBytes(effectiveDownload.total)}
                  </p>
                </div>
              )}
            {downloadMutation.isError && (
              <p className="m-0 text-xs text-destructive">
                {String(downloadMutation.error)}
              </p>
            )}
          </div>
          <DialogFooter>
            {downloading ? (
              <Button
                type="button"
                variant="destructive"
                onClick={() => void cancelModelDownload()}
              >
                Stop
              </Button>
            ) : (
              <Button
                type="button"
                variant="ghost"
                onClick={() => setAddModelOpenState(false)}
              >
                Cancel
              </Button>
            )}
            <Button
              type="button"
              disabled={!canDownloadNewModel || downloading}
              onClick={() => downloadMutation.mutate(trimmedNewModel)}
            >
              {downloading ? "Downloading…" : "Download"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
