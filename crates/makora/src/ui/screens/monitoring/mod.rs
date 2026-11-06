mod cost_chart;
mod masked;
mod network;
mod period;
mod usage;

use std::time::Duration;

use gpui_kit::component::h_flex;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Task,
    Window, div,
};

use crate::ui::motion;
use crate::ui::prefs::Prefs;
use crate::ui::route::MonitoringTab;
use crate::ui::screens::Screen;
use crate::ui::widgets::page_header::PageHeader;

use masked::MaskedTab;
use network::NetworkTab;
use usage::UsageTab;

/// The network log grows while the page is open, from AI runs and model downloads alike.
const POLL: Duration = Duration::from_secs(10);

const TABS: [(MonitoringTab, &str); 3] = [
    (MonitoringTab::Network, "Network"),
    (MonitoringTab::Masked, "Masked data"),
    (MonitoringTab::Usage, "AI usage"),
];

pub struct MonitoringScreen {
    pub tab: MonitoringTab,
    network: Entity<NetworkTab>,
    masked: Entity<MaskedTab>,
    usage: Entity<UsageTab>,
    poll: Task<()>,
}

impl MonitoringScreen {
    /// Shows `tab` and remembers it for the next run.
    pub fn set_tab(&mut self, tab: MonitoringTab, cx: &mut Context<Self>) {
        let switched = tab != self.tab;
        self.tab = tab;
        Prefs::update(cx, |prefs| prefs.monitoring_tab = tab);
        if switched {
            self.refresh(cx);
        }
        cx.notify();
    }

    /// Reloads the visible log tab every few seconds while the screen is on view.
    fn set_polling(&mut self, on: bool, cx: &mut Context<Self>) {
        if !on {
            self.poll = Task::ready(());
            return;
        }
        self.poll = cx.spawn(async move |screen, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let polled = screen.update(cx, |screen, cx| {
                    if screen.tab != MonitoringTab::Usage {
                        screen.refresh(cx);
                    }
                });
                if polled.is_err() {
                    break;
                }
            }
        });
    }

    fn refresh(&self, cx: &mut Context<Self>) {
        match self.tab {
            MonitoringTab::Network => self.network.update(cx, |tab, cx| tab.refresh(cx)),
            MonitoringTab::Masked => self.masked.update(cx, |tab, cx| tab.refresh(cx)),
            MonitoringTab::Usage => self.usage.update(cx, |tab, cx| tab.refresh(cx)),
        }
    }
}

impl Screen for MonitoringScreen {
    type Location = MonitoringTab;

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let screen = Self {
            tab: Prefs::get(cx).monitoring_tab,
            network: cx.new(NetworkTab::new),
            masked: cx.new(MaskedTab::new),
            usage: cx.new(UsageTab::new),
            poll: Task::ready(()),
        };
        screen.refresh(cx);
        screen
    }

    fn navigate(&mut self, tab: MonitoringTab, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(tab, cx);
    }

    fn shown(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
        self.set_polling(true, cx);
    }

    fn hidden(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.set_polling(false, cx);
    }
}

impl Render for MonitoringScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab;
        let body = match tab {
            MonitoringTab::Network => self.network.clone().into_any_element(),
            MonitoringTab::Masked => self.masked.clone().into_any_element(),
            MonitoringTab::Usage => self.usage.clone().into_any_element(),
        };
        v_flex()
            .gap_4()
            .child(PageHeader::new(
                "Monitoring",
                "What left this device, what was masked first, and what the AI used",
            ))
            .child(
                h_flex().child(
                    TabBar::new("monitoring-tabs")
                        .segmented()
                        .selected_index(TABS.iter().position(|(t, _)| *t == tab).unwrap_or(0))
                        .children(TABS.iter().map(|(_, label)| Tab::new().label(*label)))
                        .on_click(cx.listener(|screen, ix: &usize, _, cx| {
                            screen.set_tab(TABS[*ix].0, cx);
                        })),
                ),
            )
            .child(motion::enter(
                SharedString::from(format!("monitoring-{tab:?}")),
                div().w_full().child(body),
                cx,
            ))
    }
}
