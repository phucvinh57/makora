import { categorySubtreeIds, orderedCategories } from "@/categories";
import { cn } from "@/lib/utils";
import type { Category } from "@/types";

const INDENT_PER_LEVEL_PX = 16;

export function CategoryPicker({
  categories: allCategories,
  selected,
  onChange,
}: {
  categories: Category[];
  selected: number[];
  onChange: (ids: number[]) => void;
}) {
  const categories = allCategories;

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
        No categories yet — create one on the Categories page first.
      </p>
    );
  }

  // Time in a subcategory of an on-task category is on-task too, so a category covered by
  // a picked ancestor is shown as already counted rather than as something still to pick.
  const coveredByAncestor = new Set<number>();
  for (const id of selected) {
    for (const descendant of categorySubtreeIds(id, categories)) {
      if (descendant !== id) coveredByAncestor.add(descendant);
    }
  }

  return (
    <div className="flex flex-col items-start gap-1.5">
      {orderedCategories(categories).map(({ category, depth }) => {
        const covered = coveredByAncestor.has(category.id);
        const active = selected.includes(category.id) || covered;
        return (
          <button
            key={category.id}
            type="button"
            aria-pressed={active}
            disabled={covered}
            title={covered ? "Already on-task through its parent" : undefined}
            onClick={() => toggle(category.id)}
            style={{ marginLeft: depth * INDENT_PER_LEVEL_PX }}
            className={cn(
              "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors",
              active
                ? "border-primary bg-primary/10 text-foreground"
                : "border-input text-muted-foreground hover:bg-accent hover:text-accent-foreground",
              covered && "opacity-60",
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
