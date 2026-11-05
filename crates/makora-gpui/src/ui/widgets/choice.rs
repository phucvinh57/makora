//! A labelled option for a `Select` whose value is a fixed wire name, such as a rule kind:
//!
//! ```ignore
//! let kinds = RULE_KINDS.map(|kind| Choice::new(kind.as_str(), rule_kind_label(kind))).to_vec();
//! let select = cx.new(|cx| SelectState::new(kinds, Some(IndexPath::default()), window, cx));
//! ```

use gpui_kit::SharedString;
use gpui_kit::component::searchable_list::SearchableListItem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: &'static str,
    pub label: SharedString,
}

impl Choice {
    pub fn new(value: &'static str, label: impl Into<SharedString>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }
}

impl SearchableListItem for Choice {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}
