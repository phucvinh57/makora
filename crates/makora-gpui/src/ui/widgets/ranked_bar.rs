//! A row of a ranked list: the name, a bar sized against the largest row, and the amount.

use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::{
    App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
    relative,
};

use crate::ui::format::categories::UNCATEGORIZED_COLOR;
use crate::ui::widgets::category_dot::category_color;

/// How much of the bar `value` fills when the largest row is `max`, from 0 to 1.
pub fn bar_fraction(value: f64, max: f64) -> f32 {
    if max > 0.0 && value.is_finite() {
        (value / max).clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

#[derive(IntoElement)]
pub struct RankedBar {
    label: SharedString,
    fraction: f32,
    amount: SharedString,
    color: Hsla,
}

impl RankedBar {
    /// `fraction` is from [`bar_fraction`].
    pub fn new(
        label: impl Into<SharedString>,
        fraction: f32,
        amount: impl Into<SharedString>,
    ) -> Self {
        Self {
            label: label.into(),
            fraction: fraction.clamp(0.0, 1.0),
            amount: amount.into(),
            color: category_color(UNCATEGORIZED_COLOR),
        }
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for RankedBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_3()
            .child(
                div()
                    .w(px(160.))
                    .flex_shrink_0()
                    .truncate()
                    .text_sm()
                    .child(self.label),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_2()
                    .overflow_hidden()
                    .rounded_full()
                    .bg(cx.theme().muted)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(relative(self.fraction))
                            .rounded_full()
                            .bg(self.color),
                    ),
            )
            .child(
                div()
                    .w(px(64.))
                    .flex_shrink_0()
                    .text_right()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.amount),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_scale_against_the_largest_row() {
        assert_eq!(bar_fraction(50.0, 200.0), 0.25);
        assert_eq!(bar_fraction(200.0, 200.0), 1.0);
        assert_eq!(bar_fraction(300.0, 200.0), 1.0);
    }

    #[test]
    fn an_empty_list_draws_empty_bars() {
        assert_eq!(bar_fraction(0.0, 0.0), 0.0);
        assert_eq!(bar_fraction(5.0, 0.0), 0.0);
        assert_eq!(bar_fraction(f64::NAN, 10.0), 0.0);
    }
}
