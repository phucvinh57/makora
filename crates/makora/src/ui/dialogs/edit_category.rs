//! Renames, recolors, redescribes or moves a category.

use gpui_kit::{App, AppContext, Window};
use makora_lib::domain::types::{Category, NewCategory};

use crate::ui::app_state::AppState;
use crate::ui::dialogs::AppDialog;
use crate::ui::dialogs::category_form::{CategoryForm, Submit};
use crate::ui::format::categories::category_subtree_ids;

pub struct EditCategoryDialog;

impl AppDialog for EditCategoryDialog {
    type Data = Category;

    fn open(category: Category, window: &mut Window, cx: &mut App) {
        // Moving a category inside its own branch would cut that branch loose from every root,
        // so its own subtree is not on offer as a new parent.
        let excluded = category_subtree_ids(category.id, &AppState::read(cx).categories);
        let initial = NewCategory {
            name: category.name,
            color: category.color,
            description: category.description,
            parent_id: category.parent_id,
        };
        let submit = Submit::Update(category.id);
        let form = cx.new(|cx| CategoryForm::new(initial, excluded, submit, window, cx));
        CategoryForm::open(form, window, cx);
    }
}
