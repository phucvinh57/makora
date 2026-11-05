//! Inline states for a panel: an error banner, and the "nothing here" line.

use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div};

/// A red box with a load or save error, above the content it concerns.
#[derive(IntoElement)]
pub struct ErrorBanner {
    message: SharedString,
}

impl ErrorBanner {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl RenderOnce for ErrorBanner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let danger = cx.theme().danger;
        div()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(danger.opacity(0.4))
            .bg(danger.opacity(0.1))
            .px_3()
            .py_2()
            .text_sm()
            .text_color(danger)
            .child(self.message)
    }
}

/// A centered, muted line such as "Nothing to show for this range."
#[derive(IntoElement)]
pub struct EmptyState {
    message: SharedString,
}

impl EmptyState {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .w_full()
            .py_8()
            .text_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(self.message)
    }
}
