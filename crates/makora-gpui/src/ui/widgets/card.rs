//! A bordered panel with an optional header (title, description, action on the right):
//!
//! ```ignore
//! Card::new()
//!     .small()
//!     .title("Top apps & sites")
//!     .description("Ranked by time spent, tinted by category.")
//!     .action(button)
//!     .child(list)
//! ```

use gpui_kit::component::{ActiveTheme, Sizable, Size, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, px,
};

use crate::ui::theme;

#[derive(IntoElement)]
pub struct Card {
    size: Size,
    title: Option<SharedString>,
    description: Option<SharedString>,
    action: Option<AnyElement>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Card {
    pub fn new() -> Self {
        Self {
            size: Size::Medium,
            title: None,
            description: None,
            action: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Controls at the header's right edge.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    fn is_small(&self) -> bool {
        matches!(self.size, Size::Small | Size::XSmall)
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

/// The padding and gap inside a card: 16px, or 12px for a small one.
pub fn card_spacing(small: bool) -> Pixels {
    if small { px(12.) } else { px(16.) }
}

impl Sizable for Card {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let small = self.is_small();
        let spacing = card_spacing(small);
        let has_header =
            self.title.is_some() || self.description.is_some() || self.action.is_some();
        let muted = cx.theme().muted_foreground;

        v_flex()
            .gap(spacing)
            .py(spacing)
            .overflow_hidden()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .bg(theme::card(cx))
            .text_color(theme::card_foreground(cx))
            .text_sm()
            .refine_style(&self.style)
            .when(has_header, |this| {
                this.child(
                    h_flex()
                        .items_start()
                        .gap_2()
                        .px(spacing)
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .when_some(self.title, |this, title| {
                                    this.child(
                                        div()
                                            .font_medium()
                                            .map(|this| {
                                                if small {
                                                    this.text_sm()
                                                } else {
                                                    this.text_base()
                                                }
                                            })
                                            .child(title),
                                    )
                                })
                                .when_some(self.description, |this, description| {
                                    this.child(div().text_sm().text_color(muted).child(description))
                                }),
                        )
                        .children(self.action),
                )
            })
            .when(!self.children.is_empty(), |this| {
                this.child(v_flex().px(spacing).children(self.children))
            })
    }
}
