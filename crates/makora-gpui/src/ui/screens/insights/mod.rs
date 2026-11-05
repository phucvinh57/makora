mod cost_chart;
mod logs;
mod usage;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, ClickEvent, Context, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div, px,
};

use crate::ui::prefs::Prefs;
use crate::ui::route::InsightsTab;
use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

use logs::LogsTab;
use usage::UsageTab;

/// The shell's padding around a screen, above and below, with room for borders.
const CONTENT_PADDING: f32 = 52.;

pub struct InsightsScreen {
    pub tab: InsightsTab,
    usage: Entity<UsageTab>,
    logs: Entity<LogsTab>,
}

impl InsightsScreen {
    /// Shows `tab` and remembers it for the next run.
    pub fn set_tab(&mut self, tab: InsightsTab, cx: &mut Context<Self>) {
        self.tab = tab;
        Prefs::update(cx, |prefs| prefs.insights_tab = tab);
        if tab != InsightsTab::Logs {
            self.logs.update(cx, |logs, _| logs.stop_polling());
        }
        cx.notify();
    }
}

impl Screen for InsightsScreen {
    type Location = InsightsTab;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            tab: Prefs::get(cx).insights_tab,
            usage: cx.new(UsageTab::new),
            logs: cx.new(|cx| LogsTab::new(window, cx)),
        }
    }

    fn navigate(&mut self, tab: InsightsTab, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(tab, cx);
    }
}

impl Render for InsightsScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab;
        let logs = tab == InsightsTab::Logs;
        let height = window.viewport_size().height - px(CONTENT_PADDING);
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

/// A row of buttons in a bordered box, one of them picked, like a period switch.
pub(crate) fn segmented(
    id: impl Into<ElementId>,
    labels: Vec<SharedString>,
    selected: usize,
    on_select: impl Fn(&usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let on_select = std::rc::Rc::new(on_select);
    h_flex()
        .id(id)
        .gap_1()
        .p(px(2.))
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .children(labels.into_iter().enumerate().map(|(ix, label)| {
            let on_select = on_select.clone();
            Button::new(ix)
                .small()
                .map(|this| {
                    if ix == selected {
                        this.secondary()
                    } else {
                        this.ghost()
                    }
                })
                .label(label)
                .on_click(move |_: &ClickEvent, window, cx| on_select(&ix, window, cx))
        }))
}
