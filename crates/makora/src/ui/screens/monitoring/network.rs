use chrono::{Local, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement, Render, Styled, Task, Window, div, px,
};
use makora_lib::logs::HostTraffic;

use super::period::LogPeriod;
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::network::{format_relative, format_size};
use crate::ui::format::number::to_locale_string;
use crate::ui::widgets::card::Card;
use crate::ui::widgets::feedback::{AllClear, ErrorBanner};
use crate::ui::widgets::stat_tile::StatTile;

const NUMERIC_WIDTH: f32 = 96.;
const LAST_CONTACT_WIDTH: f32 = 128.;

/// Every server Makora contacted, with what went each way.
pub struct NetworkTab {
    period: LogPeriod,
    hosts: Option<Vec<HostTraffic>>,
    error: Option<String>,
    load: Task<()>,
}

impl NetworkTab {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            period: LogPeriod::default(),
            hosts: None,
            error: None,
            load: Task::ready(()),
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let days = self.period.days();
        self.load = data::fetch(
            cx,
            move |core| {
                core.network_log
                    .hosts(days, Utc::now())
                    .map_err(UiError::from)
            },
            |tab, result, _| match result {
                Ok(hosts) => {
                    tab.hosts = Some(hosts);
                    tab.error = None;
                }
                Err(err) => tab.error = Some(err.message),
            },
        );
    }

    fn set_period(&mut self, period: LogPeriod, cx: &mut Context<Self>) {
        if self.period != period {
            self.period = period;
            self.refresh(cx);
            cx.notify();
        }
    }
}

impl Render for NetworkTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let phrase = self.period.phrase();
        let header = h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .mr_auto()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Every server Makora contacted. Tracking and categorizing with a local \
                         model never use the network; downloading one does.",
                    ),
            )
            .child(self.period.picker(
                "network-period",
                cx.listener(|tab: &mut Self, period: &LogPeriod, _, cx| {
                    tab.set_period(*period, cx)
                }),
            ));

        let body: AnyElement = match &self.hosts {
            None if self.error.is_none() => Skeleton::new().h_24().w_full().into_any_element(),
            None => div().into_any_element(),
            Some(hosts) if hosts.is_empty() => {
                AllClear::new(format!("Nothing was sent over the network {phrase}."))
                    .into_any_element()
            }
            Some(hosts) => v_flex()
                .gap_4()
                .child(tiles(hosts))
                .child(
                    Card::new()
                        .small()
                        .title("Servers contacted")
                        .description(format!("Most recent first, {phrase}."))
                        .child(host_table(hosts, cx)),
                )
                .into_any_element(),
        };

        v_flex()
            .gap_4()
            .child(header)
            .when_some(self.error.clone(), |this, error| {
                this.child(ErrorBanner::new(format!(
                    "Failed to load network activity: {error}"
                )))
            })
            .child(body)
    }
}

fn tiles(hosts: &[HostTraffic]) -> impl IntoElement {
    let requests: u64 = hosts.iter().map(|host| host.requests).sum();
    let sent: u64 = hosts.iter().map(|host| host.bytes_sent).sum();
    let received: u64 = hosts.iter().map(|host| host.bytes_received).sum();
    let activities: u64 = hosts.iter().map(|host| host.activities_sent).sum();
    div()
        .grid()
        .grid_cols(4)
        .gap_3()
        .child(StatTile::new(
            "Servers",
            to_locale_string(hosts.len() as f64),
        ))
        .child(StatTile::new("Requests", to_locale_string(requests as f64)))
        .child(
            StatTile::new("Sent", format_size(sent as f64)).detail(format!(
                "{} activities",
                to_locale_string(activities as f64)
            )),
        )
        .child(StatTile::new("Received", format_size(received as f64)))
}

fn host_table(hosts: &[HostTraffic], cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let border = theme.border;
    let now = Local::now();
    let numeric = |content: AnyElement| {
        div()
            .flex()
            .flex_col()
            .items_end()
            .flex_shrink_0()
            .w(px(NUMERIC_WIDTH))
            .child(content)
    };
    let header = h_flex()
        .h_8()
        .gap_2()
        .font_medium()
        .text_color(muted)
        .child(div().w(px(200.)).flex_shrink_0().child("Host"))
        .child(div().flex_1().min_w_0().child("Why"))
        .child(numeric("Requests".into_any_element()))
        .child(numeric("Sent".into_any_element()))
        .child(numeric("Received".into_any_element()))
        .child(
            div()
                .flex()
                .justify_end()
                .flex_shrink_0()
                .w(px(LAST_CONTACT_WIDTH))
                .child("Last contact"),
        );

    v_flex()
        .text_sm()
        .child(header)
        .children(hosts.iter().map(|host| {
            let why = if host.purposes.is_empty() {
                "—".to_owned()
            } else {
                host.purposes.join(", ")
            };
            h_flex()
                .items_start()
                .gap_2()
                .py_2()
                .border_t_1()
                .border_color(border)
                .child(
                    div()
                        .w(px(200.))
                        .flex_shrink_0()
                        .min_w_0()
                        .truncate()
                        .font_medium()
                        .child(host.host.clone()),
                )
                .child(div().flex_1().min_w_0().text_color(muted).child(why))
                .child(numeric(
                    to_locale_string(host.requests as f64).into_any_element(),
                ))
                .child(numeric(
                    v_flex()
                        .items_end()
                        .child(
                            h_flex()
                                .gap_1()
                                .child(Icon::new(IconName::ArrowUp).xsmall().text_color(muted))
                                .child(format_size(host.bytes_sent as f64)),
                        )
                        .when(host.activities_sent > 0, |this| {
                            this.child(div().text_xs().text_color(muted).child(format!(
                                "{} activities",
                                to_locale_string(host.activities_sent as f64)
                            )))
                        })
                        .when(host.redactions > 0, |this| {
                            this.child(div().text_xs().text_color(muted).child(format!(
                                "{} masked",
                                to_locale_string(host.redactions as f64)
                            )))
                        })
                        .into_any_element(),
                ))
                .child(numeric(
                    h_flex()
                        .gap_1()
                        .child(Icon::new(IconName::ArrowDown).xsmall().text_color(muted))
                        .child(format_size(host.bytes_received as f64))
                        .into_any_element(),
                ))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .flex_shrink_0()
                        .w(px(LAST_CONTACT_WIDTH))
                        .text_color(muted)
                        .child(format_relative(&host.last_contact, now)),
                )
        }))
}
