//! A checkbox that can also show "some" (a dash), for select-all over a partly selected list.

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, px,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CheckState {
    #[default]
    Unchecked,
    Indeterminate,
    Checked,
}

impl CheckState {
    /// All, some or none of `total` rows are selected.
    pub fn from_counts(selected: usize, total: usize) -> Self {
        match selected {
            0 => Self::Unchecked,
            n if n >= total => Self::Checked,
            _ => Self::Indeterminate,
        }
    }

    /// What a click asks for: a partly checked box checks everything.
    pub fn toggled(self) -> bool {
        self != Self::Checked
    }
}

type OnToggle = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct TriCheckbox {
    id: ElementId,
    state: CheckState,
    disabled: bool,
    on_toggle: Option<OnToggle>,
}

impl TriCheckbox {
    pub fn new(id: impl Into<ElementId>, state: CheckState) -> Self {
        Self {
            id: id.into(),
            state,
            disabled: false,
            on_toggle: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called with whether the box should now be checked.
    pub fn on_toggle(mut self, on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(on_toggle));
        self
    }
}

impl RenderOnce for TriCheckbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let filled = self.state != CheckState::Unchecked;
        let icon = match self.state {
            CheckState::Unchecked => None,
            CheckState::Indeterminate => Some(IconName::Minus),
            CheckState::Checked => Some(IconName::Check),
        };
        let next = self.state.toggled();

        div()
            .id(self.id)
            .size_4()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .rounded(px(4.))
            .map(|this| {
                if filled {
                    this.border_color(theme.primary).bg(theme.primary)
                } else {
                    this.border_color(theme.input).bg(theme.background)
                }
            })
            .when_some(icon, |this, icon| {
                this.child(
                    Icon::new(icon)
                        .xsmall()
                        .text_color(theme.primary_foreground),
                )
            })
            .map(|this| {
                if self.disabled {
                    this.opacity(0.5).cursor_not_allowed()
                } else {
                    this.cursor_pointer()
                }
            })
            .when_some(
                self.on_toggle.filter(|_| !self.disabled),
                |this, on_toggle| {
                    this.on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_toggle(next, window, cx);
                    })
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_follows_how_many_rows_are_selected() {
        assert_eq!(CheckState::from_counts(0, 5), CheckState::Unchecked);
        assert_eq!(CheckState::from_counts(2, 5), CheckState::Indeterminate);
        assert_eq!(CheckState::from_counts(5, 5), CheckState::Checked);
        assert_eq!(CheckState::from_counts(0, 0), CheckState::Unchecked);
    }

    #[test]
    fn clicking_a_partial_box_selects_everything() {
        assert!(CheckState::Unchecked.toggled());
        assert!(CheckState::Indeterminate.toggled());
        assert!(!CheckState::Checked.toggled());
    }
}
