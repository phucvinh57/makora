import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { setMinSessionSeconds } from "@/api";
import { queryKeys, settingsQueryOptions } from "@/queries";

/** Mirrors `MIN_SESSION_SECONDS_RANGE` in the backend, which is what actually enforces it. */
const MIN_SECONDS = 1;
const MAX_SECONDS = 300;

export function TrackingSection() {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery(settingsQueryOptions);
  const saved = settingsQuery.data?.tracking.min_session_seconds;

  // What is being typed, until it is saved or abandoned. `null` shows the saved value.
  const [draft, setDraft] = useState<string | null>(null);

  const saveMutation = useMutation({
    mutationFn: setMinSessionSeconds,
    onSuccess: (settings) => {
      queryClient.setQueryData(queryKeys.settings, settings);
      setDraft(null);
    },
  });

  const commit = () => {
    if (draft === null) return;
    const seconds = Number(draft);
    if (!Number.isInteger(seconds) || draft.trim() === "") {
      setDraft(null);
      return;
    }
    if (seconds === saved) {
      setDraft(null);
      return;
    }
    saveMutation.mutate(seconds);
  };

  const value = draft ?? (saved === undefined ? "" : String(saved));

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Tracking</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-4">
          <label htmlFor="min-session-seconds" className="text-sm">
            Ignore sessions shorter than
          </label>
          <div className="flex items-center gap-2">
            <Input
              id="min-session-seconds"
              type="number"
              inputMode="numeric"
              min={MIN_SECONDS}
              max={MAX_SECONDS}
              step={1}
              className="w-20"
              value={value}
              disabled={settingsQuery.isLoading || saveMutation.isPending}
              aria-invalid={saveMutation.isError}
              onChange={(event) => {
                saveMutation.reset();
                setDraft(event.target.value);
              }}
              onBlur={commit}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
                if (event.key === "Escape") setDraft(null);
              }}
            />
            <span className="text-sm text-muted-foreground">seconds</span>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          Activity shorter than this is discarded when it ends. Already recorded
          history is kept.
        </p>
        {saveMutation.isError && (
          <p className="text-xs text-destructive">
            {String(saveMutation.error)}
          </p>
        )}
        {settingsQuery.isError && (
          <p className="text-xs text-destructive">
            {String(settingsQuery.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
