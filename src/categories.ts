import type { Category } from "./types";

export interface CategoryNode {
  category: Category;
  depth: number;
  children: CategoryNode[];
}

/** Assembles the flat list `list_categories` returns into the tree the UI shows. */
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

/** The tree in reading order, each node carrying the depth to indent it by. */
export function flattenCategoryTree(nodes: CategoryNode[]): CategoryNode[] {
  return nodes.flatMap((node) => [node, ...flattenCategoryTree(node.children)]);
}

/** Categories in tree order — what every category picker and dropdown lists. */
export function orderedCategories(categories: Category[]): CategoryNode[] {
  return flattenCategoryTree(buildCategoryTree(categories));
}

/** `"Work / Reading"` — how a nested category reads outside the tree itself. */
export function categoryPath(
  category: Category,
  categories: Category[],
): string {
  const byId = new Map(categories.map((c) => [c.id, c]));
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

/**
 * Only the categories the user made — drops built-ins like Distraction.
 *
 * Used where offering a built-in would be nonsense (nothing can nest under it; marking
 * "Distraction" on-task defeats the point), not as a blanket filter: it stays pickable
 * anywhere a person might legitimately file an activity there by hand.
 */
export function userCategories(categories: Category[]): Category[] {
  return categories.filter((category) => category.system_key === null);
}

/** A category plus everything nested under it — what picking it now covers. */
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
