use std::collections::HashMap;

use crate::domain::types::Category;
use crate::shared::ids::CategoryId;
use crate::shared::text::single_line;
use crate::taxonomy::domain::PATH_SEPARATOR;

/// A category as the model sees it: its full path on one line, which is also the exact answer
/// the model gives for it.
pub(super) struct Choice {
    pub path: String,
    pub id: CategoryId,
    pub description: Option<String>,
}

pub(super) fn choices(categories: &[Category]) -> Vec<Choice> {
    let by_id: HashMap<CategoryId, &Category> = categories.iter().map(|c| (c.id, c)).collect();
    let mut out: Vec<Choice> = categories
        .iter()
        .map(|category| {
            let mut names = vec![category.name.as_str()];
            let mut cursor = category.parent_id;
            // Bounded so a corrupt parent cycle cannot hang the run.
            for _ in 0..16 {
                let Some(parent) = cursor.and_then(|id| by_id.get(&id)) else {
                    break;
                };
                names.push(parent.name.as_str());
                cursor = parent.parent_id;
            }
            names.reverse();
            Choice {
                path: single_line(&names.join(PATH_SEPARATOR)),
                id: category.id,
                description: category.description.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(id: i64, name: &str, parent: Option<i64>) -> Category {
        Category {
            id: CategoryId(id),
            name: name.into(),
            color: String::new(),
            parent_id: parent.map(CategoryId),
            description: None,
        }
    }

    #[test]
    fn paths_join_ancestors_on_one_line() {
        let choices = choices(&[category(1, "Work", None), category(2, "Deep\n  work", Some(1))]);
        let paths: Vec<(&str, CategoryId)> = choices.iter().map(|c| (c.path.as_str(), c.id)).collect();
        assert_eq!(paths, [("Work", CategoryId(1)), ("Work > Deep work", CategoryId(2))]);
    }

    #[test]
    fn a_parent_cycle_does_not_hang() {
        let choices = choices(&[category(1, "A", Some(2)), category(2, "B", Some(1))]);
        assert_eq!(choices.len(), 2);
    }
}
