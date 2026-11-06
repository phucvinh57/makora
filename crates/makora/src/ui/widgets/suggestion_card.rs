//! The look every list of suggestions shares: a tinted panel with a title and why the
//! suggestions are there, and a bordered row per suggestion.
//!
//! ```ignore
//! SuggestionCard::new("Suggested rules", "From how you filed activity yourself.")
//!     .children(suggestions.iter().map(|s| suggestion_row(cx).child(…)))
//! ```

use gpui_kit::component::{ActiveTheme, StyledExt, v_flex};
use gpui_kit::{
    AnyElement, App, Div, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
};

#[derive(IntoElement)]
pub struct SuggestionCard {
    title: SharedString,
    description: SharedString,
    children: Vec<AnyElement>,
}

impl SuggestionCard {
    pub fn new(title: impl Into<SharedString>, description: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            children: Vec::new(),
        }
    }
}

impl ParentElement for SuggestionCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SuggestionCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(theme.primary.opacity(0.4))
            .bg(theme.primary.opacity(0.05))
            .p_3()
            .child(
                v_flex()
                    .child(div().text_sm().font_medium().child(self.title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(self.description),
                    ),
            )
            .children(self.children)
    }
}

/// The frame of one suggestion in a [`SuggestionCard`].
pub fn suggestion_row(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .px_3()
        .py_2()
}
