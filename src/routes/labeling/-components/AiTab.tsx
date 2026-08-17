import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  getClassifierStatus,
  runAiCategorization,
  setAiCategorizationSchedule,
} from "@/api";
import type { AiCategorizationSchedule } from "@/types";

const scheduleLabels: Record<AiCategorizationSchedule, string> = {
  manual: "Manual only",
  hourly: "Every hour",
  daily: "Every day",
  weekly: "Every week",
};

function formatTimestamp(value: string | null | undefined) {
  return value ? new Date(value).toLocaleString() : "—";
}

function formatState(state: string | undefined) {
  return state?.replaceAll("_", " ") ?? "Loading…";
}

export function AiTab() {
  const queryClient = useQueryClient();
  const statusQuery = useQuery({
    queryKey: ["classifierStatus"],
    queryFn: getClassifierStatus,
    refetchInterval: (query) =>
      query.state.data?.state === "queued" ||
      query.state.data?.state === "running"
        ? 1000
        : false,
  });
  const run = useMutation({
    mutationFn: runAiCategorization,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] });
    },
  });
  const setSchedule = useMutation({
    mutationFn: setAiCategorizationSchedule,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] });
    },
  });
  const status = statusQuery.data;

  const busy = status?.state === "queued" || status?.state === "running";

  return (
    <Card size="sm" className="max-w-xl">
      <CardHeader>
        <CardTitle>AI categorization</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 text-sm">
        <p className="text-muted-foreground">
          Uses your labels to categorize activity on this device.
        </p>
        <dl className="grid grid-cols-2 gap-x-6 gap-y-2">
          <dt className="text-muted-foreground">Status</dt>
          <dd className="capitalize">{formatState(status?.state)}</dd>
          <dt className="text-muted-foreground">Examples</dt>
          <dd>{status?.training_example_count ?? 0}</dd>
          <dt className="text-muted-foreground">Eligible categories</dt>
          <dd>{status?.trained_category_count ?? 0}</dd>
          <dt className="text-muted-foreground">Last completed</dt>
          <dd>{formatTimestamp(status?.last_completed_at)}</dd>
          {status?.schedule !== "manual" && (
            <>
              <dt className="text-muted-foreground">Next run</dt>
              <dd>{formatTimestamp(status?.next_scheduled_at)}</dd>
            </>
          )}
        </dl>
        <div className="flex items-center gap-3">
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
        {status?.last_error && (
          <p className="text-xs text-destructive">{status.last_error}</p>
        )}
        {(run.error || setSchedule.error) && (
          <p className="text-xs text-destructive">
            {String(run.error ?? setSchedule.error)}
          </p>
        )}
        <Button
          className="self-start"
          variant="outline"
          disabled={run.isPending || busy}
          onClick={() => run.mutate()}
        >
          Run now
        </Button>
      </CardContent>
    </Card>
  );
}
