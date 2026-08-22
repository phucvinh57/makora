import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  acceptCategorySuggestions,
  dismissCategorySuggestions,
  getClassifierStatus,
  listCategories,
  listCategorySuggestions,
  listMultiPurposeKeys,
  runAiCategorization,
  setAiCategorizationConfidenceThreshold,
  setAiCategorizationSchedule,
  setMultiPurposeKey,
} from "@/api";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
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
  const [confidenceDraft, setConfidenceDraft] = useState("");
  const [acceptCutoff, setAcceptCutoff] = useState("");
  const [selectedCategories, setSelectedCategories] = useState<
    Record<number, string>
  >({});
  const [multiKeyKind, setMultiKeyKind] = useState<"exe" | "domain">("domain");
  const [multiKeyDraft, setMultiKeyDraft] = useState("");
  const statusQuery = useQuery({
    queryKey: ["classifierStatus"],
    queryFn: getClassifierStatus,
    refetchInterval: (query) =>
      query.state.data?.state === "queued" ||
      query.state.data?.state === "running"
        ? 1000
        : false,
  });
  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const suggestionsQuery = useQuery({
    queryKey: ["categorySuggestions"],
    queryFn: listCategorySuggestions,
  });
  const multiPurposeQuery = useQuery({
    queryKey: ["multiPurposeKeys"],
    queryFn: listMultiPurposeKeys,
  });
  const run = useMutation({
    mutationFn: runAiCategorization,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] }),
  });
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

  function invalidateAccepted() {
    for (const key of [
      "categories",
      "categoryRules",
      "sessions",
      "activityStats",
      "workSessionStats",
      "categorySuggestions",
      "multiPurposeKeys",
      "classifierStatus",
    ]) {
      queryClient.invalidateQueries({ queryKey: [key] });
    }
  }

  const setMultiPurpose = useMutation({
    mutationFn: ({
      keyKind,
      key,
      enabled,
    }: {
      keyKind: "exe" | "domain";
      key: string;
      enabled: boolean;
    }) => setMultiPurposeKey(keyKind, key, enabled),
    onSuccess: () => {
      setMultiKeyDraft("");
      invalidateAccepted();
    },
  });

  const accept = useMutation({
    mutationFn: ({ id, categoryId }: { id: number; categoryId: number }) =>
      acceptCategorySuggestions([id], categoryId),
    onSuccess: invalidateAccepted,
  });
  const acceptAll = useMutation({
    mutationFn: (ids: number[]) => acceptCategorySuggestions(ids),
    onSuccess: invalidateAccepted,
  });
  const dismiss = useMutation({
    mutationFn: (id: number) => dismissCategorySuggestions([id]),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["categorySuggestions"] }),
  });
  const status = statusQuery.data;
  const categories = categoriesQuery.data ?? [];
  const suggestions = suggestionsQuery.data ?? [];
  const multiPurposeKeys = multiPurposeQuery.data ?? [];

  useEffect(() => {
    if (status) {
      setConfidenceDraft(String(status.confidence_threshold * 100));
      if (!acceptCutoff)
        setAcceptCutoff(String(status.confidence_threshold * 100));
    }
  }, [status?.confidence_threshold, status, acceptCutoff]);

  const busy = status?.state === "queued" || status?.state === "running";
  const confidencePercent = Number(confidenceDraft);
  const confidenceIsValid =
    confidenceDraft.trim() !== "" &&
    Number.isFinite(confidencePercent) &&
    confidencePercent >= 50 &&
    confidencePercent <= 99;
  const confidenceIsUnchanged =
    status !== undefined &&
    Math.abs(confidencePercent / 100 - status.confidence_threshold) < 0.000001;
  const cutoffPercent = Number(acceptCutoff);
  const cutoffIsValid =
    Number.isFinite(cutoffPercent) &&
    cutoffPercent >= 50 &&
    cutoffPercent <= 99;
  const bulkSuggestions = cutoffIsValid
    ? suggestions.filter(
        (suggestion) => suggestion.confidence * 100 >= cutoffPercent,
      )
    : [];

  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <Card size="sm">
        <CardHeader>
          <CardTitle>AI categorization</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 text-sm">
          <p className="text-muted-foreground">
            Uses offline embeddings to suggest rules and label individual
            sessions for multi-purpose keys. URL paths can graduate into
            reviewable rules.
          </p>
          <dl className="grid grid-cols-2 gap-x-6 gap-y-2">
            <dt className="text-muted-foreground">Status</dt>
            <dd className="capitalize">{formatState(status?.state)}</dd>
            <dt className="text-muted-foreground">Labeled keys</dt>
            <dd>{status?.training_example_count ?? 0}</dd>
            <dt className="text-muted-foreground">Categories with vectors</dt>
            <dd>{status?.trained_category_count ?? 0}</dd>
            <dt className="text-muted-foreground">Holdout accuracy</dt>
            <dd>
              {status?.holdout_accuracy == null
                ? "Not enough labeled keys"
                : `${Math.round(status.holdout_accuracy * 100)}% (${status.holdout_sample_count} keys)`}
            </dd>
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
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-3">
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
          {status?.last_error && (
            <p className="text-xs text-destructive">{status.last_error}</p>
          )}
          {(run.error || setSchedule.error || setConfidenceThreshold.error) && (
            <p className="text-xs text-destructive">
              {String(
                run.error ?? setSchedule.error ?? setConfidenceThreshold.error,
              )}
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

      <Card size="sm">
        <CardHeader>
          <CardTitle>Multi-purpose sites</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-sm">
          <p className="text-muted-foreground">
            These apps and sites are too varied for one blanket category, so
            their titles are classified per activity. Existing exact rules are
            kept but become inactive while a key is enabled.
          </p>
          <form
            className="flex flex-wrap items-center gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              if (multiKeyDraft.trim()) {
                setMultiPurpose.mutate({
                  keyKind: multiKeyKind,
                  key: multiKeyDraft.trim(),
                  enabled: true,
                });
              }
            }}
          >
            <Select
              value={multiKeyKind}
              onValueChange={(value) =>
                setMultiKeyKind(value as "exe" | "domain")
              }
              items={{ domain: "Site", exe: "App" }}
            >
              <SelectTrigger size="sm" className="w-24">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="domain">Site</SelectItem>
                <SelectItem value="exe">App</SelectItem>
              </SelectContent>
            </Select>
            <Input
              className="min-w-48 flex-1 font-mono"
              placeholder="youtube.com or chrome"
              value={multiKeyDraft}
              onChange={(event) => setMultiKeyDraft(event.target.value)}
            />
            <Button
              size="sm"
              type="submit"
              disabled={setMultiPurpose.isPending || !multiKeyDraft.trim()}
            >
              Add
            </Button>
          </form>
          {multiPurposeKeys.length === 0 ? (
            <p className="text-muted-foreground">No multi-purpose keys yet.</p>
          ) : (
            <div className="flex flex-col gap-2">
              {multiPurposeKeys.map((entry) => (
                <div
                  key={`${entry.key_kind}:${entry.key}`}
                  className={`flex flex-wrap items-center gap-2 rounded-lg border p-2.5 ${
                    entry.enabled ? "" : "opacity-60"
                  }`}
                >
                  <span className="min-w-40 flex-1 font-mono">{entry.key}</span>
                  <Badge variant="secondary">{entry.key_kind}</Badge>
                  <Badge variant="outline">{entry.source}</Badge>
                  {entry.top_share != null && (
                    <span className="text-xs text-muted-foreground">
                      top {Math.round(entry.top_share * 100)}%
                    </span>
                  )}
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={setMultiPurpose.isPending}
                    onClick={() =>
                      setMultiPurpose.mutate({
                        keyKind: entry.key_kind,
                        key: entry.key,
                        enabled: !entry.enabled,
                      })
                    }
                  >
                    {entry.enabled ? "Remove" : "Enable"}
                  </Button>
                </div>
              ))}
            </div>
          )}
          {multiPurposeQuery.error || setMultiPurpose.error ? (
            <p className="text-xs text-destructive">
              {String(multiPurposeQuery.error ?? setMultiPurpose.error)}
            </p>
          ) : null}
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader>
          <CardTitle>Suggested rules</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-sm">
          {suggestions.length === 0 ? (
            <p className="text-muted-foreground">
              No pending suggestions. Run AI after recording or labeling more
              activity.
            </p>
          ) : (
            <>
              <div className="flex flex-wrap items-center gap-2 border-b pb-3">
                <span className="text-muted-foreground">Accept all above</span>
                <Input
                  className="w-20"
                  type="number"
                  min={50}
                  max={99}
                  value={acceptCutoff}
                  onChange={(event) => setAcceptCutoff(event.target.value)}
                />
                <span className="text-muted-foreground">%</span>
                <Button
                  size="sm"
                  disabled={
                    !cutoffIsValid ||
                    bulkSuggestions.length === 0 ||
                    acceptAll.isPending
                  }
                  onClick={() =>
                    acceptAll.mutate(
                      bulkSuggestions.map((suggestion) => suggestion.id),
                    )
                  }
                >
                  Accept {bulkSuggestions.length || "all"}
                </Button>
              </div>
              {suggestions.map((suggestion) => (
                <div
                  key={suggestion.id}
                  className="flex flex-wrap items-center gap-2 rounded-lg border p-3"
                >
                  <div className="min-w-48 flex-1">
                    <div className="font-medium">{suggestion.key}</div>
                    <div className="text-xs text-muted-foreground">
                      {suggestion.key_kind} ·{" "}
                      {formatSeconds(suggestion.seconds)} observed ·{" "}
                      {Math.round(suggestion.confidence * 100)}% match
                    </div>
                  </div>
                  <Select
                    value={
                      selectedCategories[suggestion.id] ??
                      String(suggestion.category_id)
                    }
                    onValueChange={(value) =>
                      setSelectedCategories((current) => ({
                        ...current,
                        [suggestion.id]:
                          value ?? String(suggestion.category_id),
                      }))
                    }
                    items={categoryValueLabels(categories)}
                  >
                    <SelectTrigger size="sm" className="w-44">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>{categoryOptions(categories)}</SelectContent>
                  </Select>
                  <Button
                    size="sm"
                    disabled={accept.isPending}
                    onClick={() =>
                      accept.mutate({
                        id: suggestion.id,
                        categoryId: Number(
                          selectedCategories[suggestion.id] ??
                            suggestion.category_id,
                        ),
                      })
                    }
                  >
                    Accept
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={dismiss.isPending}
                    onClick={() => dismiss.mutate(suggestion.id)}
                  >
                    Dismiss
                  </Button>
                </div>
              ))}
            </>
          )}
          {(suggestionsQuery.error ||
            accept.error ||
            acceptAll.error ||
            dismiss.error) && (
            <p className="text-xs text-destructive">
              {String(
                suggestionsQuery.error ??
                  accept.error ??
                  acceptAll.error ??
                  dismiss.error,
              )}
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
