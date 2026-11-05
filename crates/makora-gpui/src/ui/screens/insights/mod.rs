use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window};

use crate::ui::prefs::Prefs;
use crate::ui::route::InsightsTab;
use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

pub struct InsightsScreen {
    pub tab: InsightsTab,
}

impl InsightsScreen {
    /// Shows `tab` and remembers it for the next run.
    pub fn set_tab(&mut self, tab: InsightsTab, cx: &mut Context<Self>) {
        self.tab = tab;
        Prefs::update(cx, |prefs| prefs.insights_tab = tab);
        cx.notify();
    }
}

impl Screen for InsightsScreen {
    type Location = InsightsTab;

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            tab: Prefs::get(cx).insights_tab,
        }
    }

    fn navigate(&mut self, tab: InsightsTab, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(tab, cx);
    }
}

impl Render for InsightsScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_6().child(PageHeader::new(
            "Insights",
            "What the AI used and cost, and what Makora has been doing",
        ))
    }
}
