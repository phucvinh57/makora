use std::collections::{HashMap, HashSet};

use makora_lib::domain::types::Category;
use makora_lib::shared::ids::CategoryId;

/// The filter and select value for activity with no category.
pub const UNCATEGORIZED: &str = "uncategorized";
pub const UNCATEGORIZED_LABEL: &str = "Uncategorized";
pub const UNCATEGORIZED_COLOR: &str = "#9ca3af";
/// The color a new category starts with.
pub const DEFAULT_CATEGORY_COLOR: &str = "#6366f1";

#[derive(Debug, Clone)]
pub struct CategoryNode {
    pub category: Category,
    pub depth: usize,
    pub children: Vec<CategoryNode>,
}

/// A category in tree order, with how deep it sits.
#[derive(Debug, Clone, Copy)]
pub struct OrderedCategory<'a> {
    pub category: &'a Category,
    pub depth: usize,
}

struct Forest {
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
}

/// Indices into `categories`, the first of each id only. A category whose parent is missing, or
/// whose ancestry loops, becomes a root without children so it still shows.
fn forest(categories: &[Category]) -> Forest {
    let mut index = HashMap::with_capacity(categories.len());
    for (i, category) in categories.iter().enumerate() {
        index.entry(category.id).or_insert(i);
    }
    let unique = |i: &usize| index[&categories[*i].id] == *i;
    let mut roots = Vec::new();
    let mut children = vec![Vec::new(); categories.len()];
    for i in (0..categories.len()).filter(unique) {
        match categories[i]
            .parent_id
            .and_then(|parent| index.get(&parent))
        {
            Some(&parent) => children[parent].push(i),
            None => roots.push(i),
        }
    }
    let mut seen = vec![false; categories.len()];
    let mut stack = roots.clone();
    while let Some(i) = stack.pop() {
        seen[i] = true;
        stack.extend(&children[i]);
    }
    for i in (0..categories.len()).filter(unique) {
        if !seen[i] {
            children[i].clear();
            roots.push(i);
        }
    }
    Forest { roots, children }
}

pub fn build_category_tree(categories: &[Category]) -> Vec<CategoryNode> {
    fn build(i: usize, depth: usize, categories: &[Category], forest: &Forest) -> CategoryNode {
        CategoryNode {
            category: categories[i].clone(),
            depth,
            children: forest.children[i]
                .iter()
                .map(|&child| build(child, depth + 1, categories, forest))
                .collect(),
        }
    }
    let forest = forest(categories);
    forest
        .roots
        .iter()
        .map(|&root| build(root, 0, categories, &forest))
        .collect()
}

/// Every node, each followed by its descendants.
pub fn flatten_category_tree(nodes: &[CategoryNode]) -> Vec<&CategoryNode> {
    fn walk<'a>(nodes: &'a [CategoryNode], out: &mut Vec<&'a CategoryNode>) {
        for node in nodes {
            out.push(node);
            walk(&node.children, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out
}

/// The categories in tree order, as a list or select shows them.
pub fn ordered_categories(categories: &[Category]) -> Vec<OrderedCategory<'_>> {
    fn walk<'a>(
        i: usize,
        depth: usize,
        categories: &'a [Category],
        forest: &Forest,
        out: &mut Vec<OrderedCategory<'a>>,
    ) {
        out.push(OrderedCategory {
            category: &categories[i],
            depth,
        });
        for &child in &forest.children[i] {
            walk(child, depth + 1, categories, forest, out);
        }
    }
    let forest = forest(categories);
    let mut out = Vec::with_capacity(categories.len());
    for &root in &forest.roots {
        walk(root, 0, categories, &forest, &mut out);
    }
    out
}

fn path_of(category: &Category, by_id: &HashMap<CategoryId, &Category>) -> String {
    let mut names = vec![category.name.as_str()];
    let mut seen = HashSet::from([category.id]);
    let mut current = category;
    while let Some(parent) = current.parent_id.and_then(|id| by_id.get(&id)) {
        if !seen.insert(parent.id) {
            break;
        }
        names.push(&parent.name);
        current = parent;
    }
    names.reverse();
    names.join(" / ")
}

/// Every category's `Parent / Child` path, built from one lookup.
pub fn category_paths(categories: &[Category]) -> HashMap<CategoryId, String> {
    let by_id: HashMap<CategoryId, &Category> = categories.iter().map(|c| (c.id, c)).collect();
    categories
        .iter()
        .map(|c| (c.id, path_of(c, &by_id)))
        .collect()
}

