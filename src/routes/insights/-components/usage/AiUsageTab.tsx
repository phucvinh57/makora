import { useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { CalendarClock, MousePointerClick, Pencil } from "lucide-react";
import { useState } from "react";
import {
  averageLatencyMs,
  cacheHitRatio,
  estimateCost,
  findPrice,
  formatCost,
  formatCostExact,
  formatMs,
  formatPercent,
  formatRate,
  formatTokens,
  itemsPerMinute,
  outputTokensPerSecond,
  overallOutputTokensPerSecond,
  speedDetail,
} from "@/aiUsage";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { openDialog } from "@/hooks/useDialog";
import { cn } from "@/lib/utils";
import {
  aiUsageRunsQueryOptions,
  aiUsageSummaryQueryOptions,
  settingsQueryOptions,
} from "@/queries";
import type { AiRunView, ModelPrice, ModelUsage, RunStatus } from "@/types";
import { AiCostChart } from "./AiCostChart";

const PERIODS = [
  { value: "7", label: "7d", days: 7 },
  { value: "30", label: "30d", days: 30 },
  { value: "all", label: "All", days: null },
] as const;

type Period = (typeof PERIODS)[number]["value"];

const RECENT_RUNS = 8;

const NUMERIC = "text-right tabular-nums";

function Stat({
  label,
  value,
  detail,
}: {
  label: string;
  value: string;
  detail?: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-md border px-3 py-2">
      <span className="truncate text-xs text-muted-foreground">{label}</span>
      <span className="text-base font-medium tabular-nums">{value}</span>
      {detail && (
        <span className="truncate text-xs text-muted-foreground">{detail}</span>
      )}
    </div>
  );
}

function ModelName({
  provider,
  model,
  providerNames,
}: {
  provider: string | null;
  model: string;
  providerNames: Map<string, string>;
}) {
  const source =
    provider === null ? "Local" : (providerNames.get(provider) ?? provider);
  return (
    <div className="flex max-w-56 flex-col">
      <span className="truncate" title={model}>
        {model}
      </span>
      <span className="truncate text-xs text-muted-foreground">{source}</span>
    </div>
  );
}

const STATUS_LABELS: Record<RunStatus, string> = {
  completed: "Done",
  stopped: "Stopped",
  failed: "Failed",
};

function StatusBadge({ run }: { run: AiRunView }) {
  if (run.status !== "failed") {
    return (
      <Badge variant={run.status === "completed" ? "secondary" : "outline"}>
        {STATUS_LABELS[run.status]}
      </Badge>
    );
  }
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            className="cursor-default rounded-4xl outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          />
        }
      >
        <Badge variant="destructive">{STATUS_LABELS.failed}</Badge>
      </TooltipTrigger>
      <TooltipContent side="left" className="max-w-sm break-words">
        {run.error ?? "Unknown error"}
      </TooltipContent>
    </Tooltip>
  );
}

function RunRow({
  run,
  providerNames,
}: {
  run: AiRunView;
  providerNames: Map<string, string>;
}) {
  const { usage } = run;
  const scheduled = run.trigger === "scheduled";
  const TriggerIcon = scheduled ? CalendarClock : MousePointerClick;
  const speed = outputTokensPerSecond(usage, run.provider === null);
  return (
    <TableRow>
      <TableCell>
        <div className="flex items-center gap-1.5">
          <span
            className="flex shrink-0 text-muted-foreground"
            title={scheduled ? "Scheduled run" : "Started by hand"}
          >
            <TriggerIcon
              className="size-3.5"
              aria-label={scheduled ? "Scheduled" : "Manual"}
            />
          </span>
          {format(new Date(run.started_at), "MMM d, HH:mm")}
        </div>
      </TableCell>
      <TableCell>
        <ModelName
          provider={run.provider}
          model={run.model}
          providerNames={providerNames}
        />
      </TableCell>
      <TableCell className={NUMERIC} title={`${run.labeled} labeled`}>
        {run.items}
      </TableCell>
      <TableCell
        className={NUMERIC}
        title={`${usage.input_tokens} in, ${usage.output_tokens} out`}
      >
        {formatTokens(usage.input_tokens)}
        <span className="text-muted-foreground"> / </span>
        {formatTokens(usage.output_tokens)}
      </TableCell>
      <TableCell className={NUMERIC}>
        {formatMs(usage.embed_ms + usage.llm_ms)}
      </TableCell>
      <TableCell
        className={NUMERIC}
        title={speedDetail(usage, run.provider === null)}
      >
        {formatRate(speed, "tok/s")}
      </TableCell>
      <TableCell className="text-right">
        <StatusBadge run={run} />
      </TableCell>
    </TableRow>
  );
}

