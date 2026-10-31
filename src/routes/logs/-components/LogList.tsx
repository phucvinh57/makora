import { format } from "date-fns";
import { ChevronRight } from "lucide-react";
import { memo, useCallback, useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { LogArea, LogEntry, LogLevel } from "@/types";
import { AREA_META, AREAS, type GroupBy } from "./constants";
import { formatDayLabel, formatFieldValue, keyFields } from "./format";

interface KeyedEntry {
  key: string;
  entry: LogEntry;
}

interface Group {
  id: string;
  label: string;
  rows: KeyedEntry[];
}

/** Stable across refetches, so expanded rows stay expanded as new entries arrive on top. */
function withKeys(entries: LogEntry[]): KeyedEntry[] {
  const seen = new Map<string, number>();
  return entries.map((entry) => {
    const base = `${entry.ts}|${entry.target}|${entry.message}|${JSON.stringify(entry.fields)}`;
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    return { key: `${base}|${n}`, entry };
  });
}

function groupRows(rows: KeyedEntry[], groupBy: GroupBy): Group[] {
  if (groupBy === "area") {
    const byArea = new Map<LogArea, KeyedEntry[]>();
    for (const row of rows) {
      const list = byArea.get(row.entry.area) ?? [];
      list.push(row);
      byArea.set(row.entry.area, list);
    }
    return AREAS.flatMap((area) => {
      const list = byArea.get(area);
      return list
        ? [{ id: area, label: AREA_META[area].label, rows: list }]
        : [];
    });
  }
  const groups: Group[] = [];
  for (const row of rows) {
    const date = new Date(row.entry.ts);
    const id = format(date, "yyyy-MM-dd");
    const last = groups.at(-1);
    if (last?.id === id) {
      last.rows.push(row);
    } else {
      groups.push({ id, label: formatDayLabel(date), rows: [row] });
    }
  }
  return groups;
}

export function LogList({
  entries,
  groupBy,
  emptyMessage,
}: {
  entries: LogEntry[];
  groupBy: GroupBy;
  emptyMessage: string;
}) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const groups = useMemo(
    () => groupRows(withKeys(entries), groupBy),
    [entries, groupBy],
  );

  const toggle = useCallback((key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  }, []);

  if (groups.length === 0) {
    return (
      <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
        {emptyMessage}
      </div>
    );
  }

  return (
    <div className="rounded-md border">
      {groups.map((group) => (
        <section key={group.id} className="border-t first:border-t-0">
          <h2 className="sticky top-0 z-10 m-0 flex items-baseline gap-2 border-b bg-muted/80 px-3 py-1.5 text-xs font-semibold backdrop-blur">
            {group.label}
            <span className="font-normal text-muted-foreground tabular-nums">
              {group.rows.length.toLocaleString()}
            </span>
          </h2>
          <ul className="m-0 list-none divide-y p-0">
            {group.rows.map(({ key, entry }) => (
              <LogRow
                key={key}
                rowKey={key}
                entry={entry}
                showDate={groupBy === "area"}
                expanded={expanded.has(key)}
                onToggle={toggle}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

const LEVEL_BADGE: Record<LogLevel, string> = {
  error: "bg-destructive/10 text-destructive dark:bg-destructive/20",
  warn: "bg-amber-500/15 text-amber-700 dark:text-amber-400",
  info: "bg-muted text-muted-foreground",
  debug: "bg-muted text-muted-foreground",
  trace: "bg-muted text-muted-foreground",
};

const LogRow = memo(function LogRow({
  rowKey,
  entry,
  showDate,
  expanded,
  onToggle,
}: {
  rowKey: string;
  entry: LogEntry;
  showDate: boolean;
  expanded: boolean;
  onToggle: (key: string) => void;
}) {
  const date = new Date(entry.ts);
  const { label: areaLabel, icon: AreaIcon } = AREA_META[entry.area];
  const inline = keyFields(entry.fields);

  return (
    <li>
      <button
        type="button"
        onClick={() => onToggle(rowKey)}
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-muted/50"
      >
        <ChevronRight
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform",
            expanded && "rotate-90",
          )}
        />
        <time
          dateTime={entry.ts}
          className="shrink-0 font-mono text-xs text-muted-foreground tabular-nums"
        >
          {format(date, showDate ? "MMM d HH:mm:ss" : "HH:mm:ss")}
        </time>
        <Badge
          className={cn(
            "h-4.5 w-12 rounded-sm px-1 text-[10px] uppercase",
            LEVEL_BADGE[entry.level],
          )}
        >
          {entry.level}
        </Badge>
        <Badge
          variant="outline"
          className="h-4.5 w-22 justify-start rounded-sm px-1.5 text-[11px] text-muted-foreground"
        >
          <AreaIcon />
          {areaLabel}
        </Badge>
        <span className="min-w-0 truncate font-medium">{entry.message}</span>
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
          {inline.map(([field, value]) => (
            <span key={field} className="mr-3">
              <span className="opacity-60">{field}=</span>
              {formatFieldValue(field, value)}
            </span>
          ))}
        </span>
      </button>
      {expanded && (
        <dl className="m-0 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 bg-muted/30 py-2 pr-3 pl-9 font-mono text-xs">
          <dt className="text-muted-foreground">time</dt>
          <dd className="m-0">{date.toLocaleString()}</dd>
          <dt className="text-muted-foreground">target</dt>
          <dd className="m-0 break-all">{entry.target}</dd>
          {Object.entries(entry.fields).map(([field, value]) => (
            <FieldRow key={field} field={field} value={value} />
          ))}
        </dl>
      )}
    </li>
  );
});

function FieldRow({
  field,
  value,
}: {
  field: string;
  value: LogEntry["fields"][string];
}) {
  const formatted = formatFieldValue(field, value);
  const raw = String(value);
  return (
    <>
      <dt className="text-muted-foreground">{field}</dt>
      <dd className="m-0 break-all whitespace-pre-wrap">
        {formatted}
        {formatted !== raw && typeof value === "number" && (
          <span className="ml-2 text-muted-foreground">({raw})</span>
        )}
      </dd>
    </>
  );
}
