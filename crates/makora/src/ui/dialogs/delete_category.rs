//! Deletes a category and everything under it, after saying how much goes with it.

use gpui_kit::{App, Window};
use makora_lib::domain::types::Category;

use crate::ui::app_state::{AppState, Core};
use crate::ui::dialogs::AppDialog;
use crate::ui::dialogs::confirm::Confirm;
use crate::ui::error::UiError;
use crate::ui::format::categories::category_subtree_ids;
use crate::ui::widgets::toast::Toast;

/// The title, description and button for deleting a category with `descendants` below it.
pub fn delete_copy(name: &str, descendants: usize) -> (String, String, &'static str) {
    if descendants == 0 {
        return (
            format!("Delete {name}?"),
            "Pins and rules that file under this category are removed. This cannot be undone."
                .into(),
            "Delete",
        );
    }
    let noun = if descendants == 1 {
        "subcategory"
    } else {
        "subcategories"
    };
    (
        format!("Delete {name} and everything under it?"),
        format!(
            "This also deletes {descendants} {noun} and their pins and rules. This cannot be undone."
        ),
        "Delete branch",
    )
}

pub struct DeleteCategoryDialog;

impl AppDialog for DeleteCategoryDialog {
    type Data = Category;

    fn open(category: Category, window: &mut Window, cx: &mut App) {
        let descendants =
            category_subtree_ids(category.id, &AppState::read(cx).categories).len() - 1;
        let (title, description, label) = delete_copy(&category.name, descendants);
        let id = category.id;
        Confirm::new(title)
            .description(description)
            .confirm_label(label)
            .destructive()
            .open(window, cx, move |_, cx| {
                let core = Core::get(cx);
                let job = cx
                    .background_executor()
                    .spawn(async move { core.categories.delete(id) });
                cx.spawn(async move |cx| {
                    let result = job.await.map_err(UiError::from);
                    cx.update(|cx| match result {
                        Ok(()) => {
                            AppState::update(cx, |state, cx| state.invalidate_categorization(cx))
                        }
                        Err(err) => Toast::error(err.to_string()).show_in_main_window(cx),
                    });
                })
                .detach();
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaf_is_a_plain_delete() {
        let (title, description, label) = delete_copy("Work", 0);
        assert_eq!(title, "Delete Work?");
        assert!(description.starts_with("Pins and rules"));
        assert_eq!(label, "Delete");
    }

    #[test]
    fn a_branch_counts_what_goes_with_it() {
        let (title, description, label) = delete_copy("Work", 1);
        assert_eq!(title, "Delete Work and everything under it?");
        assert!(description.starts_with("This also deletes 1 subcategory and"));
        assert_eq!(label, "Delete branch");
        assert!(delete_copy("Work", 3).1.contains("3 subcategories"));
    }
}
