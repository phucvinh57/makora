import { format } from "date-fns";
import { formatSeconds } from "@/duration";
import type { TimelineBlock, WorkSessionStats } from "@/types";
import { OFF_TASK_COLOR, ON_TASK_COLOR } from "./constants";

function blockSeconds(block: TimelineBlock) {
  return Math.max(
    0,
    (new Date(block.end_time).getTime() -
      new Date(block.start_time).getTime()) /
      1000,
  );
}

function blockTitle(block: TimelineBlock) {
  const range = `${format(new Date(block.start_time), "HH:mm")}–${format(
    new Date(block.end_time),
    "HH:mm",
  )}`;
  const label = block.kind === "idle" ? "Idle (nothing tracked)" : block.label;
  const verdict =
    block.kind === "on_task"
      ? "on task"
      : block.kind === "off_task"
        ? "off task"
        : "";
  return `${label}${verdict ? ` · ${verdict}` : ""} · ${range} · ${formatSeconds(blockSeconds(block))}`;
}

/** Chronological strip of the session window: what was focused when, colored
 * by verdict (identity is in the tooltip, not the color). Idle stretches
 * stay uncolored so tracked time visually dominates. */
export function TimelineStrip({ stats }: { stats: WorkSessionStats }) {
  const blocks = stats.timeline;

  if (blocks.length === 0) {
    return (
      <p className="m-0 py-4 text-center text-sm text-muted-foreground">
        Nothing was tracked during this session.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex h-7 w-full gap-0.5 overflow-hidden rounded-md">
        {blocks.map((block) => (
          <div
            key={block.start_time}
            title={blockTitle(block)}
            className="min-w-px rounded-[2px]"
            style={{
              flexGrow: blockSeconds(block),
              flexBasis: 0,
              backgroundColor:
                block.kind === "on_task"
                  ? ON_TASK_COLOR
                  : block.kind === "off_task"
                    ? OFF_TASK_COLOR
                    : "var(--muted)",
            }}
          />
        ))}
      </div>
      <div className="flex justify-between text-xs tabular-nums text-muted-foreground">
        <span>{format(new Date(stats.window_start), "HH:mm")}</span>
        <span>{format(new Date(stats.window_end), "HH:mm")}</span>
      </div>
    </div>
  );
}
