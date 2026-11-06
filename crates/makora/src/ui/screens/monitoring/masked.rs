use chrono::{Local, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement, Render, Styled, Task, Window, div, px,
};
use makora_lib::network_log::{MaskedSummary, RemoteSend};

use super::period::LogPeriod;
use crate::runtime::window::show_main_window;
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::network::format_relative;
use crate::ui::format::number::to_locale_string;
use crate::ui::format::privacy::masked_kind_label;
use crate::ui::route::Route;
use crate::ui::widgets::card::Card;
use crate::ui::widgets::feedback::{AllClear, EmptyState, ErrorBanner};
use crate::ui::widgets::ranked_bar::{RankedBar, bar_fraction};
use crate::ui::widgets::stat_tile::StatTile;

const SHOWN_SENDS: usize = 20;

/// What the privacy rules took out of activities before they went to a remote AI provider.
pub struct MaskedTab {
    period: LogPeriod,
    summary: Option<MaskedSummary>,
    error: Option<String>,
    load: Task<()>,
}

impl MaskedTab {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            period: LogPeriod::default(),
            summary: None,
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
                    .masked(days, Utc::now())
                    .map_err(UiError::from)
            },
            |tab, result, _| match result {
                Ok(summary) => {
                    tab.summary = Some(summary);
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

impl Render for MaskedTab {
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
                        "What was hidden from remote AI providers before activities were sent. \
                         Local models see everything and send nothing.",
                    ),
            )
            .child(self.period.picker(
                "masked-period",
                cx.listener(|tab: &mut Self, period: &LogPeriod, _, cx| {
                    tab.set_period(*period, cx)
                }),
            ))
            .child(
                Button::new("privacy-settings")
                    .outline()
                    .small()
                    .icon(Icon::new(IconName::Settings2))
                    .label("Privacy settings")
                    .on_click(|_, _, cx| {
                        cx.defer(|cx| show_main_window(Some(Route::Settings), cx));
                    }),
            );

        let body: AnyElement = match &self.summary {
            None if self.error.is_none() => Skeleton::new().h_24().w_full().into_any_element(),
            None => div().into_any_element(),
            Some(summary) if summary.sends.is_empty() => AllClear::new(format!(
                "No activity was sent to a remote AI provider {phrase}."
            ))
            .into_any_element(),
            Some(summary) => v_flex()
                .gap_4()
                .child(tiles(summary))
                .child(by_kind(summary, phrase))
                .child(sends(summary, cx))
                .into_any_element(),
        };

        v_flex()
            .gap_4()
            .child(header)
            .when_some(self.error.clone(), |this, error| {
                this.child(ErrorBanner::new(format!(
                    "Failed to load masked data: {error}"
                )))
            })
            .child(body)
    }
}

fn tiles(summary: &MaskedSummary) -> impl IntoElement {
    let runs = summary.sends.len() as u64;
    div()
        .grid()
        .grid_cols(3)
        .gap_3()
        .child(
            StatTile::new(
                "Activities sent",
                to_locale_string(summary.activities_sent as f64),
            )
            .detail(format!(
                "in {} {}",
                to_locale_string(runs as f64),
                if runs == 1 { "run" } else { "runs" }
            )),
        )
        .child(
            StatTile::new("Items masked", to_locale_string(summary.redactions as f64))
                .detail("replaced before sending"),
        )
        .child(
            StatTile::new("Kinds found", to_locale_string(summary.kinds.len() as f64))
                .detail("caught by the rules"),
        )
}

fn by_kind(summary: &MaskedSummary, phrase: &str) -> impl IntoElement {
    let max = summary.kinds.first().map_or(0, |(_, count)| *count) as f64;
    Card::new()
        .small()
        .title("Masked by kind")
        .description(format!("What the privacy rules caught {phrase}."))
        .map(|card| {
            if summary.kinds.is_empty() {
                card.child(EmptyState::new("Nothing matched the privacy rules."))
            } else {
                card.child(
                    v_flex()
                        .gap_2()
                        .children(summary.kinds.iter().map(|(kind, count)| {
                            RankedBar::new(
                                masked_kind_label(kind),
                                bar_fraction(*count as f64, max),
                                to_locale_string(*count as f64),
                            )
                        })),
                )
            }
        })
}

fn sends(summary: &MaskedSummary, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let border = theme.border;
    let now = Local::now();
    let fields = summary
        .sends
        .iter()
        .find(|send| !send.fields.is_empty())
        .map(|send| format!("Each activity told the provider its {}.", send.fields));
    let hidden = summary.sends.len().saturating_sub(SHOWN_SENDS);
    Card::new()
        .small()
        .title("Sent to remote providers")
        .when_some(fields, |card, fields| card.description(fields))
        .child(
            v_flex()
                .text_sm()
                .children(summary.sends.iter().take(SHOWN_SENDS).map(|send| {
                    h_flex()
                        .items_start()
                        .gap_3()
                        .py_2()
                        .border_t_1()
                        .border_color(border)
                        .child(
                            div()
                                .w(px(120.))
                                .flex_shrink_0()
                                .text_color(muted)
                                .child(format_relative(&send.at, now)),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .truncate()
                                        .font_medium()
                                        .child(format!("{} · {}", send.host, send.model)),
                                )
                                .child(div().text_xs().text_color(muted).child(kinds_line(send))),
                        )
                        .child(div().flex_shrink_0().text_color(muted).child(format!(
                            "{} activities",
                            to_locale_string(send.activities_sent as f64)
                        )))
                }))
                .when(hidden > 0, |this| {
                    this.child(
                        div()
                            .pt_2()
                            .border_t_1()
                            .border_color(border)
                            .text_xs()
                            .text_color(muted)
                            .child(format!("And {hidden} earlier runs.")),
                    )
                }),
        )
}

/// `Email addresses 3, Keys and tokens 1`, or a note that nothing was masked.
fn kinds_line(send: &RemoteSend) -> String {
    if send.kinds.is_empty() {
        return "Nothing masked".to_string();
    }
    send.kinds
        .iter()
        .map(|(kind, count)| format!("{} {count}", masked_kind_label(kind)))
        .collect::<Vec<_>>()
        .join(", ")
}
