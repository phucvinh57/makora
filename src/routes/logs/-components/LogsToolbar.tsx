import { Search } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn, parseOneOf } from "@/lib/utils";
import type { LogArea, LogPage } from "@/types";
import {
  AREA_META,
  AREAS,
  type GroupBy,
  MIN_LEVEL_LABELS,
  type MinLevel,
  type Period,
  PERIODS,
} from "./constants";

const MIN_LEVELS = Object.keys(MIN_LEVEL_LABELS) as MinLevel[];
const GROUPS: readonly GroupBy[] = ["day", "area"];

export function LogsToolbar({
  searchInput,
  onSearchInputChange,
  period,
  onPeriodChange,
  minLevel,
  onMinLevelChange,
  groupBy,
  onGroupByChange,
  areas,
  onAreasChange,
  areaCounts,
}: {
  searchInput: string;
  onSearchInputChange: (value: string) => void;
  period: Period;
  onPeriodChange: (period: Period) => void;
  minLevel: MinLevel;
  onMinLevelChange: (level: MinLevel) => void;
  groupBy: GroupBy;
  onGroupByChange: (groupBy: GroupBy) => void;
  areas: LogArea[];
  onAreasChange: (areas: LogArea[]) => void;
  areaCounts: LogPage["area_counts"] | undefined;
}) {
  const total = AREAS.reduce((sum, area) => sum + (areaCounts?.[area] ?? 0), 0);

  function toggleArea(area: LogArea) {
    onAreasChange(
      areas.includes(area) ? areas.filter((a) => a !== area) : [...areas, area],
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-40 flex-1 basis-48 sm:max-w-72">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="text"
            value={searchInput}
            onChange={(e) => onSearchInputChange(e.target.value)}
            placeholder="Search messages and fields…"
            className="h-8 pl-8"
            aria-label="Search logs"
          />
        </div>
        <div className="flex items-center gap-1 rounded-md border p-0.5">
          {PERIODS.map((preset) => (
            <Button
              key={preset.value}
              type="button"
              variant={period === preset.value ? "secondary" : "ghost"}
              size="sm"
              onClick={() => onPeriodChange(preset.value)}
            >
              {preset.label}
            </Button>
          ))}
        </div>
        <Select
          value={minLevel}
          onValueChange={(value) => {
            const level = parseOneOf(MIN_LEVELS, value);
            if (level) onMinLevelChange(level);
          }}
          items={MIN_LEVEL_LABELS}
        >
          <SelectTrigger size="sm" className="w-40" aria-label="Minimum level">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {MIN_LEVELS.map((level) => (
              <SelectItem key={level} value={level}>
                {MIN_LEVEL_LABELS[level]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Tabs
          value={groupBy}
          onValueChange={(value) => {
            const next = parseOneOf(GROUPS, value);
            if (next) onGroupByChange(next);
          }}
          className="ml-auto"
        >
          <TabsList aria-label="Group by">
            <TabsTrigger value="day">By day</TabsTrigger>
            <TabsTrigger value="area">By area</TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <AreaChip
          label="All"
          count={total}
          active={areas.length === 0}
          onClick={() => onAreasChange([])}
        />
        {AREAS.map((area) => {
          const { label, icon: Icon } = AREA_META[area];
          return (
            <AreaChip
              key={area}
              label={label}
              icon={<Icon className="size-3" />}
              count={areaCounts?.[area] ?? 0}
              active={areas.includes(area)}
              onClick={() => toggleArea(area)}
            />
          );
        })}
      </div>
    </div>
  );
}

function AreaChip({
  label,
  icon,
  count,
  active,
  onClick,
}: {
  label: string;
  icon?: ReactNode;
  count: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition-colors",
        active
          ? "border-primary/40 bg-primary/10 text-primary"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
        count === 0 && !active && "opacity-60",
      )}
    >
      {icon}
      {label}
      <span className="tabular-nums opacity-70">{count.toLocaleString()}</span>
    </button>
  );
}
