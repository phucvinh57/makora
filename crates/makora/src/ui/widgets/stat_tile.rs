//! One headline number in a small card, like "Total tracked time · 6h 12m".

use gpui_kit::component::{ActiveTheme, Sizable, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div};

use crate::ui::widgets::card::Card;

#[derive(IntoElement)]
pub struct StatTile {
    label: SharedString,
    value: SharedString,
    detail: Option<SharedString>,
}

impl StatTile {
    pub fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: None,
        }
    }

    /// A muted line under the value, such as "12 runs".
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl RenderOnce for StatTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        Card::new()
            .small()
            .child(
                div()
                    .mb_1()
                    .text_xs()
                    .font_medium()
                    .text_color(muted)
                    .child(self.label),
            )
            .child(
                div()
                    .text_2xl()
                    .font_semibold()
                    .truncate()
                    .child(self.value),
            )
            .when_some(self.detail, |this, detail| {
                this.child(div().text_xs().text_color(muted).truncate().child(detail))
            })
    }
}
