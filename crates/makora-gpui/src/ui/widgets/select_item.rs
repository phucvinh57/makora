//! A row for a plain `Select`: a value, the label shown for it, and an optional leading icon.
//!
//! ```ignore
//! let items = SelectItems::from(vec![SelectItem::new("site", "Website")]);
//! let select = cx.new(|cx| SelectState::new(items, Some(IndexPath::default()), window, cx));
//! ```

use gpui_kit::assets::IconName;
use gpui_kit::component::searchable_list::{SearchableListItem, SearchableVec};
use gpui_kit::component::{Icon, IndexPath, Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{App, IntoElement, ParentElement, SharedString, Styled, Window};

#[derive(Debug, Clone, PartialEq)]
pub struct SelectItem {
    pub value: SharedString,
    pub label: SharedString,
    pub icon: Option<IconName>,
}

impl SelectItem {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl SearchableListItem for SelectItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_flex()
            .gap_2()
            .when_some(self.icon, |this, icon| this.child(Icon::new(icon).small()))
            .child(self.label.clone())
    }

    fn value(&self) -> &SharedString {
        &self.value
    }
}

pub type SelectItems = SearchableVec<SelectItem>;

/// Where `value` sits in `choices`, to select it.
pub fn select_item_index(choices: &[SelectItem], value: &str) -> Option<IndexPath> {
    choices
        .iter()
        .position(|choice| choice.value.as_ref() == value)
        .map(|row| IndexPath::default().row(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_value_by_position() {
        let choices = [SelectItem::new("a", "A"), SelectItem::new("b", "B")];
        assert_eq!(select_item_index(&choices, "b").map(|ix| ix.row), Some(1));
        assert_eq!(select_item_index(&choices, "c"), None);
    }
}
