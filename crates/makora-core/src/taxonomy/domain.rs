use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use crate::shared::text::single_line;

use super::error::TaxonomyError;

/// Joins a category to its parent's name in the path the AI picks from (`Work > Meetings`).
pub(crate) const PATH_SEPARATOR: &str = " > ";

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

/// `name` on one line with its whitespace collapsed, as it is stored and shown. A name must not be
/// empty, and must not contain the path separator, or two categories could share a path.
pub(crate) fn category_name(name: &str) -> Result<String, TaxonomyError> {
    let name = single_line(name);
    if name.is_empty() {
        return Err(TaxonomyError::InvalidName("a category needs a name".into()));
    }
    if name.contains(PATH_SEPARATOR) {
        return Err(TaxonomyError::InvalidName(format!(
            "a category name cannot contain \"{}\"",
            PATH_SEPARATOR.trim()
        )));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_kept_on_one_trimmed_line() {
        assert_eq!(category_name("  Deep\n work\t ").unwrap(), "Deep work");
    }

    #[test]
    fn blank_names_are_rejected() {
        assert!(matches!(category_name(" \n\t"), Err(TaxonomyError::InvalidName(_))));
    }

    #[test]
    fn names_with_the_path_separator_are_rejected() {
        assert!(matches!(category_name("Work > Meetings"), Err(TaxonomyError::InvalidName(_))));
        assert!(matches!(category_name("Work\n>  Meetings"), Err(TaxonomyError::InvalidName(_))));
        assert_eq!(category_name("A>B").unwrap(), "A>B");
    }
}
