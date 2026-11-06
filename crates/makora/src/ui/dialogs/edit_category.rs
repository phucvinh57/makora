//! Renames, recolors, redescribes or moves a category.

use std::rc::Rc;

use gpui_kit::{App, AppContext, Window};
use makora_lib::domain::types::{Category, NewCategory};

use crate::ui::app_state::AppState;
use crate::ui::dialogs::AppDialog;
use crate::ui::dialogs::category_form::{CategoryForm, OnSaved, Submit};
use crate::ui::format::categories::category_subtree_ids;

/// The category as the form starts out, and what to do once it is saved.
pub struct EditCategoryData {
    pub category: Category,
    pub on_saved: Option<OnSaved>,
}

impl EditCategoryData {
    pub fn on_saved(mut self, on_saved: impl Fn(&mut App) + 'static) -> Self {
        self.on_saved = Some(Rc::new(on_saved));
        self
    }
}

impl From<Category> for EditCategoryData {
    fn from(category: Category) -> Self {
        Self {
            category,
            on_saved: None,
        }
    }
}

pub struct EditCategoryDialog;

impl AppDialog for EditCategoryDialog {
    type Data = EditCategoryData;

    fn open(EditCategoryData { category, on_saved }: EditCategoryData, window: &mut Window, cx: &mut App) {
        // Moving a category inside its own branch would cut that branch loose from every root,
        // so its own subtree is not on offer as a new parent.
        let excluded = category_subtree_ids(category.id, &AppState::read(cx).categories);
        let initial = NewCategory {
            name: category.name,
            color: category.color,
            description: category.description,
            parent_id: category.parent_id,
        };
        let submit = Submit::Update {
            id: category.id,
            on_saved,
        };
        let form = cx.new(|cx| CategoryForm::new(initial, excluded, submit, window, cx));
        CategoryForm::open(form, window, cx);
    }
}
