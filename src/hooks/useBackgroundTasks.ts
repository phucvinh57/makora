import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { categorizeStatus, modelDownloadStatus } from "@/api";
import type { AiCategorizeProgress } from "@/types";

interface DownloadProgress {
  downloaded: number;
  total: number;
}

export interface BackgroundTask {
  id: string;
  label: string;
  progress: string;
}

const RUNNING_POLL_MS = 1_000;
const IDLE_POLL_MS = 4_000;

function formatBytes(bytes: number): string {
  return bytes >= 1e9
    ? `${(bytes / 1e9).toFixed(2)} GB`
    : `${Math.round(bytes / 1e6)} MB`;
}

/**
 * Polls the backend's categorize/download status so a background task started on the
 * Categories page keeps being reported after the user navigates away from it.
 */
export function useBackgroundTasks(): BackgroundTask[] {
  const categorizeStatusQuery = useQuery({
    queryKey: ["aiCategorizeStatus"],
    queryFn: categorizeStatus,
    refetchInterval: (query) =>
      query.state.data?.running ? RUNNING_POLL_MS : IDLE_POLL_MS,
  });
  const downloadStatusQuery = useQuery({
    queryKey: ["aiModelDownloadStatus"],
    queryFn: modelDownloadStatus,
    refetchInterval: (query) =>
      query.state.data?.running ? RUNNING_POLL_MS : IDLE_POLL_MS,
  });
  const [categorizeProgress, setCategorizeProgress] =
    useState<AiCategorizeProgress | null>(null);
  const [downloadProgress, setDownloadProgress] =
    useState<DownloadProgress | null>(null);

  useEffect(() => {
    const unlisten = listen<AiCategorizeProgress>(
      "ai-categorize-progress",
      (event) => setCategorizeProgress(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<DownloadProgress>(
      "ai-model-download-progress",
      (event) => setDownloadProgress(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const tasks: BackgroundTask[] = [];

  if (categorizeStatusQuery.data?.running) {
    const status = categorizeStatusQuery.data;
    const done = categorizeProgress?.done ?? status.done;
    const total = categorizeProgress?.total ?? status.total;
    tasks.push({
      id: "categorize",
      label: status.recheck
        ? "Re-checking AI labels"
        : "Categorizing activities",
      progress: total > 0 ? `${done}/${total}` : "Loading model…",
    });
  }

  if (downloadStatusQuery.data?.running) {
    const status = downloadStatusQuery.data;
    const downloaded = downloadProgress?.downloaded ?? status.downloaded;
    const total = downloadProgress?.total ?? status.total;
    tasks.push({
      id: "download",
      label: `Downloading ${status.model}`,
      progress:
        total > 0
          ? `${formatBytes(downloaded)} of ${formatBytes(total)}`
          : "Starting…",
    });
  }

  return tasks;
}
