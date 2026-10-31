import { formatDistanceToNow } from "date-fns";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { settings } from "@/api";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  categorizeStatusQueryOptions,
  nextAutoRunQueryOptions,
  queryKeys,
  settingsQueryOptions,
} from "@/queries";

/** Mirrors `AUTO_CATEGORIZE_HOURS_OPTIONS` in the backend, which is what actually enforces it. */
const PERIODS = [
  { hours: 3, label: "3 hours" },
  { hours: 6, label: "6 hours" },
  { hours: 12, label: "12 hours" },
  { hours: 24, label: "day" },
  { hours: 72, label: "3 days" },
  { hours: 168, label: "week" },
];
const DEFAULT_HOURS = 24;

const items = Object.fromEntries(
  PERIODS.map(({ hours, label }) => [String(hours), label]),
);

function describeNextRun(next: string, running: boolean) {
  if (running) return "A run is in progress.";
  const at = new Date(next);
  if (at.getTime() - Date.now() < 60_000) return "Next run: any moment now.";
  return `Next run: ${at.toLocaleString()} (${formatDistanceToNow(at, { addSuffix: true })}).`;
}

export function AutoCategorizeSetting() {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery(settingsQueryOptions);
  const saved = settingsQuery.data?.ai.auto_categorize_hours;

  const saveMutation = useMutation({
    mutationFn: settings.setAutoCategorizeHours,
    onSuccess: (next) => queryClient.setQueryData(queryKeys.settings, next),
  });

  const enabled = saved !== undefined && saved > 0;

  const running = useQuery(categorizeStatusQueryOptions).data?.running ?? false;
  // A finished run or a changed period moves the due time, so either one is a new query.
  const nextRun = useQuery(nextAutoRunQueryOptions(saved, running)).data;

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-2">
        <Switch
          id="auto-categorize"
          checked={enabled}
          disabled={saved === undefined || saveMutation.isPending}
          onCheckedChange={(on) => {
            saveMutation.reset();
            saveMutation.mutate(on ? DEFAULT_HOURS : 0);
          }}
        />
        <label htmlFor="auto-categorize" className="text-xs font-medium">
          Categorize and remind me to review every
        </label>
        <Select
          value={enabled ? String(saved) : null}
          items={items}
          disabled={!enabled || saveMutation.isPending}
          onValueChange={(next) => {
            if (!next) return;
            saveMutation.reset();
            saveMutation.mutate(Number(next));
          }}
        >
          <SelectTrigger
            id="auto-categorize-period"
            size="sm"
            className="w-28"
            aria-label="Period between automatic runs"
            aria-invalid={saveMutation.isError}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PERIODS.map(({ hours, label }) => (
              <SelectItem key={hours} value={String(hours)}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <p className="m-0 text-xs text-muted-foreground">
        {enabled && nextRun && `${describeNextRun(nextRun, running)}`}
      </p>
      {saveMutation.isError && (
        <p className="m-0 text-xs text-destructive">
          {String(saveMutation.error)}
        </p>
      )}
    </div>
  );
}
