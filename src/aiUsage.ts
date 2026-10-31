import { formatSeconds } from "./duration";
import type { ModelUsage, RunUsage } from "./types";

type Timings = Pick<
  RunUsage,
  "output_tokens" | "busy_ms" | "decode_ms" | "requests"
>;

export function formatTokens(count: number): string {
  if (count >= 1_000_000) return `${trim(count / 1_000_000)}M`;
  if (count >= 1_000) return `${trim(count / 1_000)}k`;
  return String(count);
}

function trim(value: number): string {
  return value >= 100 ? value.toFixed(0) : value.toFixed(1).replace(/\.0$/, "");
}

export function formatMs(ms: number): string {
  if (ms < 1_000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(1)} s`;
  return formatSeconds(ms / 1_000);
}

export function formatRate(value: number | null, unit: string): string {
  if (value === null) return "—";
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`;
}

export function formatPercent(ratio: number | null): string {
  return ratio === null ? "—" : `${Math.round(ratio * 100)}%`;
}

/**
 * The time output speed is measured over: decoding alone for the local model, the whole request
 * for a remote one, whose generation time is not reported apart.
 */
function generationMs(usage: Timings, local: boolean): number {
  return local ? usage.decode_ms : usage.busy_ms;
}

export function outputTokensPerSecond(
  usage: Timings,
  local: boolean,
): number | null {
  const ms = generationMs(usage, local);
  return ms > 0 && usage.output_tokens > 0
    ? (usage.output_tokens * 1_000) / ms
    : null;
}

/** Output speed across models, each timed its own way. */
export function overallOutputTokensPerSecond(
  models: ModelUsage[],
): number | null {
  let tokens = 0;
  let ms = 0;
  for (const { provider, totals } of models) {
    tokens += totals.output_tokens;
    ms += generationMs(totals, provider === null);
  }
  return ms > 0 && tokens > 0 ? (tokens * 1_000) / ms : null;
}

/** Local prefill speed: the prompt tokens that were not already in the KV cache. */
export function promptTokensPerSecond(
  usage: Pick<RunUsage, "input_tokens" | "cached_input_tokens" | "prefill_ms">,
): number | null {
  const fresh = usage.input_tokens - usage.cached_input_tokens;
  return usage.prefill_ms > 0 && fresh > 0
    ? (fresh * 1_000) / usage.prefill_ms
    : null;
}

export function cacheHitRatio(
  usage: Pick<RunUsage, "input_tokens" | "cached_input_tokens">,
): number | null {
  return usage.input_tokens > 0
    ? usage.cached_input_tokens / usage.input_tokens
    : null;
}

export function itemsPerMinute(items: number, llmMs: number): number | null {
  return llmMs > 0 && items > 0 ? (items * 60_000) / llmMs : null;
}

export function averageLatencyMs(usage: Timings): number | null {
  return usage.requests > 0 ? usage.busy_ms / usage.requests : null;
}

/** "12.3k tokens in 45.0 s", for a finished run's toast. */
export function describeRunUsage(usage: RunUsage): string | null {
  const tokens = usage.input_tokens + usage.output_tokens;
  const ms = usage.embed_ms + usage.llm_ms;
  if (tokens === 0 && ms === 0) return null;
  return `${formatTokens(tokens)} tokens in ${formatMs(ms)}.`;
}

/** A title for a speed cell: the local model's prefill speed, which a remote one does not report. */
export function speedDetail(
  usage: Pick<RunUsage, "input_tokens" | "cached_input_tokens" | "prefill_ms">,
  local: boolean,
): string | undefined {
  if (!local) return undefined;
  const prompt = promptTokensPerSecond(usage);
  return prompt === null ? undefined : `Prompt: ${formatRate(prompt, "tok/s")}`;
}
