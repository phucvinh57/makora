use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window, px};

use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

pub struct SettingsScreen;

impl Screen for SettingsScreen {
    type Location = ();

    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for SettingsScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .max_w(px(768.))
            .gap_6()
            .child(PageHeader::new(
                "Settings",
                "App preferences, tracking exclusions and AI providers",
            ))
    }
}
