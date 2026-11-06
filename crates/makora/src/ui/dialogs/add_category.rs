//! "Add category", optionally under a parent. A caller that wants the new category, such as a
//! category picker's "New category", passes `on_created`:
//!
//! ```ignore
//! dialogs::open::<AddCategoryDialog>(
//!     AddCategoryData::new(None).on_created(move |created, window, cx| pick(created.id, window, cx)),
//!     window,
//!     cx,
//! );
//! ```

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::{App, AppContext, Window};
use makora_lib::domain::types::{Category, NewCategory};

use crate::ui::dialogs::AppDialog;
use crate::ui::dialogs::category_form::{CategoryForm, OnCreated, Submit};
use crate::ui::format::categories::DEFAULT_CATEGORY_COLOR;

pub struct AddCategoryData {
    /// The category the new one goes under; it also lends its color. `None` adds a top-level one.
    pub parent: Option<Category>,
    /// The fields to start from, such as a suggested category's; empty, with the parent's color,
    /// when `None`.
    pub prefill: Option<NewCategory>,
    /// Called in the main window with the created category, after the dialog closes and once
    /// `AppState::categories` lists it. Not called if that reload fails.
    pub on_created: Option<OnCreated>,
}

impl AddCategoryData {
    pub fn new(parent: Option<Category>) -> Self {
        Self {
            parent,
            prefill: None,
            on_created: None,
        }
    }

    pub fn prefill(mut self, initial: NewCategory) -> Self {
        self.prefill = Some(initial);
        self
    }

    pub fn on_created(
        mut self,
        on_created: impl Fn(&Category, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_created = Some(Rc::new(on_created));
        self
    }
}

pub struct AddCategoryDialog;

impl AppDialog for AddCategoryDialog {
    type Data = AddCategoryData;

    fn open(data: AddCategoryData, window: &mut Window, cx: &mut App) {
        let initial = data.prefill.unwrap_or_else(|| NewCategory {
            name: String::new(),
            color: data.parent.as_ref().map_or_else(
                || DEFAULT_CATEGORY_COLOR.to_string(),
                |parent| parent.color.clone(),
            ),
            description: None,
            parent_id: data.parent.as_ref().map(|parent| parent.id),
        });
        let submit = Submit::Create {
            on_created: data.on_created,
        };
        let form = cx.new(|cx| CategoryForm::new(initial, HashSet::new(), submit, window, cx));
        CategoryForm::open(form, window, cx);
    }
}
