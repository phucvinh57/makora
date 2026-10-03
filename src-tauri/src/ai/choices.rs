use std::collections::HashMap;

use crate::domain::types::Category;
use crate::shared::ids::CategoryId;

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
                path: names.join(" > "),
                id: category.id,
                description: category.description.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}
