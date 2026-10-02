import { Autocomplete } from "@base-ui/react/autocomplete";
import { Input } from "@/components/ui/input";

export interface Suggestion {
  /** What goes into the field when picked. */
  value: string;
  /** Muted text on the right of the row. */
  hint?: string;
  /** More text the typed query is matched against, besides `value`. */
  search?: string;
}

function matches(suggestion: Suggestion, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return `${suggestion.value} ${suggestion.search ?? ""}`
    .toLowerCase()
    .includes(needle);
}

/**
 * A text field that offers suggestions as you type but takes anything: the value is whatever
 * is in the box, whether or not it was picked from the list.
 */
export function SuggestInput({
  value,
  onValueChange,
  suggestions,
  placeholder,
  ariaLabel,
}: {
  value: string;
  onValueChange: (value: string) => void;
  suggestions: Suggestion[];
  placeholder?: string;
  ariaLabel: string;
}) {
  return (
    <Autocomplete.Root
      items={suggestions}
      value={value}
      onValueChange={onValueChange}
      itemToStringValue={(suggestion) => suggestion.value}
      filter={matches}
      openOnInputClick
    >
      <Autocomplete.Input
        render={<Input />}
        placeholder={placeholder}
        aria-label={ariaLabel}
        autoComplete="off"
        spellCheck={false}
      />
      <Autocomplete.Portal>
        <Autocomplete.Positioner sideOffset={4} className="isolate z-50">
          <Autocomplete.Popup className="w-(--anchor-width) max-w-(--available-width) overflow-hidden rounded-lg bg-popover text-popover-foreground shadow-md ring-1 ring-foreground/10 has-[[data-empty]]:hidden">
            <Autocomplete.List className="max-h-[min(18rem,var(--available-height))] overflow-y-auto overscroll-contain p-1 outline-0 data-empty:p-0">
              {(suggestion: Suggestion) => (
                <Autocomplete.Item
                  key={suggestion.value}
                  value={suggestion}
                  className="flex cursor-default items-center justify-between gap-3 rounded-md px-1.5 py-1 text-sm outline-hidden select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground"
                >
                  <span className="truncate">{suggestion.value}</span>
                  {suggestion.hint && (
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {suggestion.hint}
                    </span>
                  )}
                </Autocomplete.Item>
              )}
            </Autocomplete.List>
          </Autocomplete.Popup>
        </Autocomplete.Positioner>
      </Autocomplete.Portal>
    </Autocomplete.Root>
  );
}
