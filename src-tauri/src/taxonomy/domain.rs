use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

/// The shape of the category tree, built from every category's `(id, parent_id)`.
#[derive(Debug, Clone)]
pub(crate) struct CategoryTree {
    ids: HashSet<i64>,
    children: HashMap<i64, Vec<i64>>,
}

impl CategoryTree {
    pub(crate) fn new<I>(categories: I) -> Self
    where
        I: IntoIterator<Item = (i64, Option<i64>)>,
    {
        let mut ids = HashSet::new();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for (id, parent_id) in categories {
            ids.insert(id);
            if let Some(parent_id) = parent_id {
                children.entry(parent_id).or_default().push(id);
            }
        }
        for child_ids in children.values_mut() {
            child_ids.sort_unstable();
        }
        Self { ids, children }
    }

    pub(crate) fn contains(&self, id: i64) -> bool {
        self.ids.contains(&id)
    }

    pub(crate) fn descendants(&self, root: i64) -> Vec<i64> {
        if !self.contains(root) {
            return Vec::new();
        }
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            result.push(id);
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev().copied());
            }
        }
        result
    }

    pub(crate) fn deletion_order(&self, root: i64) -> Vec<i64> {
        let descendants = self.descendants(root);
        let descendant_set: HashSet<_> = descendants.iter().copied().collect();
        let mut depth = HashMap::new();
        depth.insert(root, 0usize);
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let next_depth = depth.get(&id).copied().unwrap_or(0) + 1;
            if let Some(children) = self.children.get(&id) {
                for child in children {
                    if descendant_set.contains(child) && !depth.contains_key(child) {
                        depth.insert(*child, next_depth);
                        stack.push(*child);
                    }
                }
            }
        }
        let mut ordered = descendants;
        ordered.sort_by_key(|id| (Reverse(depth.get(id).copied().unwrap_or(0)), *id));
        ordered
    }
}
