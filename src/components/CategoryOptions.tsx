import { useMemo } from "react";
import { categoryPaths, orderedCategories } from "@/categories";
import type { Category } from "@/types";

export function CategoryDot({ color }: { color: string }) {
  return (
    <span
      className="size-2 shrink-0 rounded-full"
      style={{ backgroundColor: color }}
    />
  );
}

export interface CategoryChoice {
  value: string;
  label: string;
  /** The category's own name, for the indented list where `label` would repeat its parents. */
  name?: string;
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

export interface CategoryPicker {
  pathById: Map<number, string>;
  choices: CategoryChoice[];
}

export function useCategoryPicker(categories: Category[]): CategoryPicker {
  return useMemo(
    () => ({
      pathById: categoryPaths(categories),
      choices: categoryChoices(categories),
    }),
    [categories],
  );
}
