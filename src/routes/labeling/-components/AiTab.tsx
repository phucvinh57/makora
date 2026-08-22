import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  acceptCategorySuggestions,
  dismissCategorySuggestions,
  getClassifierStatus,
  listCategories,
  listCategorySuggestions,
  listMultiPurposeKeys,
  runAiCategorization,
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

function formatTimestamp(value: string | null | undefined) {
  return value ? new Date(value).toLocaleString() : "—";
}

function formatExecutionTime(milliseconds: number | null | undefined) {
  if (milliseconds == null) return "—";
  if (milliseconds < 1000) return `${milliseconds} ms`;
  return formatSeconds(Math.ceil(milliseconds / 1000));
}

function externalSuggestionUrl(
  keyKind: "exe" | "domain" | "url",
  key: string,
): string | null {
  if (keyKind === "exe") return null;

  const value = key.trim();
  if (!value) return null;

  const candidate = /^[a-z][a-z\d+.-]*:\/\//i.test(value)
    ? value
    : `https://${value}`;

  try {
    const url = new URL(candidate);
    return url.protocol === "http:" || url.protocol === "https:"
      ? url.href
      : null;
  } catch {
    return null;
  }
}

function openSuggestion(url: string) {
  void openUrl(url).catch((error) => {
    toast.error("Could not open link", { description: String(error) });
  });
}

function SuggestedKey({
  keyKind,
  value,
}: {
  keyKind: "exe" | "domain" | "url";
  value: string;
}) {
  const url = externalSuggestionUrl(keyKind, value);
  return url ? (
    <a
      href={url}
      className="block truncate font-medium text-primary underline-offset-4 hover:underline"
      onClick={(event) => {
        event.preventDefault();
        openSuggestion(url);
      }}
    >
      {value}
    </a>
  ) : (
    <div className="truncate font-medium">{value}</div>
  );
}

export function AiTab() {
  const queryClient = useQueryClient();
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
  const busy = status?.state === "queued" || status?.state === "running";

  useEffect(() => {
    if (status && !acceptCutoff)
      setAcceptCutoff(String(status.confidence_threshold * 100));
  }, [status?.confidence_threshold, status, acceptCutoff]);

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
    <div className="flex w-full flex-col gap-4 @4xl/main:grid @4xl/main:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <div className="flex min-w-0 flex-col gap-4">
        <Card size="sm">
          <CardHeader>
            <CardTitle>AI categorization</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 text-sm">
            <dl className="grid grid-cols-2 gap-x-6 gap-y-2">
              <dt className="text-muted-foreground">Last run</dt>
              <dd>{formatTimestamp(status?.last_completed_at)}</dd>
              <dt className="text-muted-foreground">Next run</dt>
              <dd>{formatTimestamp(status?.next_scheduled_at)}</dd>
              <dt className="text-muted-foreground">Execution time</dt>
              <dd>{formatExecutionTime(status?.last_execution_duration_ms)}</dd>
            </dl>
            {run.error && (
              <p className="text-xs text-destructive">{String(run.error)}</p>
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
              <p className="text-muted-foreground">
                No multi-purpose keys yet.
              </p>
            ) : (
              <div className="flex flex-col gap-2">
                {multiPurposeKeys.map((entry) => (
                  <div
                    key={`${entry.key_kind}:${entry.key}`}
                    className={`flex flex-wrap items-center gap-2 rounded-lg border p-2.5 ${
                      entry.enabled ? "" : "opacity-60"
                    }`}
                  >
                    <span className="min-w-40 flex-1 font-mono">
                      {entry.key}
                    </span>
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
      </div>

      <div className="min-w-0">
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
                  <span className="text-muted-foreground">
                    Accept all above
                  </span>
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
                      <SuggestedKey
                        keyKind={suggestion.key_kind}
                        value={suggestion.key}
                      />
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
                      <SelectTrigger
                        size="sm"
                        className="w-fit min-w-52 max-w-80"
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {categoryOptions(categories)}
                      </SelectContent>
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
    </div>
  );
}
