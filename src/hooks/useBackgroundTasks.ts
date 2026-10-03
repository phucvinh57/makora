import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { formatBytes } from "@/bytes";
import {
  categorizeStatusQueryOptions,
  downloadStatusQueryOptions,
  invalidateCategorizationQueries,
  queryKeys,
} from "@/queries";
import type {
  AiCategorizeProgress,
  AiCategorizeStatus,
  ModelDownloadStatus,
} from "@/types";

export interface BackgroundTask {
  id: string;
  label: string;
  progress: string;
}

/**
 * Mounted once, at the root: folds the backend's progress events into the status queries so
 * every reader of those queries sees live numbers, and refreshes the data a task changes when
 * it ends — even when no page was watching it run.
 */
export function useBackgroundTaskSync() {
  const queryClient = useQueryClient();
  const categorizing =
    useQuery(categorizeStatusQueryOptions).data?.running ?? false;
  const downloading =
    useQuery(downloadStatusQueryOptions).data?.running ?? false;
  const wasCategorizing = useRef(false);
  const wasDownloading = useRef(false);

  useEffect(() => {
    const unlisten = listen<AiCategorizeProgress>(
      "ai-categorize-progress",
      ({ payload }) =>
        queryClient.setQueryData<AiCategorizeStatus>(
          queryKeys.aiCategorizeStatus,
          (old) => old && { ...old, ...payload },
        ),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, [queryClient]);

  useEffect(() => {
    const unlisten = listen<Pick<ModelDownloadStatus, "downloaded" | "total">>(
      "ai-model-download-progress",
      ({ payload }) =>
        queryClient.setQueryData<ModelDownloadStatus>(
          queryKeys.aiModelDownloadStatus,
          (old) => old && { ...old, ...payload },
        ),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, [queryClient]);

  useEffect(() => {
    if (wasCategorizing.current && !categorizing) {
      void invalidateCategorizationQueries(queryClient);
    }
    wasCategorizing.current = categorizing;
  }, [categorizing, queryClient]);

  useEffect(() => {
    if (wasDownloading.current && !downloading) {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.aiInstalledModels,
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.aiModelInstalled,
      });
    }
    wasDownloading.current = downloading;
  }, [downloading, queryClient]);
}

export function useBackgroundTasks(): BackgroundTask[] {
  const categorizeStatus = useQuery(categorizeStatusQueryOptions).data;
  const downloadStatus = useQuery(downloadStatusQueryOptions).data;
  const tasks: BackgroundTask[] = [];

  if (categorizeStatus?.running) {
    const { done, total, recheck } = categorizeStatus;
    tasks.push({
      id: "categorize",
      label: recheck ? "Re-checking AI labels" : "Categorizing activities",
      progress: total > 0 ? `${done}/${total}` : "Starting…",
    });
  }

  if (downloadStatus?.running) {
    const { downloaded, total, model } = downloadStatus;
    tasks.push({
      id: "download",
      label: `Downloading ${model}`,
      progress:
        total > 0
          ? `${formatBytes(downloaded)} of ${formatBytes(total)}`
          : "Starting…",
    });
  }

  return tasks;
}