function CostCell({
  usage: { provider, model, totals },
  price,
  providerNames,
}: {
  usage: ModelUsage;
  price: ModelPrice | undefined;
  providerNames: Map<string, string>;
}) {
  if (provider === null) {
    return <span className="text-muted-foreground">Free</span>;
  }
  const edit = () =>
    openDialog("model-price", {
      provider,
      providerName: providerNames.get(provider) ?? provider,
      model,
      price,
    });
  if (!price) {
    return (
      <Button
        type="button"
        variant="link"
        size="xs"
        className="h-auto p-0 text-xs"
        onClick={edit}
      >
        Set price
      </Button>
    );
  }
  const cost = estimateCost(totals, price);
  return (
    <span className="inline-flex items-center justify-end gap-1">
      <span title={formatCostExact(cost)}>{formatCost(cost)}</span>
      <button
        type="button"
        onClick={edit}
        aria-label={`Edit the price of ${model}`}
        className="rounded p-0.5 text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      >
        <Pencil className="size-3" />
      </button>
    </span>
  );
}

/** The cost of the priced remote models, and how many remote models have no price. */
function totalCost(models: ModelUsage[], prices: ModelPrice[]) {
  let cost: number | null = null;
  let unpriced = 0;
  for (const { provider, model, totals } of models) {
    if (provider === null) continue;
    const price = findPrice(prices, provider, model);
    if (price) cost = (cost ?? 0) + estimateCost(totals, price);
    else unpriced += 1;
  }
  return { cost, unpriced };
}

function SectionLabel({ children }: { children: string }) {
  return (
    <h3 className="m-0 text-xs font-medium text-muted-foreground">
      {children}
    </h3>
  );
}

