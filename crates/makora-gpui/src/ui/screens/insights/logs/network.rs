use chrono::Local;
use gpui_kit::assets::IconName;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use makora_lib::logs::reader::HostTraffic;

use super::LogsTab;
use crate::ui::format::logs::{format_relative, format_size};
use crate::ui::format::number::to_locale_string;
use crate::ui::widgets::card::Card;

const NUMERIC_WIDTH: f32 = 96.;

impl LogsTab {
    pub(super) fn render_network(&self, cx: &mut Context<Self>) -> AnyElement {
        let phrase = self.period.phrase();
        let theme = cx.theme();
        let content: AnyElement = if let Some(error) = &self.network_error {
            div()
                .text_sm()
                .text_color(theme.danger)
                .child(format!("Failed to load network activity: {error}"))
                .into_any_element()
        } else {
            match &self.hosts {
                None => Skeleton::new().h_10().w_full().into_any_element(),
                Some(hosts) if hosts.is_empty() => h_flex()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.muted.opacity(0.5))
                    .px_3()
                    .py_2()
                    .text_sm()
                    .child(
                        Icon::new(IconName::ShieldCheck)
                            .small()
                            .text_color(theme.success),
                    )
                    .child(format!("Nothing was sent over the network {phrase}."))
                    .into_any_element(),
                Some(hosts) => self.render_hosts(hosts, cx).into_any_element(),
            }
        };
        Card::new()
            .small()
            .title("Data that left this device")
            .description(format!(
                "Every server Makora contacted {phrase}. Activity tracking and local AI never \
                 use the network."
            ))
            .child(content)
            .into_any_element()
    }

    fn render_hosts(&self, hosts: &[HostTraffic], cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(numeric("Last contact".into_any_element()));

        v_flex()
            .text_sm()
            .child(header)
            .children(hosts.iter().enumerate().map(|(ix, host)| {
                let name = SharedString::from(host.host.clone());
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
                        div().w(px(200.)).flex_shrink_0().min_w_0().child(
                            div()
                                .id(("host", ix))
                                .truncate()
                                .font_medium()
                                .cursor_pointer()
                                .hover(|style| style.underline())
                                .tooltip(|window, cx| {
                                    Tooltip::new("Show these requests in the log").build(window, cx)
                                })
                                .on_click(cx.listener({
                                    let name = name.clone();
                                    move |tab, _: &ClickEvent, window, cx| {
                                        tab.show_host(&name, window, cx)
                                    }
                                }))
                                .child(name),
                        ),
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
                            .into_any_element(),
                    ))
                    .child(numeric(
                        h_flex()
                            .gap_1()
                            .child(Icon::new(IconName::ArrowDown).xsmall().text_color(muted))
                            .child(format_size(host.bytes_received as f64))
                            .into_any_element(),
                    ))
                    .child(numeric(
                        div()
                            .text_color(muted)
                            .child(format_relative(&host.last_contact, now))
                            .into_any_element(),
                    ))
            }))
    }
}
