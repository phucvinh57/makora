import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { PlusIcon } from "lucide-react";
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
  unloadAiModel,
} from "@/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatSeconds } from "@/duration";
import { invalidateCategorizationQueries } from "@/queries";
import type { AiCategorizeProgress, FeedbackStats } from "@/types";

const CATEGORIZE_STATUS_POLL_MS = 1_000;

const MODEL_STORAGE_KEY = "makora.aiModel";
const DEFAULT_MODEL = "qwen2.5:7b-instruct";
const SUGGESTED_MODELS = ["llama3.2", "llama3.2:1b", DEFAULT_MODEL];

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

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function learningSummary(stats: FeedbackStats): string {
  const total =
    stats.corrected + stats.rejected + stats.confirmed + stats.pinned;
  if (total === 0) {
    return "No feedback yet. Accept, change or reject AI labels below and the model will learn from them.";
  }
  return `Learning from ${plural(stats.corrected, "correction")}, ${plural(
    stats.rejected,
    "rejection",
  )} and ${plural(stats.confirmed + stats.pinned, "confirmed label")}.`;
}

/** How much of the AI-labelled time the user has judged, by time rather than by row count. */
function ReviewProgress({ stats }: { stats: FeedbackStats }) {
  const labelled = stats.reviewed_seconds + stats.pending_seconds;
  if (labelled <= 0) return null;
  const percent = Math.round((stats.reviewed_seconds / labelled) * 100);
  return (
    <div className="flex flex-col gap-1">
      <div
        className="h-1.5 overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-valuenow={percent}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label="Share of AI-labelled time reviewed"
      >
        <div
          className="h-full rounded-full bg-primary transition-all"
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className="m-0 text-xs text-muted-foreground">
        Reviewed {percent}% of AI-labelled time
        {stats.pending_seconds > 0 &&
          ` — ${formatSeconds(stats.pending_seconds)} left`}
        . Suggestions count in your reports before you review them.
      </p>
    </div>
  );
}

export function AiCategorizationSection() {
  const queryClient = useQueryClient();
  const [model, setModel] = useState(storedModel);
  const [addingModel, setAddingModel] = useState(false);
  const [newModel, setNewModel] = useState("");
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

  const trimmedNewModel = newModel.trim();
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
      setNewModel("");
      setAddingModel(false);
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

  // Reopen the "add model" form so a resumed download's progress bar and Stop button are
  // reachable, even though `addingModel` itself did not survive the reload.
  useEffect(() => {
    if (backendDownloading) {
      setAddingModel(true);
      setNewModel(
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
        <CardTitle>AI categorization</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        {statsQuery.data && (
          <>
            <p className="m-0 text-xs text-muted-foreground">
              {learningSummary(statsQuery.data)}
            </p>
            <ReviewProgress stats={statsQuery.data} />
          </>
        )}
        <div className="flex flex-col gap-1">
          <label htmlFor="ai-model" className="text-xs font-medium">
            Model
          </label>
          {installedModels.length > 0 ? (
            <div className="flex flex-wrap items-center gap-2">
              <Select
                value={effectiveModel}
                onValueChange={(value) => {
                  if (!value) return;
                  setModel(value);
                  rememberModel(value);
                }}
                items={Object.fromEntries(
                  installedModels.map((name) => [name, name]),
                )}
                disabled={running || downloading}
              >
                <SelectTrigger id="ai-model" size="sm" className="w-56">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {installedModels.map((name) => (
                    <SelectItem key={name} value={name}>
                      {name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                size="sm"
                variant="outline"
                disabled={running || downloading}
                onClick={() => setAddingModel((v) => !v)}
              >
                <PlusIcon /> Add model
              </Button>
            </div>
          ) : (
            installedModelsQuery.isSuccess && (
              <p className="m-0 text-xs text-muted-foreground">
                No models downloaded yet. Add one below to get started.
              </p>
            )
          )}
          {(addingModel || installedModels.length === 0) && (
            <div className="mt-1 flex flex-col gap-1">
              <div className="flex flex-wrap items-center gap-2">
                <Input
                  list="ai-model-suggestions"
                  value={newModel}
                  disabled={downloading}
                  placeholder={DEFAULT_MODEL}
                  className="w-56"
                  onChange={(event) => setNewModel(event.target.value)}
                />
                <datalist id="ai-model-suggestions">
                  {SUGGESTED_MODELS.map((name) => (
                    <option key={name} value={name} />
                  ))}
                </datalist>
                {newModelInstalledQuery.data ? (
                  <span className="text-xs text-muted-foreground">
                    Already installed
                  </span>
                ) : (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={!canDownloadNewModel || downloading}
                    onClick={() => downloadMutation.mutate(trimmedNewModel)}
                  >
                    {downloading ? "Downloading…" : "Download"}
                  </Button>
                )}
                {downloading && (
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => void cancelModelDownload()}
                  >
                    Stop
                  </Button>
                )}
                {installedModels.length > 0 && !downloading && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setAddingModel(false);
                      setNewModel("");
                    }}
                  >
                    Cancel
                  </Button>
                )}
              </div>
              <p className="m-0 text-xs text-muted-foreground">
                Pick a suggestion or type any model name from the{" "}
                <a
                  href="https://ollama.com/library"
                  target="_blank"
                  rel="noreferrer"
                  className="underline hover:text-foreground"
                >
                  Ollama library
                </a>
                , e.g. <code>llama3.1:8b</code>.
              </p>
            </div>
          )}
          {downloading && effectiveDownload && effectiveDownload.total > 0 && (
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
        <div className="flex flex-wrap items-center gap-3">
          <Button
            disabled={running || downloading || effectiveModel === ""}
            onClick={() => mutation.mutate(false)}
          >
            {label("Categorize uncategorized", running && !rechecking)}
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
    </Card>
  );
}
