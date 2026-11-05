use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window};

use crate::ui::route::HistoryFilters;
use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

pub struct HistoryScreen {
    pub filters: HistoryFilters,
}

impl Screen for HistoryScreen {
    type Location = HistoryFilters;

    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            filters: HistoryFilters::default(),
        }
    }

    fn navigate(&mut self, filters: HistoryFilters, _: &mut Window, cx: &mut Context<Self>) {
        self.filters = filters;
        cx.notify();
    }
}

impl Render for HistoryScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .child(PageHeader::new("History", "App-focus history"))
    }
}
