mod cost_chart;
mod logs;
mod usage;

use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px,
};

use crate::ui::prefs::Prefs;
use crate::ui::route::InsightsTab;
use crate::ui::screens::Screen;
use crate::ui::shell::content_size;
use crate::ui::widgets::page_header::PageHeader;

use logs::LogsTab;
use usage::UsageTab;

pub struct InsightsScreen {
    pub tab: InsightsTab,
    usage: Entity<UsageTab>,
    logs: Entity<LogsTab>,
}

impl InsightsScreen {
    /// Shows `tab` and remembers it for the next run.
    pub fn set_tab(&mut self, tab: InsightsTab, cx: &mut Context<Self>) {
        let switched = tab != self.tab;
        self.tab = tab;
        Prefs::update(cx, |prefs| prefs.insights_tab = tab);
        if switched && tab == InsightsTab::Usage {
            self.usage.update(cx, |usage, cx| usage.refresh(cx));
        }
        self.poll_logs(tab == InsightsTab::Logs, cx);
        cx.notify();
    }

    fn poll_logs(&self, on: bool, cx: &mut Context<Self>) {
        self.logs.update(cx, |logs, cx| logs.set_polling(on, cx));
    }
}

impl Screen for InsightsScreen {
    type Location = InsightsTab;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let screen = Self {
            tab: Prefs::get(cx).insights_tab,
            usage: cx.new(UsageTab::new),
            logs: cx.new(|cx| LogsTab::new(window, cx)),
        };
        screen.poll_logs(screen.tab == InsightsTab::Logs, cx);
        screen
    }

    fn navigate(&mut self, tab: InsightsTab, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(tab, cx);
    }

    fn shown(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match self.tab {
            InsightsTab::Usage => self.usage.update(cx, |usage, cx| usage.refresh(cx)),
            InsightsTab::Logs => self.poll_logs(true, cx),
        }
    }

    fn hidden(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.poll_logs(false, cx);
    }
}

impl Render for InsightsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab;
        let logs = tab == InsightsTab::Logs;
        let height = content_size(window, cx).height;
        v_flex()
            .gap_4()
            .when(logs, |this| this.min_h(height))
            .child(PageHeader::new(
                "Insights",
                "What the AI used and cost, and what Makora has been doing",
            ))
            .child(
                div().w(px(168.)).child(
                    TabBar::new("insights-tabs")
                        .segmented()
                        .selected_index(match tab {
                            InsightsTab::Usage => 0,
                            InsightsTab::Logs => 1,
                        })
                        .children([Tab::new().label("AI usage"), Tab::new().label("Logs")])
                        .on_click(cx.listener(|screen, ix: &usize, _, cx| {
                            let tab = if *ix == 0 {
                                InsightsTab::Usage
                            } else {
                                InsightsTab::Logs
                            };
                            screen.set_tab(tab, cx);
                        })),
                ),
            )
            .map(|this| {
                if logs {
                    this.child(div().flex_1().flex().flex_col().child(self.logs.clone()))
                } else {
                    this.child(self.usage.clone())
                }
            })
    }
}
