import {
  addMonths,
  addYears,
  eachDayOfInterval,
  endOfMonth,
  endOfWeek,
  format,
  isSameDay,
  isSameMonth,
  isValid,
  parse,
  setHours,
  setMinutes,
  startOfMonth,
  startOfWeek,
} from "date-fns";
import { CalendarClock, ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";

const VALUE_FORMAT = "yyyy-MM-dd'T'HH:mm";

const HOURS = Array.from({ length: 24 }, (_, i) => i);
const MINUTES = Array.from({ length: 60 }, (_, i) => i);
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

function parseValue(value: string): Date | null {
  if (!value) return null;
  const parsed = parse(value, VALUE_FORMAT, new Date());
  return isValid(parsed) ? parsed : null;
}

function formatValue(date: Date) {
  return format(date, VALUE_FORMAT);
}

export function DateTimePicker({
  id,
  value,
  onValueChange,
  placeholder = "Pick a date and time",
  clearable = false,
  disabled,
  className,
  "aria-label": ariaLabel,
}: {
  id?: string;
  value: string;
  onValueChange: (value: string) => void;
  placeholder?: string;
  clearable?: boolean;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  const [open, setOpen] = useState(false);
  const selected = parseValue(value);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        id={id}
        disabled={disabled}
        aria-label={ariaLabel}
        className={cn(
          "flex h-8 w-full min-w-0 items-center gap-2 rounded-lg border border-input bg-transparent px-2.5 py-1 text-left text-sm font-normal transition-colors outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 dark:bg-input/30",
          !selected && "text-muted-foreground",
          className,
        )}
      >
        <CalendarClock className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">
          {selected ? format(selected, "MMM d, yyyy, HH:mm") : placeholder}
        </span>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="start">
        <DateTimePanel
          selected={selected}
          clearable={clearable}
          onSelect={(date) => onValueChange(formatValue(date))}
          onClear={() => {
            onValueChange("");
            setOpen(false);
          }}
          onDone={() => setOpen(false)}
        />
      </PopoverContent>
    </Popover>
  );
}

function DateTimePanel({
  selected,
  clearable,
  onSelect,
  onClear,
  onDone,
}: {
  selected: Date | null;
  clearable: boolean;
  onSelect: (date: Date) => void;
  onClear: () => void;
  onDone: () => void;
}) {
  const [month, setMonth] = useState(() =>
    startOfMonth(selected ?? new Date()),
  );

  const days = eachDayOfInterval({
    start: startOfWeek(startOfMonth(month)),
    end: endOfWeek(endOfMonth(month)),
  });

  const base = selected ?? new Date();

  return (
    <div className="flex flex-col">
      <div className="flex items-stretch">
        <div className="flex flex-col gap-2 p-3">
          <div className="flex items-center justify-between gap-2">
            <NavGroup
              label={format(month, "MMMM")}
              onPrev={() => setMonth(addMonths(month, -1))}
              onNext={() => setMonth(addMonths(month, 1))}
              prevLabel="Previous month"
              nextLabel="Next month"
            />
            <NavGroup
              label={format(month, "yyyy")}
              onPrev={() => setMonth(addYears(month, -1))}
              onNext={() => setMonth(addYears(month, 1))}
              prevLabel="Previous year"
              nextLabel="Next year"
            />
          </div>
          <div className="grid grid-cols-7 gap-0.5">
            {WEEKDAYS.map((day) => (
              <span
                key={day}
                className="flex h-7 w-8 items-center justify-center text-xs font-medium text-muted-foreground"
              >
                {day.slice(0, 2)}
              </span>
            ))}
            {days.map((day) => {
              const isSelected = selected !== null && isSameDay(day, selected);
              return (
                <button
                  key={day.toISOString()}
                  type="button"
                  aria-pressed={isSelected}
                  onClick={() =>
                    onSelect(
                      setMinutes(
                        setHours(day, base.getHours()),
                        base.getMinutes(),
                      ),
                    )
                  }
                  className={cn(
                    "flex h-7 w-8 items-center justify-center rounded-md text-sm tabular-nums transition-colors hover:bg-accent hover:text-accent-foreground",
                    !isSameMonth(day, month) && "text-muted-foreground/50",
                    isSameDay(day, new Date()) &&
                      !isSelected &&
                      "font-semibold text-primary",
                    isSelected &&
                      "bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground",
                  )}
                >
                  {day.getDate()}
                </button>
              );
            })}
          </div>
        </div>

        <div className="flex border-l">
          <TimeColumn
            label="Hour"
            values={HOURS}
            selected={selected?.getHours() ?? null}
            onSelect={(hour) => onSelect(setHours(base, hour))}
          />
          <TimeColumn
            label="Min"
            values={MINUTES}
            selected={selected?.getMinutes() ?? null}
            onSelect={(minute) => onSelect(setMinutes(base, minute))}
          />
        </div>
      </div>

      <div className="flex items-center justify-between gap-2 border-t p-2">
        <div className="flex gap-1">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onSelect(new Date())}
          >
            Now
          </Button>
          {clearable && (
            <Button type="button" variant="ghost" size="sm" onClick={onClear}>
              Clear
            </Button>
          )}
        </div>
        <Button type="button" size="sm" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}

function NavGroup({
  label,
  onPrev,
  onNext,
  prevLabel,
  nextLabel,
}: {
  label: string;
  onPrev: () => void;
  onNext: () => void;
  prevLabel: string;
  nextLabel: string;
}) {
  return (
    <div className="flex items-center gap-0.5">
      <NavButton label={prevLabel} onClick={onPrev}>
        <ChevronLeft className="size-4" />
      </NavButton>
      <span className="min-w-16 text-center text-sm font-medium">{label}</span>
      <NavButton label={nextLabel} onClick={onNext}>
        <ChevronRight className="size-4" />
      </NavButton>
    </div>
  );
}

function NavButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
    >
      {children}
    </button>
  );
}

function TimeColumn({
  label,
  values,
  selected,
  onSelect,
}: {
  label: string;
  values: number[];
  selected: number | null;
  onSelect: (value: number) => void;
}) {
  const selectedRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "center" });
  }, []);

  return (
    <div className="flex w-14 flex-col">
      <span className="border-b px-2 py-1.5 text-center text-xs font-medium text-muted-foreground">
        {label}
      </span>
      <div className="h-56 overflow-y-auto p-1">
        <div className="flex flex-col gap-0.5">
          {values.map((entry) => {
            const isSelected = entry === selected;
            return (
              <button
                key={entry}
                ref={isSelected ? selectedRef : undefined}
                type="button"
                aria-pressed={isSelected}
                onClick={() => onSelect(entry)}
                className={cn(
                  "rounded-md py-1 text-center text-sm tabular-nums transition-colors hover:bg-accent hover:text-accent-foreground",
                  isSelected &&
                    "bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground",
                )}
              >
                {String(entry).padStart(2, "0")}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
