//! Previous/next with a "Page 2 of 5 (120 sessions)" readout. Pages are zero-based.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, h_flex};
use gpui_kit::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, div,
};

pub fn page_summary(page: usize, page_count: usize, label: &str) -> String {
    format!("Page {} of {} ({label})", page + 1, page_count.max(1))
}

pub fn has_previous(page: usize) -> bool {
    page > 0
}

pub fn has_next(page: usize, page_count: usize) -> bool {
    page + 1 < page_count
}

type OnPage = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Pagination {
    id: ElementId,
    page: usize,
    page_count: usize,
    label: SharedString,
    on_page: Option<OnPage>,
}

impl Pagination {
    pub fn new(
        id: impl Into<ElementId>,
        page: usize,
        page_count: usize,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            page,
            page_count,
            label: label.into(),
            on_page: None,
        }
    }

    pub fn on_page(mut self, on_page: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_page = Some(Rc::new(on_page));
        self
    }
}

impl RenderOnce for Pagination {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let page = self.page;
        let previous = self.on_page.clone();
        let next = self.on_page;

        h_flex()
            .id(self.id)
            .justify_center()
            .gap_2()
            .text_sm()
            .child(
                Button::new("previous")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronLeft)
                    .label("Previous")
                    .disabled(!has_previous(page))
                    .on_click(move |_, window, cx| {
                        if let Some(on_page) = &previous {
                            on_page(page.saturating_sub(1), window, cx);
                        }
                    }),
            )
            .child(
                div()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(page_summary(page, self.page_count, &self.label)),
            )
            .child(
                Button::new("next")
                    .ghost()
                    .small()
                    .label("Next")
                    .child(Icon::new(IconName::ChevronRight))
                    .disabled(!has_next(page, self.page_count))
                    .on_click(move |_, window, cx| {
                        if let Some(on_page) = &next {
                            on_page(page + 1, window, cx);
                        }
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readout_counts_from_one() {
        assert_eq!(
            page_summary(0, 5, "120 sessions"),
            "Page 1 of 5 (120 sessions)"
        );
        assert_eq!(page_summary(0, 0, "0 sessions"), "Page 1 of 1 (0 sessions)");
    }

    #[test]
    fn the_buttons_stop_at_the_ends() {
        assert!(!has_previous(0));
        assert!(has_previous(1));
        assert!(has_next(0, 2));
        assert!(!has_next(1, 2));
        assert!(!has_next(0, 0));
    }
}
