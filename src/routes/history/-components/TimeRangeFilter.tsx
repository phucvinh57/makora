import { CalendarRange, Target, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
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

/** The history page's time scope: relative presets, an explicit custom
 * window, or a focus session's own window. `session` is the work session
 * `range` points at once it has loaded, used for the button's label. */
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
  const active = isRangeActive(range);

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
              {range.kind === "session" ? (
                <Target className="size-3.5 shrink-0" />
              ) : (
                <CalendarRange className="size-3.5 shrink-0" />
              )}
              <span className="truncate">{describeRange(range, session)}</span>
            </Button>
          }
        />
        <PopoverContent className="w-80" align="end">
          <Tabs
            defaultValue={range.kind === "session" ? "session" : "range"}
            className="flex flex-col gap-3"
          >
            <TabsList className="w-full">
              <TabsTrigger value="range" className="flex-1">
                Time range
              </TabsTrigger>
              <TabsTrigger value="session" className="flex-1">
                Focus session
              </TabsTrigger>
            </TabsList>

            <TabsContent value="range" className="flex flex-col gap-3">
              <div className="grid grid-cols-2 gap-1.5">
                {RELATIVE_PRESETS.map((preset) => (
                  <Button
                    key={preset.kind}
                    type="button"
                    size="sm"
                    variant={range.kind === preset.kind ? "secondary" : "ghost"}
                    className="justify-start"
                    onClick={() => apply({ kind: preset.kind })}
                  >
                    {preset.label}
                  </Button>
                ))}
              </div>
              <CustomRangeFields range={range} onApply={apply} />
            </TabsContent>

            <TabsContent value="session">
              <WorkSessionPicker
                selectedId={
                  range.kind === "session" ? range.sessionId : undefined
                }
                onSelect={(picked) =>
                  apply({ kind: "session", sessionId: picked.id })
                }
              />
            </TabsContent>
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

/** The two `datetime-local` bounds of a custom window. Held locally until
 * "Apply" so a half-typed date never fires a query; either bound may be left
 * empty for an open-ended window. */
function CustomRangeFields({
  range,
  onApply,
}: {
  range: TimeRange;
  onApply: (range: TimeRange) => void;
}) {
  const [from, setFrom] = useState(() => toLocalInputValue(range.from));
  const [to, setTo] = useState(() => toLocalInputValue(range.to));

  // Re-seed when the popover is reopened against a different range (e.g.
  // after picking a preset), so the fields show what is actually applied.
  useEffect(() => {
    setFrom(toLocalInputValue(range.from));
    setTo(toLocalInputValue(range.to));
  }, [range.from, range.to]);

  const invalid = from !== "" && to !== "" && from >= to;

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <p className="m-0 text-xs font-medium text-muted-foreground">Custom</p>
      <div className="flex flex-col gap-1.5">
        <Input
          type="datetime-local"
          aria-label="Range start"
          value={from}
          onChange={(e) => setFrom(e.target.value)}
          className="text-sm"
        />
        <Input
          type="datetime-local"
          aria-label="Range end"
          value={to}
          onChange={(e) => setTo(e.target.value)}
          className="text-sm"
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
