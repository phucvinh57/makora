import { type ReactNode, useMemo } from "react";
import { categoryPaths, orderedCategories } from "@/categories";
import { SelectItem } from "@/components/ui/select";
import type { Category } from "@/types";

const INDENT_PER_LEVEL_PX = 12;

function CategoryDot({ color }: { color: string }) {
  return (
    <span
      className="size-2 shrink-0 rounded-full"
      style={{ backgroundColor: color }}
    />
  );
}

/**
 * The dropdown list itself: the tree, indented, in reading order. Picking a parent means
 * the parent *and* everything under it wherever a category is used as a filter.
 */
export function categoryOptions(categories: Category[]): ReactNode[] {
  return orderedCategories(categories).map(({ category, depth }) => (
    <SelectItem key={category.id} value={String(category.id)}>
      <span
        className="flex min-w-0 items-center gap-1.5"
        style={{ paddingLeft: depth * INDENT_PER_LEVEL_PX }}
      >
        <CategoryDot color={category.color} />
        {category.name}
      </span>
    </SelectItem>
  ));
}

/**
 * What the closed trigger shows for each value. Indentation reads as damage there, so a
 * nested category names its whole path instead.
 */
export function categoryValueLabels(
  categories: Category[],
): Record<string, ReactNode> {
  const paths = categoryPaths(categories);
  return Object.fromEntries(
    categories.map((category) => [
      String(category.id),
      <span className="flex min-w-0 items-center gap-1.5" key={category.id}>
        <CategoryDot color={category.color} />
        {paths.get(category.id)}
      </span>,
    ]),
  );
}

/** Everything a category dropdown needs, built once per category list instead of once per dropdown. */
export interface CategoryPicker {
  categories: Category[];
  pathById: Map<number, string>;
  options: ReactNode[];
  valueLabels: Record<string, ReactNode>;
}

export function useCategoryPicker(categories: Category[]): CategoryPicker {
  return useMemo(
    () => ({
      categories,
      pathById: categoryPaths(categories),
      options: categoryOptions(categories),
      valueLabels: categoryValueLabels(categories),
    }),
    [categories],
  );
}
