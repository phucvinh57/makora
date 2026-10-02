import { type ReactNode, useMemo } from "react";
import { categoryPaths, orderedCategories } from "@/categories";
import { SelectItem } from "@/components/ui/select";
import type { Category } from "@/types";

const INDENT_PER_LEVEL_PX = 12;

export function CategoryDot({ color }: { color: string }) {
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

/** One row of a searchable category list. `label` is what the search matches and the trigger shows. */
export interface CategoryChoice {
  value: string;
  label: string;
  /** The category's own name, for the indented list where `label` would repeat its parents. */
  name?: string;
  /** Absent for the non-category rows (“Uncategorized”, “No parent”…). */
  color?: string;
  depth?: number;
}

export function categoryChoices(categories: Category[]): CategoryChoice[] {
  const paths = categoryPaths(categories);
  return orderedCategories(categories).map(({ category, depth }) => ({
    value: String(category.id),
    label: paths.get(category.id) ?? category.name,
    name: category.name,
    color: category.color,
    depth,
  }));
}

/** Everything a category dropdown needs, built once per category list instead of once per dropdown. */
export interface CategoryPicker {
  categories: Category[];
  pathById: Map<number, string>;
  options: ReactNode[];
  valueLabels: Record<string, ReactNode>;
  choices: CategoryChoice[];
}

export function useCategoryPicker(categories: Category[]): CategoryPicker {
  return useMemo(
    () => ({
      categories,
      pathById: categoryPaths(categories),
      options: categoryOptions(categories),
      valueLabels: categoryValueLabels(categories),
      choices: categoryChoices(categories),
    }),
    [categories],
  );
}