export function AiUsageTab() {
  const [period, setPeriod] = useState<Period>("30");
  const days = PERIODS.find((p) => p.value === period)?.days ?? null;
  const summaryQuery = useQuery(aiUsageSummaryQueryOptions(days));
  const runsQuery = useQuery(aiUsageRunsQueryOptions(RECENT_RUNS));
  const ai = useQuery(settingsQueryOptions).data?.ai;
  const providers = ai?.providers;
  const prices = ai?.prices ?? [];
  const providerNames = new Map(
    (providers ?? []).map((provider) => [provider.id, provider.name]),
  );

  const summary = summaryQuery.data;
  const runs = runsQuery.data ?? [];
  const totals = summary?.totals;
  const { cost, unpriced } = totalCost(summary?.models ?? [], prices);
  const hasRemote = summary?.models.some((m) => m.provider !== null) ?? false;
  const hasPricedDay =
    summary?.daily.some((row) => findPrice(prices, row.provider, row.model)) ??
    false;
  const empty =
    summaryQuery.isSuccess &&
    runsQuery.isSuccess &&
    runs.length === 0 &&
    totals?.runs === 0;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="m-0 mr-auto text-sm text-muted-foreground">
          What AI categorization used, how fast it ran and what it cost
        </p>
        <div className="flex items-center gap-1 rounded-md border p-0.5">
          {PERIODS.map((p) => (
            <Button
              key={p.value}
              type="button"
              variant={period === p.value ? "secondary" : "ghost"}
              size="sm"
              onClick={() => setPeriod(p.value)}
            >
              {p.label}
            </Button>
          ))}
        </div>
      </div>
      {summaryQuery.isError && (
        <p className="m-0 text-xs text-destructive">
          {String(summaryQuery.error)}
        </p>
      )}
      {empty ? (
        <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
          No AI runs yet. Tokens, speed, cache use and cost show up here once
          activities have been categorized.
        </div>
      ) : (
        <>
          {totals && summary && (
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 @4xl/main:grid-cols-6">
              <Stat
                label="Estimated cost"
                value={hasRemote ? formatCost(cost) : "Free"}
                detail={
                  !hasRemote
                    ? "local model only"
                    : unpriced > 0
                      ? `${unpriced} ${unpriced === 1 ? "model has" : "models have"} no price`
                      : "remote models"
                }
              />
              <Stat
                label="Tokens in"
                value={formatTokens(totals.input_tokens)}
                detail={`${totals.runs} ${totals.runs === 1 ? "run" : "runs"}`}
              />
              <Stat
                label="Tokens out"
                value={formatTokens(totals.output_tokens)}
                detail={`${totals.requests} requests`}
              />
              <Stat
                label="Cache hit"
                value={formatPercent(cacheHitRatio(totals))}
                detail="of input tokens"
              />
              <Stat
                label="Output speed"
                value={formatRate(
                  overallOutputTokensPerSecond(summary.models),
                  "tok/s",
                )}
              />
              <Stat
                label="Throughput"
                value={formatRate(
                  itemsPerMinute(totals.items, totals.llm_ms),
                  "/min",
                )}
                detail={`${totals.items} activities`}
              />
            </div>
          )}

          {summary && summary.models.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <SectionLabel>By model</SectionLabel>
              <div className="rounded-md border">
                <Table className="text-xs">
                  <TableHeader>
                    <TableRow className="hover:bg-transparent">
                      <TableHead className="h-8">Model</TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>Runs</TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Items
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Tokens in / out
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Speed
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Avg latency
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Est. cost
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {summary.models.map((usage) => {
                      const { provider, model, totals } = usage;
                      const latency = averageLatencyMs(totals);
                      return (
                        <TableRow key={`${provider ?? ""}/${model}`}>
                          <TableCell>
                            <ModelName
                              provider={provider}
                              model={model}
                              providerNames={providerNames}
                            />
                          </TableCell>
                          <TableCell className={NUMERIC}>
                            {totals.runs}
                          </TableCell>
                          <TableCell className={NUMERIC}>
                            {totals.items}
                          </TableCell>
                          <TableCell className={NUMERIC}>
                            {formatTokens(totals.input_tokens)}
                            <span className="text-muted-foreground">
                              {" / "}
                            </span>
                            {formatTokens(totals.output_tokens)}
                          </TableCell>
                          <TableCell
                            className={NUMERIC}
                            title={speedDetail(totals, provider === null)}
                          >
                            {formatRate(
                              outputTokensPerSecond(totals, provider === null),
                              "tok/s",
                            )}
                          </TableCell>
                          <TableCell className={NUMERIC}>
                            {latency === null ? "—" : formatMs(latency)}
                          </TableCell>
                          <TableCell className={NUMERIC}>
                            <CostCell
                              usage={usage}
                              price={findPrice(prices, provider, model)}
                              providerNames={providerNames}
                            />
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>
            </div>
          )}

          {summary && hasPricedDay && (
            <div className="flex flex-col gap-1.5">
              <SectionLabel>Estimated cost per day</SectionLabel>
              <AiCostChart daily={summary.daily} prices={prices} days={days} />
            </div>
          )}

          {runs.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <SectionLabel>Recent runs</SectionLabel>
              <div className="rounded-md border">
                <Table className="text-xs">
                  <TableHeader>
                    <TableRow className="hover:bg-transparent">
                      <TableHead className="h-8">When</TableHead>
                      <TableHead className="h-8">Model</TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Items
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Tokens in / out
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Duration
                      </TableHead>
                      <TableHead className={cn("h-8", NUMERIC)}>
                        Speed
                      </TableHead>
                      <TableHead className="h-8 text-right">Status</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {runs.map((run) => (
                      <RunRow
                        key={run.id}
                        run={run}
                        providerNames={providerNames}
                      />
                    ))}
                  </TableBody>
                </Table>
              </div>
            </div>
          )}
          <p className="m-0 text-xs text-muted-foreground">
            Speed is output tokens per second, timed over decoding for the local
            model and over the whole request for a remote one. Costs are
            estimated from the prices you set per model, not billed amounts.
          </p>
        </>
      )}
    </div>
  );
}
