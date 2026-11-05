mod exclusions;
mod general;
mod providers;
mod tracking;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, px,
};

use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;
use exclusions::ExclusionsSection;
use general::GeneralSection;
use providers::ProvidersSection;
use tracking::TrackingSection;

pub struct SettingsScreen {
    general: Entity<GeneralSection>,
    tracking: Entity<TrackingSection>,
    exclusions: Entity<ExclusionsSection>,
    providers: Entity<ProvidersSection>,
}

impl Screen for SettingsScreen {
    type Location = ();

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            general: cx.new(GeneralSection::new),
            tracking: cx.new(|cx| TrackingSection::new(window, cx)),
            exclusions: cx.new(|cx| ExclusionsSection::new(window, cx)),
            providers: cx.new(|cx| ProvidersSection::new(window, cx)),
        }
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
            .child(self.general.clone())
            .child(self.tracking.clone())
            .child(self.exclusions.clone())
            .child(self.providers.clone())
    }
}
