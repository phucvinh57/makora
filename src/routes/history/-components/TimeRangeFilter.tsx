import { CalendarRange, Target, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { DateTimePicker } from "@/components/ui/date-time-picker";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import { useFeatureFlags } from "@/stores/features";
import type { WorkSession } from "@/types";
import {
  ALL_TIME,
  describeRange,
  fromLocalInputValue,
  isRangeActive,
  RELATIVE_PRESETS,
  type TimeRange,
  toLocalInputValue,
} from "./timeRange";
import { WorkSessionPicker } from "./WorkSessionPicker";

export function TimeRangeFilter({
  range,
  session,
  onChange,
}: {
  range: TimeRange;
  session?: WorkSession | null;
  onChange: (range: TimeRange) => void;
}) {
  const [open, setOpen] = useState(false);
  const focusEnabled = useFeatureFlags((state) => state.flags.focus);
  const displayRange =
    !focusEnabled && range.kind === "session" ? ALL_TIME : range;
  const active = isRangeActive(displayRange);

  function apply(next: TimeRange) {
    onChange(next);
    setOpen(false);
  }

  return (
    <div className="flex items-center">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          render={
            <Button
              variant="outline"
              size="sm"
              className={cn(
                "max-w-56 gap-1.5",
                active && "border-primary/40 text-primary",
                active && "rounded-r-none border-r-0",
              )}
            >
              {displayRange.kind === "session" ? (
                <Target className="size-3.5 shrink-0" />
              ) : (
                <CalendarRange className="size-3.5 shrink-0" />
              )}
              <span className="truncate">
                {describeRange(displayRange, session)}
              </span>
            </Button>
          }
        />
        <PopoverContent className="w-80" align="end">
          <Tabs
            defaultValue={displayRange.kind === "session" ? "session" : "range"}
            className="flex flex-col gap-3"
          >
            <TabsList className="w-full">
              <TabsTrigger value="range" className="flex-1">
                Time range
              </TabsTrigger>
              {focusEnabled ? (
                <TabsTrigger value="session" className="flex-1">
                  Focus session
                </TabsTrigger>
              ) : null}
            </TabsList>

            <TabsContent value="range" className="flex flex-col gap-3">
              <div className="grid grid-cols-2 gap-1.5">
                {RELATIVE_PRESETS.map((preset) => (
                  <Button
                    key={preset.kind}
                    type="button"
                    size="sm"
                    variant={
                      displayRange.kind === preset.kind ? "secondary" : "ghost"
                    }
                    className="justify-start"
                    onClick={() => apply({ kind: preset.kind })}
                  >
                    {preset.label}
                  </Button>
                ))}
              </div>
              <CustomRangeFields range={range} onApply={apply} />
            </TabsContent>

            {focusEnabled ? (
              <TabsContent value="session">
                <WorkSessionPicker
                  selectedId={
                    displayRange.kind === "session"
                      ? displayRange.sessionId
                      : undefined
                  }
                  onSelect={(picked) =>
                    apply({ kind: "session", sessionId: picked.id })
                  }
                />
              </TabsContent>
            ) : null}
          </Tabs>
        </PopoverContent>
      </Popover>

      {active && (
        <Button
          variant="outline"
          size="sm"
          aria-label="Clear time filter"
          className="rounded-l-none border-primary/40 px-2 text-primary"
          onClick={() => onChange(ALL_TIME)}
        >
          <X className="size-3.5" />
        </Button>
      )}
    </div>
  );
}

function CustomRangeFields({
  range,
  onApply,
}: {
  range: TimeRange;
  onApply: (range: TimeRange) => void;
}) {
  const [from, setFrom] = useState(() => toLocalInputValue(range.from));
  const [to, setTo] = useState(() => toLocalInputValue(range.to));

  useEffect(() => {
    setFrom(toLocalInputValue(range.from));
    setTo(toLocalInputValue(range.to));
  }, [range.from, range.to]);

  const invalid = from !== "" && to !== "" && from >= to;

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <p className="m-0 text-xs font-medium text-muted-foreground">Custom</p>
      <div className="flex flex-col gap-1.5">
        <DateTimePicker
          aria-label="Range start"
          placeholder="From (any time)"
          clearable
          value={from}
          onValueChange={setFrom}
        />
        <DateTimePicker
          aria-label="Range end"
          placeholder="To (any time)"
          clearable
          value={to}
          onValueChange={setTo}
        />
      </div>
      {invalid && (
        <p className="m-0 text-xs text-destructive">
          The end must come after the start.
        </p>
      )}
      <Button
        type="button"
        size="sm"
        disabled={invalid || (from === "" && to === "")}
        onClick={() =>
          onApply({
            kind: "custom",
            from: fromLocalInputValue(from),
            to: fromLocalInputValue(to),
          })
        }
      >
        Apply custom range
      </Button>
    </div>
  );
}
