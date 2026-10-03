import { Combobox } from "@base-ui/react/combobox";
import { CheckIcon, ChevronDownIcon, PlusIcon, SearchIcon } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import {
  CategoryDot,
  type CategoryChoice,
  type CategoryPicker,
} from "@/components/CategoryOptions";
import { selectTriggerClassName } from "@/components/ui/select";
import { cn } from "@/lib/utils";

const INDENT_PER_LEVEL_PX = 12;
const NO_CHOICES: CategoryChoice[] = [];

/**
 * A category dropdown you can type into. `extraChoices` are the non-category rows that go
 * first (“Uncategorized”, “All categories”…) and should be a stable array. With `onCreate`
 * the list ends in a “New category” button, which closes the list and hands over to the caller.
 */
export function CategoryCombobox({
  picker,
  extraChoices = NO_CHOICES,
  value,
  onValueChange,
  placeholder = "Choose a category",
  ariaLabel = "Category",
  size = "default",
  className,
  disabled,
  title,
  onCreate,
  children,
}: {
  picker: CategoryPicker;
  extraChoices?: CategoryChoice[];
  /** The picked row's `value`; anything that matches no row shows the placeholder. */
  value: string | null;
  onValueChange: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  size?: "sm" | "default";
  className?: string;
  disabled?: boolean;
  title?: string;
  onCreate?: () => void;
  children?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const choices = useMemo(
    () => [...extraChoices, ...picker.choices],
    [extraChoices, picker.choices],
  );
  const selected = choices.find((choice) => choice.value === value) ?? null;

  return (
    <Combobox.Root
      items={choices}
      value={selected}
      onValueChange={(choice) => {
        if (choice) onValueChange(choice.value);
      }}
      isItemEqualToValue={(a, b) => a.value === b.value}
      itemToStringLabel={(choice) => choice.label}
      open={open}
      onOpenChange={setOpen}
      disabled={disabled}
      autoHighlight
    >
      <Combobox.Trigger
        data-size={size}
        data-placeholder={selected ? undefined : ""}
        aria-label={ariaLabel}
        title={title}
        className={cn(selectTriggerClassName, className)}
      >
        {children ?? (
          <span className="flex min-w-0 flex-1 items-center gap-1.5 text-left">
            {selected?.color && <CategoryDot color={selected.color} />}
            <span className="truncate">{selected?.label ?? placeholder}</span>
          </span>
        )}
        <ChevronDownIcon className="pointer-events-none size-4 text-muted-foreground" />
      </Combobox.Trigger>
      <Combobox.Portal>
        <Combobox.Positioner
          sideOffset={4}
          align="start"
          className="isolate z-50"
        >
          <Combobox.Popup className="w-(--anchor-width) min-w-56 max-w-(--available-width) origin-(--transform-origin) overflow-hidden rounded-lg bg-popover text-popover-foreground shadow-md ring-1 ring-foreground/10 duration-100 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95">
            <div className="flex items-center gap-1.5 border-b px-2.5">
              <SearchIcon className="size-3.5 shrink-0 text-muted-foreground" />
              <Combobox.Input
                placeholder="Search categories…"
                aria-label="Search categories"
                autoComplete="off"
                spellCheck={false}
                className="h-8 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
              />
            </div>
            <Combobox.Empty className="px-2 py-3 text-center text-sm text-muted-foreground empty:hidden">
              No category found.
            </Combobox.Empty>
            <Combobox.List className="max-h-[min(16rem,var(--available-height))] overflow-y-auto overscroll-contain p-1 outline-0 data-empty:p-0">
              {(choice: CategoryChoice) => (
                <Combobox.Item
                  key={choice.value}
                  value={choice}
                  className="relative flex cursor-default items-center gap-1.5 rounded-md py-1 pr-8 pl-1.5 text-sm outline-hidden select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground"
                >
                  <span
                    className="flex min-w-0 items-center gap-1.5"
                    style={{
                      paddingLeft: (choice.depth ?? 0) * INDENT_PER_LEVEL_PX,
                    }}
                  >
                    {choice.color && <CategoryDot color={choice.color} />}
                    <span className="truncate">
                      {choice.name ?? choice.label}
                    </span>
                  </span>
                  <Combobox.ItemIndicator className="pointer-events-none absolute right-2 flex size-4 items-center justify-center">
                    <CheckIcon className="size-4" />
                  </Combobox.ItemIndicator>
                </Combobox.Item>
              )}
            </Combobox.List>
            {onCreate && (
              <div className="border-t p-1">
                <button
                  type="button"
                  onClick={() => {
                    setOpen(false);
                    onCreate();
                  }}
                  className="flex w-full cursor-default items-center gap-1.5 rounded-md px-1.5 py-1 text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground"
                >
                  <PlusIcon className="size-4" />
                  New category
                </button>
              </div>
            )}
          </Combobox.Popup>
        </Combobox.Positioner>
      </Combobox.Portal>
    </Combobox.Root>
  );
}
