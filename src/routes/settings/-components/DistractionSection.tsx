import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  getDistractionThresholdSeconds,
  setDistractionThresholdSeconds,
} from "@/api";

export function DistractionSection() {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState("");

  const thresholdQuery = useQuery({
    queryKey: ["distractionThreshold"],
    queryFn: getDistractionThresholdSeconds,
  });
  const saved = thresholdQuery.data;

  useEffect(() => {
    if (saved !== undefined) setDraft(String(saved));
  }, [saved]);

  const saveMutation = useMutation({
    mutationFn: (seconds: number) => setDistractionThresholdSeconds(seconds),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["distractionThreshold"] });
      // Changing the cutoff re-labels past activities in both directions, so everything
      // reading a category is now out of date.
      queryClient.invalidateQueries({ queryKey: ["sessions"] });
      queryClient.invalidateQueries({ queryKey: ["activityStats"] });
      toast.success("Updated — past activities were relabelled to match.");
    },
    onError: (err) => toast.error(String(err)),
  });

  const parsed = Number(draft);
  const valid = draft.trim() !== "" && Number.isInteger(parsed) && parsed >= 0;
  const dirty = saved !== undefined && draft !== String(saved);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!valid || !dirty) return;
    saveMutation.mutate(parsed);
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Distractions</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <form onSubmit={handleSubmit} className="flex items-center gap-2">
          <label htmlFor="distraction-threshold" className="text-sm">
            Count activities shorter than
          </label>
          <Input
            id="distraction-threshold"
            type="number"
            min={0}
            step={1}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            disabled={thresholdQuery.isLoading}
            className="w-20"
          />
          <span className="text-sm">seconds as distractions</span>
          <Button
            type="submit"
            size="sm"
            disabled={!valid || !dirty || saveMutation.isPending}
          >
            Save
          </Button>
        </form>
        <p className="m-0 text-xs text-muted-foreground">
          Flicking through apps and tabs leaves a trail of one- and two-second
          activities that no rule was ever written for. Those land in the
          built-in <strong>Distraction</strong> category instead of cluttering
          "Uncategorized". A brief switch into something one of your rules
          already covers still counts toward that category — only unmatched
          activities are filed this way. Set it to <strong>0</strong> to turn
          the feature off.
        </p>
        {!valid && draft.trim() !== "" && (
          <p className="m-0 text-xs text-destructive">
            Enter a whole number of seconds, 0 or greater.
          </p>
        )}
      </CardContent>
    </Card>
  );
}
