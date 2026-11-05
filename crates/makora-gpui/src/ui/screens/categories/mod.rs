use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window};

use crate::ui::prefs::Prefs;
use crate::ui::route::CategoriesTab;
use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

pub struct CategoriesScreen {
    pub tab: CategoriesTab,
}

impl CategoriesScreen {
    /// Shows `tab` and remembers it for the next run.
    pub fn set_tab(&mut self, tab: CategoriesTab, cx: &mut Context<Self>) {
        self.tab = tab;
        Prefs::update(cx, |prefs| prefs.categories_tab = tab);
        cx.notify();
    }
}

impl Screen for CategoriesScreen {
    type Location = CategoriesTab;

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            tab: Prefs::get(cx).categories_tab,
        }
    }

    fn navigate(&mut self, tab: CategoriesTab, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(tab, cx);
    }
}

impl Render for CategoriesScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_6().child(PageHeader::new(
            "Categories",
            "Organize activity with categories, rules and the AI",
        ))
    }
}