/// `root_id` and every category below it.
pub fn category_subtree_ids(root_id: CategoryId, categories: &[Category]) -> HashSet<CategoryId> {
    let mut children: HashMap<CategoryId, Vec<CategoryId>> = HashMap::new();
    for category in categories {
        if let Some(parent) = category.parent_id {
            children.entry(parent).or_default().push(category.id);
        }
    }
    let mut ids = HashSet::from([root_id]);
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        for &child in children.get(&id).into_iter().flatten() {
            if ids.insert(child) {
                stack.push(child);
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(id: i64, name: &str, parent: Option<i64>) -> Category {
        Category {
            id: CategoryId(id),
            name: name.into(),
            color: DEFAULT_CATEGORY_COLOR.into(),
            parent_id: parent.map(CategoryId),
            description: None,
        }
    }

    fn sample() -> Vec<Category> {
        vec![
            cat(3, "Coding", Some(1)),
            cat(1, "Work", None),
            cat(2, "Fun", None),
            cat(4, "Rust", Some(3)),
            cat(5, "Meetings", Some(1)),
        ]
    }

    fn ordered(categories: &[Category]) -> Vec<(i64, usize)> {
        ordered_categories(categories)
            .iter()
            .map(|o| (o.category.id.0, o.depth))
            .collect()
    }

    #[test]
    fn tree_nests_children_under_parents() {
        let tree = build_category_tree(&sample());
        let roots: Vec<i64> = tree.iter().map(|n| n.category.id.0).collect();
        assert_eq!(roots, vec![1, 2]);
        let work = &tree[0];
        assert_eq!(work.depth, 0);
        let kids: Vec<i64> = work.children.iter().map(|n| n.category.id.0).collect();
        assert_eq!(kids, vec![3, 5]);
        assert_eq!(work.children[0].depth, 1);
        assert_eq!(work.children[0].children[0].category.id.0, 4);
        assert_eq!(work.children[0].children[0].depth, 2);
        assert!(tree[1].children.is_empty());
    }

    #[test]
    fn flatten_is_preorder() {
        let tree = build_category_tree(&sample());
        let flat: Vec<(i64, usize)> = flatten_category_tree(&tree)
            .iter()
            .map(|n| (n.category.id.0, n.depth))
            .collect();
        assert_eq!(flat, vec![(1, 0), (3, 1), (4, 2), (5, 1), (2, 0)]);
    }

    #[test]
    fn ordered_matches_flattened_tree() {
        assert_eq!(
            ordered(&sample()),
            vec![(1, 0), (3, 1), (4, 2), (5, 1), (2, 0)]
        );
        assert!(ordered(&[]).is_empty());
    }

    #[test]
    fn missing_parent_makes_a_root() {
        let categories = vec![cat(1, "Orphan", Some(99)), cat(2, "Child", Some(1))];
        assert_eq!(ordered(&categories), vec![(1, 0), (2, 1)]);
    }

    #[test]
    fn cycles_still_show_every_category() {
        let categories = vec![
            cat(1, "A", Some(2)),
            cat(2, "B", Some(1)),
            cat(3, "C", Some(1)),
            cat(4, "Self", Some(4)),
            cat(5, "Root", None),
        ];
        assert_eq!(
            ordered(&categories),
            vec![(5, 0), (1, 0), (2, 0), (3, 0), (4, 0)]
        );
        let tree = build_category_tree(&categories);
        assert_eq!(tree.len(), 5);
        assert!(tree.iter().all(|node| node.children.is_empty()));
    }

    #[test]
    fn duplicate_ids_show_once() {
        let categories = vec![
            cat(1, "A", None),
            cat(2, "B", Some(1)),
            cat(1, "A again", Some(2)),
        ];
        assert_eq!(ordered(&categories), vec![(1, 0), (2, 1)]);
    }

    #[test]
    fn paths_join_ancestors() {
        let paths = category_paths(&sample());
        assert_eq!(paths[&CategoryId(1)], "Work");
        assert_eq!(paths[&CategoryId(3)], "Work / Coding");
        assert_eq!(paths[&CategoryId(4)], "Work / Coding / Rust");
        assert_eq!(paths[&CategoryId(2)], "Fun");
    }

    #[test]
    fn paths_stop_at_missing_parents_and_cycles() {
        let categories = vec![
            cat(1, "A", Some(2)),
            cat(2, "B", Some(1)),
            cat(3, "C", Some(99)),
        ];
        let paths = category_paths(&categories);
        assert_eq!(paths[&CategoryId(1)], "B / A");
        assert_eq!(paths[&CategoryId(2)], "A / B");
        assert_eq!(paths[&CategoryId(3)], "C");
    }

    #[test]
    fn subtree_collects_descendants() {
        let ids = |root| {
            let mut ids: Vec<i64> = category_subtree_ids(CategoryId(root), &sample())
                .into_iter()
                .map(|id| id.0)
                .collect();
            ids.sort();
            ids
        };
        assert_eq!(ids(1), vec![1, 3, 4, 5]);
        assert_eq!(ids(3), vec![3, 4]);
        assert_eq!(ids(2), vec![2]);
        assert_eq!(ids(42), vec![42]);
    }

    #[test]
    fn subtree_handles_children_listed_before_parents() {
        let categories = vec![
            cat(4, "Rust", Some(3)),
            cat(3, "Coding", Some(1)),
            cat(1, "Work", None),
        ];
        let ids = category_subtree_ids(CategoryId(1), &categories);
        assert_eq!(ids.len(), 3);
        let cycle = vec![cat(1, "A", Some(2)), cat(2, "B", Some(1))];
        assert_eq!(category_subtree_ids(CategoryId(1), &cycle).len(), 2);
    }

    #[test]
    fn constants() {
        assert_eq!(UNCATEGORIZED, "uncategorized");
        assert_eq!(UNCATEGORIZED_LABEL, "Uncategorized");
        assert_eq!(UNCATEGORIZED_COLOR, "#9ca3af");
        assert_eq!(DEFAULT_CATEGORY_COLOR, "#6366f1");
    }
}
