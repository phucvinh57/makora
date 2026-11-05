use gpui_kit::assets::IconName;
use gpui_kit::base::ScrollBounce;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, list, px,
};
use makora_lib::logs::{LogEntry, LogLevel};
use serde_json::Value;

use super::model::{GroupBy, ListItem, empty_message, level_label};
use super::{LogsTab, area_icon};
use crate::ui::format::date_range::parse_instant;
use crate::ui::format::duration::format_timestamp;
use crate::ui::format::logs::{format_field_value, key_fields};
use crate::ui::format::number::to_locale_string;
use crate::ui::screens::insights::logs::model::area_label;
use crate::ui::theme;

const HEADER_HEIGHT: f32 = 40.;
/// The width of one character of the monospace field text.
const MONO_CHAR: f32 = 7.2;

impl LogsTab {
    pub(super) fn render_list(&self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.loaded {
            return div().into_any_element();
        }
        let theme = cx.theme();
        let card = v_flex()
            .flex_1()
            .min_h(px(160.))
            .overflow_hidden()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme::card(cx))
            .text_color(theme::card_foreground(cx))
            .when(self.stale, |this| this.opacity(0.6));

        let note = self.truncated.then(|| {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "Showing the newest {} matching entries. Narrow the search or period to see \
                     older ones.",
                    to_locale_string(self.entries.len() as f64)
                ))
        });

        if self.items.is_empty() {
            return v_flex()
                .gap_2()
                .children(note)
                .child(
                    card.flex_none().min_h_0().p_8().items_center().child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(empty_message(self.filtered(), self.period)),
                    ),
                )
                .into_any_element();
        }

        let sticky = self.sticky_group();
        let list = list(
            self.list.clone(),
            cx.processor(|tab: &mut Self, ix: usize, _, cx| tab.render_item(ix, cx)),
        )
        .absolute()
        .inset_0();
        let view = cx.entity().downgrade();
        let list =
            ScrollBounce::new("log-list-scroll", &self.list, list).on_scroll(move |_, cx| {
                let _ = view.update(cx, |_, cx| cx.notify());
            });

        v_flex()
            .flex_1()
            .min_h(px(160.))
            .gap_2()
            .children(note)
            .child(
                card.child(
                    div()
                        .id("log-list")
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(list)
                        .when_some(sticky, |this, group| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .child(self.render_header_row(group, false, cx)),
                            )
                        })
                        .vertical_scrollbar(&self.list),
                ),
            )
            .into_any_element()
    }

    /// The group whose rows fill the top of the list, once its own header has scrolled away.
    fn sticky_group(&self) -> Option<usize> {
        let top = self.list.logical_scroll_top();
        match self.items.get(top.item_ix)? {
            ListItem::Header(group) if top.offset_in_item > px(0.) => Some(*group),
            ListItem::Header(_) => None,
            ListItem::Row { group, .. } => Some(*group),
        }
    }

    fn render_header_row(&self, group: usize, border_top: bool, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let group = &self.groups[group];
        h_flex()
            .w_full()
            .h(px(HEADER_HEIGHT))
            .px_3()
            .gap_2()
            .text_sm()
            .font_medium()
            .bg(theme::card(cx))
            .border_b_1()
            .when(border_top, |this| this.border_t_1())
            .border_color(theme.border)
            .child(group.label.clone())
            .child(
                div()
                    .font_normal()
                    .text_color(theme.muted_foreground)
                    .child(to_locale_string(group.rows.len() as f64)),
            )
            .into_any_element()
    }

    fn render_item(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        match self.items.get(ix).copied() {
            Some(ListItem::Header(group)) => self.render_header_row(group, group > 0, cx),
            Some(ListItem::Row { group, row }) => {
                let last = self.groups[group].rows.last() == Some(&row);
                self.render_row(ix, row, last, cx)
            }
            None => div().into_any_element(),
        }
    }

    fn render_row(&self, ix: usize, row: usize, last: bool, cx: &mut Context<Self>) -> AnyElement {
        let keyed = &self.entries[row];
        let entry = &keyed.entry;
        let expanded = self.expanded.contains(&keyed.key);
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hover_bg = theme.muted.opacity(0.5);
        let mono = theme.mono_font_family.clone();
        let time = parse_instant(&entry.ts).map_or_else(
            || entry.ts.clone(),
            |at| {
                let format = if self.group_by == GroupBy::Area {
                    "%b %-d %H:%M:%S"
                } else {
                    "%H:%M:%S"
                };
                at.with_timezone(&chrono::Local).format(format).to_string()
            },
        );
        let inline = key_fields(&entry.fields)
            .into_iter()
            .map(|(field, value)| {
                h_flex()
                    .flex_shrink_0()
                    .mr_3()
                    .child(div().opacity(0.6).child(format!("{field}=")))
                    .child(format_field_value(field, value))
            })
            .collect::<Vec<_>>();

        v_flex()
            .w_full()
            .overflow_hidden()
            .when(!last, |this| this.border_b_1())
            .border_color(theme.border)
            .child(
                h_flex()
                    .id(SharedString::from(format!("log-row-{ix}")))
                    .w_full()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_bg))
                    .when(expanded, |this| this.bg(hover_bg))
                    .on_click(cx.listener({
                        let key = keyed.key.clone();
                        move |tab, _: &ClickEvent, _, cx| tab.toggle_row(&key, cx)
                    }))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(muted),
                    )
                    .child(div().flex_shrink_0().text_color(muted).child(time))
                    .child(level_tag(entry.level, cx))
                    .child(
                        Tag::secondary()
                            .outline()
                            .xsmall()
                            .flex_shrink_0()
                            .w(px(96.))
                            .gap_1()
                            .text_color(muted)
                            .child(Icon::new(area_icon(entry.area)).xsmall())
                            .child(area_label(entry.area)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_shrink_1()
                            .truncate()
                            .font_medium()
                            .child(entry.message.clone()),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .font_family(mono.clone())
                            .text_xs()
                            .text_color(muted)
                            .children(inline),
                    ),
            )
            .when(expanded, |this| this.child(details(entry, cx)))
            .into_any_element()
    }
}

