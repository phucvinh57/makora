use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window};

use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

pub struct StatisticsScreen;

impl Screen for StatisticsScreen {
    type Location = ();

    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for StatisticsScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_6()
            .child(PageHeader::new("Statistics", "Where your time went"))
    }
}
