import type { Category } from "./types";

/** The filter and select value for activity with no category. */
export const UNCATEGORIZED = "uncategorized";
export const UNCATEGORIZED_LABEL = "Uncategorized";
export const UNCATEGORIZED_COLOR = "#9ca3af";

export interface CategoryNode {
  category: Category;
  depth: number;
  children: CategoryNode[];
}

export function buildCategoryTree(categories: Category[]): CategoryNode[] {
  const nodes = new Map<number, CategoryNode>(
    categories.map((category) => [
      category.id,
      { category, depth: 0, children: [] },
    ]),
  );
  const roots: CategoryNode[] = [];
  for (const category of categories) {
    const node = nodes.get(category.id);
    if (!node) continue;
    const parent =
      category.parent_id !== null ? nodes.get(category.parent_id) : undefined;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }

  const seen = new Set<number>();
  const setDepth = (list: CategoryNode[], depth: number) => {
    for (const node of list) {
      seen.add(node.category.id);
      node.depth = depth;
      setDepth(node.children, depth + 1);
    }
  };
  setDepth(roots, 0);
  // A parent that went missing would otherwise take its children off the page entirely.
  for (const node of nodes.values()) {
    if (!seen.has(node.category.id)) {
      node.children = [];
      roots.push(node);
    }
  }
  return roots;
}

export function flattenCategoryTree(nodes: CategoryNode[]): CategoryNode[] {
  return nodes.flatMap((node) => [node, ...flattenCategoryTree(node.children)]);
}

export function orderedCategories(categories: Category[]): CategoryNode[] {
  return flattenCategoryTree(buildCategoryTree(categories));
}

function pathOf(category: Category, byId: Map<number, Category>): string {
  const names = [category.name];
  const seen = new Set([category.id]);
  let current = category;
  while (current.parent_id !== null) {
    const parent = byId.get(current.parent_id);
    if (!parent || seen.has(parent.id)) break;
    seen.add(parent.id);
    names.unshift(parent.name);
    current = parent;
  }
  return names.join(" / ");
}

export function categoryPath(
  category: Category,
  categories: Category[],
): string {
  return pathOf(category, new Map(categories.map((c) => [c.id, c])));
}

/** `categoryPath` for every category at once, for callers that would otherwise rebuild the lookup per row. */
export function categoryPaths(categories: Category[]): Map<number, string> {
  const byId = new Map(categories.map((c) => [c.id, c]));
  return new Map(categories.map((c) => [c.id, pathOf(c, byId)]));
}

export function categorySubtreeIds(
  rootId: number,
  categories: Category[],
): Set<number> {
  const ids = new Set([rootId]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const category of categories) {
      if (
        category.parent_id !== null &&
        ids.has(category.parent_id) &&
        !ids.has(category.id)
      ) {
        ids.add(category.id);
        grew = true;
      }
    }
  }
  return ids;
}