fn level_tag(level: LogLevel, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let tag = match level {
        LogLevel::Error => Tag::danger(),
        LogLevel::Warn => Tag::warning(),
        LogLevel::Info => Tag::secondary(),
        LogLevel::Debug | LogLevel::Trace => Tag::secondary().text_color(theme.muted_foreground),
    };
    tag.xsmall()
        .flex_shrink_0()
        .w(px(56.))
        .justify_center()
        .child(level_label(level))
}

fn details(entry: &LogEntry, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let mut rows: Vec<(String, AnyElement)> = vec![
        (
            "time".to_owned(),
            div().child(format_timestamp(&entry.ts)).into_any_element(),
        ),
        (
            "target".to_owned(),
            div().child(entry.target.clone()).into_any_element(),
        ),
    ];
    rows.extend(entry.fields.iter().map(|(field, value)| {
        let formatted = format_field_value(field, value);
        let raw = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        let show_raw = formatted != raw && value.is_number();
        (
            field.clone(),
            h_flex()
                .flex_wrap()
                .gap_2()
                .child(div().min_w_0().child(formatted))
                .when(show_raw, |this| {
                    this.child(div().text_color(muted).child(format!("({raw})")))
                })
                .into_any_element(),
        )
    }));
    let key_width = rows
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0) as f32
        * MONO_CHAR;

    v_flex()
        .gap_1()
        .py_3()
        .pr_3()
        .pl(px(34.))
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.3))
        .font_family(theme.mono_font_family.clone())
        .text_xs()
        .children(rows.into_iter().map(|(key, value)| {
            h_flex()
                .w_full()
                .items_start()
                .gap_4()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(key_width.clamp(32., 240.)))
                        .text_color(muted)
                        .child(key),
                )
                .child(div().flex_1().min_w_0().child(value))
        }))
}
