import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  getClassifierStatus,
  setAiCategorizationConfidenceThreshold,
  setAiCategorizationSchedule,
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
import type { AiCategorizationSchedule } from "@/types";

const scheduleLabels: Record<AiCategorizationSchedule, string> = {
  manual: "Manual only",
  hourly: "Every hour",
  daily: "Every day",
  weekly: "Every week",
};

export function AiCategorizationSettingsSection() {
  const queryClient = useQueryClient();
  const [confidenceDraft, setConfidenceDraft] = useState("");
  const statusQuery = useQuery({
    queryKey: ["classifierStatus"],
    queryFn: getClassifierStatus,
    refetchInterval: (query) =>
      query.state.data?.state === "queued" ||
      query.state.data?.state === "running"
        ? 1000
        : false,
  });
  const status = statusQuery.data;
  const setSchedule = useMutation({
    mutationFn: setAiCategorizationSchedule,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] }),
  });
  const setConfidenceThreshold = useMutation({
    mutationFn: setAiCategorizationConfidenceThreshold,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] }),
  });

  useEffect(() => {
    if (status) setConfidenceDraft(String(status.confidence_threshold * 100));
  }, [status?.confidence_threshold, status]);

  const confidencePercent = Number(confidenceDraft);
  const confidenceIsValid =
    confidenceDraft.trim() !== "" &&
    Number.isFinite(confidencePercent) &&
    confidencePercent >= 50 &&
    confidencePercent <= 99;
  const confidenceIsUnchanged =
    status !== undefined &&
    Math.abs(confidencePercent / 100 - status.confidence_threshold) < 0.000001;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>AI categorization settings</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 text-sm">
        <div className="flex flex-wrap items-center gap-3">
          <label htmlFor="ai-schedule" className="text-muted-foreground">
            Schedule
          </label>
          <Select
            value={status?.schedule ?? "manual"}
            disabled={statusQuery.isLoading || setSchedule.isPending}
            onValueChange={(value) =>
              setSchedule.mutate(value as AiCategorizationSchedule)
            }
          >
            <SelectTrigger id="ai-schedule" size="sm" className="w-36">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Object.entries(scheduleLabels).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <p className="-mt-2 text-xs text-muted-foreground">
          Hourly runs at the start of each hour; daily runs at midnight.
        </p>
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-3">
            <label htmlFor="ai-confidence" className="text-muted-foreground">
              Confidence threshold
            </label>
            <Input
              id="ai-confidence"
              className="w-24"
              type="number"
              min={50}
              max={99}
              step={1}
              value={confidenceDraft}
              disabled={
                statusQuery.isLoading || setConfidenceThreshold.isPending
              }
              onChange={(event) => setConfidenceDraft(event.target.value)}
            />
            <span className="text-muted-foreground">%</span>
            <Button
              size="sm"
              variant="outline"
              disabled={
                statusQuery.isLoading ||
                setConfidenceThreshold.isPending ||
                !confidenceIsValid ||
                confidenceIsUnchanged
              }
              onClick={() =>
                setConfidenceThreshold.mutate(confidencePercent / 100)
              }
            >
              Save
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            Lower values suggest more activity with greater error risk.
          </p>
        </div>
        {(status?.last_error ||
          setSchedule.error ||
          setConfidenceThreshold.error) && (
          <p className="text-xs text-destructive">
            {String(
              status?.last_error ??
                setSchedule.error ??
                setConfidenceThreshold.error,
            )}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
