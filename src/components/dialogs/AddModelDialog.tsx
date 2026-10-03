import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import { aiModel } from "@/api";
import { formatBytes } from "@/bytes";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useDialog } from "@/hooks/useDialog";
import type { ModelRole } from "@/types";
import {
  downloadStatusQueryOptions,
  markDownloadStarted,
  queryKeys,
} from "@/queries";

const ROLE_OPTIONS: {
  role: ModelRole;
  label: string;
  hint: string;
  suggestions: string[];
}[] = [
  {
    role: "embedding",
    label: "Embedding model",
    hint: "Small and fast. Finds the activities you filed that are most like each new one. Runs on any machine.",
    suggestions: [
      "all-minilm",
      "nomic-embed-text",
      "mxbai-embed-large",
      "paraphrase-multilingual",
    ],
  },
  {
    role: "llm",
    label: "LLM",
    hint: "A language model that labels each activity, shown the ones you filed that look like it. Accurate, but needs a capable machine.",
    suggestions: ["llama3.2", "llama3.2:1b", "qwen2.5:7b-instruct"],
  },
];

function DownloadProgress({
  downloaded,
  total,
}: {
  downloaded: number;
  total: number;
}) {
  return (
    <div className="flex flex-col gap-1">
      <div
        className="h-1.5 overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-valuenow={Math.round((downloaded / total) * 100)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label="Model download progress"
      >
        <div
          className="h-full rounded-full bg-primary transition-all"
          style={{ width: `${(downloaded / total) * 100}%` }}
        />
      </div>
      <p className="m-0 text-xs text-muted-foreground">
        {formatBytes(downloaded)} of {formatBytes(total)}
      </p>
    </div>
  );
}

/** Mounted only while the dialog is open, so the typed name starts empty every time. */
function AddModelForm({
  initialRole,
  initialModel,
  downloading,
  progress,
  error,
  runningModel,
  onDownload,
  onStop,
  onCancel,
}: {
  initialRole: ModelRole;
  initialModel: string;
  downloading: boolean;
  progress: { downloaded: number; total: number } | null;
  error: unknown;
  runningModel: string | undefined;
  onDownload: (role: ModelRole, model: string) => void;
  onStop: () => void;
  onCancel: () => void;
}) {
  const [role, setRole] = useState(initialRole);
  const [typedName, setTypedName] = useState(initialModel);
  const { hint, suggestions } =
    ROLE_OPTIONS.find((option) => option.role === role) ?? ROLE_OPTIONS[0];
  const name = downloading && runningModel ? runningModel : typedName;
  const trimmed = name.trim();

  const installedQuery = useQuery({
    queryKey: [...queryKeys.aiModelInstalled, trimmed],
    queryFn: () => aiModel.isInstalled(trimmed),
    enabled: trimmed !== "",
  });
  const canDownload =
    trimmed !== "" && installedQuery.isSuccess && !installedQuery.data;

  return (
    <>
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
          to use for AI categorization.
        </DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-2">
        <div className="flex gap-2" role="radiogroup" aria-label="Model type">
          {ROLE_OPTIONS.map((option) => (
            <Button
              key={option.role}
              type="button"
              size="sm"
              role="radio"
              aria-checked={role === option.role}
              variant={role === option.role ? "default" : "outline"}
              disabled={downloading}
              onClick={() => setRole(option.role)}
            >
              {option.label}
            </Button>
          ))}
        </div>
        <p className="m-0 text-xs text-muted-foreground">{hint}</p>
        <Input
          list="ai-model-suggestions"
          value={name}
          disabled={downloading}
          placeholder={suggestions[0]}
          autoFocus
          onChange={(event) => setTypedName(event.target.value)}
        />
        <datalist id="ai-model-suggestions">
          {suggestions.map((suggestion) => (
            <option key={suggestion} value={suggestion} />
          ))}
        </datalist>
        {trimmed !== "" && installedQuery.data && (
          <p className="m-0 text-xs text-muted-foreground">
            Already installed.
          </p>
        )}
        {downloading && progress && progress.total > 0 && (
          <DownloadProgress
            downloaded={progress.downloaded}
            total={progress.total}
          />
        )}
        {error != null && (
          <p className="m-0 text-xs text-destructive">{String(error)}</p>
        )}
      </div>
      <DialogFooter>
        {downloading ? (
          <Button type="button" variant="destructive" onClick={onStop}>
            Stop
          </Button>
        ) : (
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        )}
        <Button
          type="button"
          disabled={!canDownload || downloading}
          onClick={() => onDownload(role, trimmed)}
        >
          {downloading ? "Downloading…" : "Download"}
        </Button>
      </DialogFooter>
    </>
  );
}

export interface AddModelDialogData {
  /** The role and name to start from, e.g. the model a run needs and does not have yet. */
  role?: ModelRole;
  model?: string;
  /** Called with the model's role and name once its download has finished and it is installed. */
  onDownloaded?: (role: ModelRole, model: string) => void;
}

export function AddModelDialog() {
  const queryClient = useQueryClient();
  const { open, setOpen, data } = useDialog<AddModelDialogData>("add-model");
  const status = useQuery(downloadStatusQueryOptions).data;

  const downloadMutation = useMutation({
    mutationFn: ({ model }: { role: ModelRole; model: string }) =>
      aiModel.download(model),
    onMutate: ({ model }) => markDownloadStarted(queryClient, model),
    onSuccess: async (finished, { role, model }) => {
      if (!finished) return;
      toast.success(`Downloaded ${model}`);
      await queryClient.invalidateQueries({
        queryKey: queryKeys.aiInstalledModels,
      });
      data?.onDownloaded?.(role, model);
      setOpen(false);
    },
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({
          queryKey: queryKeys.aiModelDownloadStatus,
        }),
        queryClient.invalidateQueries({
          queryKey: queryKeys.aiModelInstalled,
        }),
      ]),
  });
  const stopMutation = useMutation({
    mutationFn: aiModel.cancelDownload,
    onError: (err) => toast.error(String(err)),
  });

  const downloading = downloadMutation.isPending || (status?.running ?? false);

  function handleOpenChange(next: boolean) {
    // A failed download's message should not greet the next time the dialog opens.
    if (!next && !downloadMutation.isPending) downloadMutation.reset();
    setOpen(next);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <AddModelForm
          initialRole={data?.role ?? "embedding"}
          initialModel={data?.model ?? ""}
          downloading={downloading}
          progress={status?.running ? status : null}
          error={downloadMutation.error}
          runningModel={status?.running ? status.model : undefined}
          onDownload={(role, model) => downloadMutation.mutate({ role, model })}
          onStop={() => stopMutation.mutate()}
          onCancel={() => handleOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
