//! One headline number in a small card, like "Total tracked time · 6h 12m".

use gpui_kit::component::{ActiveTheme, Sizable, StyledExt};
use gpui_kit::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div};

use crate::ui::widgets::card::Card;

#[derive(IntoElement)]
pub struct StatTile {
    label: SharedString,
    value: SharedString,
}

impl StatTile {
    pub fn new(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

impl RenderOnce for StatTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        Card::new()
            .small()
            .child(
                div()
                    .mb_1()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.label),
            )
            .child(
                div()
                    .text_2xl()
                    .font_semibold()
                    .truncate()
                    .child(self.value),
            )
    }
}
