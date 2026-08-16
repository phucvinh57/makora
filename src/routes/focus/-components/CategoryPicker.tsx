import { cn } from "@/lib/utils";
import type { Category } from "@/types";

export function CategoryPicker({
  categories,
  selected,
  onChange,
}: {
  categories: Category[];
  selected: number[];
  onChange: (ids: number[]) => void;
}) {
  function toggle(id: number) {
    onChange(
      selected.includes(id)
        ? selected.filter((s) => s !== id)
        : [...selected, id],
    );
  }

  if (categories.length === 0) {
    return (
      <p className="m-0 text-sm text-muted-foreground">
        No categories yet — create one on the Labelling page first.
      </p>
    );
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      {categories.map((category) => {
        const active = selected.includes(category.id);
        return (
          <button
            key={category.id}
            type="button"
            aria-pressed={active}
            onClick={() => toggle(category.id)}
            className={cn(
              "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors",
              active
                ? "border-primary bg-primary/10 text-foreground"
                : "border-input text-muted-foreground hover:bg-accent hover:text-accent-foreground",
            )}
          >
            <span
              className="size-2 shrink-0 rounded-full"
              style={{ backgroundColor: category.color }}
            />
            {category.name}
          </button>
        );
      })}
    </div>
  );
}
