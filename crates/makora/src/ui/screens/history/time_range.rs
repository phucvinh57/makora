//! The history's time filter: a button naming the range, opening presets and a custom range
//! with two optional bounds, plus an X to drop an active range. The bounds are edited inline
//! with a calendar and a time field, since a date picker's own dropdown inside this popover
//! would close it on every click.

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::calendar::{Calendar, CalendarEvent, CalendarState, Date};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::time_field::{TimeField, TimeFieldEvent, TimeFieldState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Anchor, AnyElement, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, px,
};

use crate::ui::format::time_range::{
    ALL_TIME, RELATIVE_PRESETS, TimeRange, describe_range, from_local_input_value, is_range_active,
    to_local_input_value,
};

const INPUT_FORMAT: &str = "%Y-%m-%dT%H:%M";

/// An applied bound as the local wall-clock time the fields edit.
pub fn bound_to_local(value: Option<&str>) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(&to_local_input_value(value), INPUT_FORMAT).ok()
}

pub fn local_to_bound(value: Option<NaiveDateTime>) -> Option<String> {
    value.and_then(|at| from_local_input_value(&at.format(INPUT_FORMAT).to_string()))
}

/// Both bounds set, and the end not after the start.
pub fn is_invalid(from: Option<NaiveDateTime>, to: Option<NaiveDateTime>) -> bool {
    matches!((from, to), (Some(from), Some(to)) if from >= to)
}

pub fn format_bound_field(value: NaiveDateTime) -> String {
    value.format("%b %-d, %Y, %H:%M").to_string()
}

fn now_to_minute() -> NaiveDateTime {
    let now = Local::now().naive_local();
    now.with_second(0)
        .and_then(|at| at.with_nanosecond(0))
        .unwrap_or(now)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bound {
    From,
    To,
}

/// One bound's value and the calendar and time field that edit it.
struct BoundField {
    value: Option<NaiveDateTime>,
    calendar: Entity<CalendarState>,
    time: Entity<TimeFieldState>,
}

impl BoundField {
    fn new(window: &mut Window, cx: &mut Context<TimeRangeFilter>) -> Self {
        Self {
            value: None,
            calendar: cx.new(|cx| CalendarState::new(window, cx)),
            time: cx.new(|cx| TimeFieldState::new(window, cx)),
        }
    }

    /// The date and time a pick builds on: the value, else now as React's picker did.
    fn base(&self) -> NaiveDateTime {
        self.value.unwrap_or_else(now_to_minute)
    }

    fn set(
        &mut self,
        value: Option<NaiveDateTime>,
        window: &mut Window,
        cx: &mut Context<TimeRangeFilter>,
    ) {
        self.value = value;
        let date = Date::Single(value.map(|at| at.date()));
        let time = value.unwrap_or_else(now_to_minute).time();
        self.calendar
            .update(cx, |calendar, cx| calendar.set_date(date, window, cx));
        self.time
            .update(cx, |field, cx| field.set_time(time, window, cx));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeRangeFilterEvent {
    Change(TimeRange),
}

pub struct TimeRangeFilter {
    range: TimeRange,
    open: bool,
    editing: Option<Bound>,
    from: BoundField,
    to: BoundField,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TimeRangeFilterEvent> for TimeRangeFilter {}

impl TimeRangeFilter {
    pub fn new(range: TimeRange, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let from = BoundField::new(window, cx);
        let to = BoundField::new(window, cx);
        let mut subscriptions = Vec::new();
        for (bound, field) in [(Bound::From, &from), (Bound::To, &to)] {
            subscriptions.push(cx.subscribe_in(
                &field.calendar,
                window,
                move |this, _, CalendarEvent::Selected(date), window, cx| {
                    if let Date::Single(Some(day)) = date {
                        this.pick_day(bound, *day, window, cx);
                    }
                },
            ));
            subscriptions.push(cx.subscribe_in(
                &field.time,
                window,
                move |this, _, TimeFieldEvent::Change(time), window, cx| {
                    this.pick_time(bound, *time, window, cx);
                },
            ));
        }
        Self {
            range,
            open: false,
            editing: None,
            from,
            to,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_range(&mut self, range: TimeRange, cx: &mut Context<Self>) {
        if range != self.range {
            self.range = range;
            cx.notify();
        }
    }

    fn field(&mut self, bound: Bound) -> &mut BoundField {
        match bound {
            Bound::From => &mut self.from,
            Bound::To => &mut self.to,
        }
    }

    fn pick_day(
        &mut self,
        bound: Bound,
        day: NaiveDate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.field(bound);
        let at = day.and_time(field.base().time());
        field.set(Some(at), window, cx);
        cx.notify();
    }

    fn pick_time(
        &mut self,
        bound: Bound,
        time: NaiveTime,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.field(bound);
        let at = field.base().date().and_time(time);
        field.value = Some(at);
        field.calendar.update(cx, |calendar, cx| {
            calendar.set_date(Date::Single(Some(at.date())), window, cx)
        });
        cx.notify();
    }

    fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open && !self.open {
            // The custom fields start from the applied range each time the popover opens.
            let from = bound_to_local(self.range.from.as_deref());
            let to = bound_to_local(self.range.to.as_deref());
            self.from.set(from, window, cx);
            self.to.set(to, window, cx);
            self.editing = None;
        }
        self.open = open;
        cx.notify();
    }

    fn apply(&mut self, range: TimeRange, cx: &mut Context<Self>) {
        self.open = false;
        self.range = range.clone();
        cx.emit(TimeRangeFilterEvent::Change(range));
        cx.notify();
    }

    fn render_bound(
        &self,
        bound: Bound,
        placeholder: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let field = match bound {
            Bound::From => &self.from,
            Bound::To => &self.to,
        };
        let editing = self.editing == Some(bound);
        let value = field.value;
        let id = match bound {
            Bound::From => "range-from",
            Bound::To => "range-to",
        };
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .id(id)
                    .h_8()
                    .w_full()
                    .gap_2()
                    .px(px(10.))
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(if editing { theme.ring } else { theme.input })
                    .text_sm()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.editing = if this.editing == Some(bound) {
                            None
                        } else {
                            Some(bound)
                        };
                        cx.notify();
                    }))
                    .child(
                        Icon::new(IconName::CalendarClock)
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(div().flex_1().min_w_0().truncate().map(|this| match value {
                        Some(at) => this.child(format_bound_field(at)),
                        None => this.text_color(theme.muted_foreground).child(placeholder),
                    }))
                    .when(value.is_some(), |this| {
                        this.child(
                            div()
                                .id(SharedString::from(format!("{id}-clear")))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size_5()
                                .rounded(px(4.))
                                .text_color(theme.muted_foreground)
                                .hover(|style| style.bg(theme.muted))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.field(bound).set(None, window, cx);
                                    if this.editing == Some(bound) {
                                        this.editing = None;
                                    }
                                    cx.notify();
                                }))
                                .child(Icon::new(IconName::X).xsmall()),
                        )
                    }),
            )
            .when(editing, |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .p_2()
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(theme.border)
                        .child(Calendar::new(&field.calendar).small())
                        .child(
                            h_flex()
                                .gap_2()
                                .child(TimeField::new(&field.time).small())
                                .child(div().flex_1())
                                .child(
                                    Button::new(SharedString::from(format!("{id}-now")))
                                        .ghost()
                                        .small()
                                        .label("Now")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.field(bound).set(
                                                Some(now_to_minute()),
                                                window,
                                                cx,
                                            );
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("{id}-done")))
                                        .primary()
                                        .small()
                                        .label("Done")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.editing = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let border = theme.border;
        let (from, to) = (self.from.value, self.to.value);
        let invalid = is_invalid(from, to);
        let presets: Vec<AnyElement> = RELATIVE_PRESETS
            .iter()
            .map(|&(kind, label)| {
                Button::new(label)
                    .small()
                    .w_full()
                    .justify_start()
                    .map(|this| {
                        if self.range.kind == kind {
                            this.secondary()
                        } else {
                            this.ghost()
                        }
                    })
                    .label(label)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.apply(TimeRange::preset(kind), cx)),
                    )
                    .into_any_element()
            })
            .collect();
        let from_field = self.render_bound(Bound::From, "From (any time)", cx);
        let to_field = self.render_bound(Bound::To, "To (any time)", cx);
        v_flex()
            .w(px(296.))
            .gap_3()
            .child(div().grid().grid_cols(2).gap_1p5().children(presets))
            .child(
                v_flex()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(muted)
                            .child("Custom"),
                    )
                    .child(v_flex().gap_1p5().child(from_field).child(to_field))
                    .when(invalid, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(danger)
                                .child("The end must come after the start."),
                        )
                    })
                    .child(
                        Button::new("apply-custom-range")
                            .primary()
                            .small()
                            .w_full()
                            .label("Apply custom range")
                            .disabled(invalid || (from.is_none() && to.is_none()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let range =
                                    TimeRange::custom(local_to_bound(from), local_to_bound(to));
                                this.apply(range, cx)
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for TimeRangeFilter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active = is_range_active(&self.range);
        let (primary, ring) = (theme.primary, theme.primary.opacity(0.4));
        let this = cx.entity().downgrade();
        let content_view = this.clone();
        h_flex()
            .gap_1()
            .child(
                Popover::new("time-range")
                    .anchor(Anchor::TopRight)
                    .open(self.open)
                    .on_open_change(move |open, window, cx| {
                        let _ = this.update(cx, |this, cx| this.set_open(*open, window, cx));
                    })
                    .trigger(
                        Button::new("time-range-trigger")
                            .outline()
                            .small()
                            .icon(IconName::CalendarRange)
                            .label(describe_range(&self.range))
                            .when(active, |this| this.text_color(primary).border_color(ring)),
                    )
                    .content(move |_, _, cx| {
                        content_view
                            .update(cx, |this, cx| this.render_content(cx))
                            .unwrap_or_else(|_| div().into_any_element())
                    }),
            )
            .when(active, |this| {
                this.child(
                    Button::new("clear-time-range")
                        .outline()
                        .small()
                        .icon(IconName::X)
                        .tooltip("Clear time filter")
                        .text_color(primary)
                        .border_color(ring)
                        .on_click(cx.listener(|this, _, _, cx| this.apply(ALL_TIME, cx))),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn bounds_round_trip_through_local_time() {
        let local = at(5, 14, 30);
        let bound = local_to_bound(Some(local));
        assert!(bound.is_some());
        assert_eq!(bound_to_local(bound.as_deref()), Some(local));
        assert_eq!(local_to_bound(None), None);
        assert_eq!(bound_to_local(None), None);
        assert_eq!(bound_to_local(Some("garbage")), None);
    }

    #[test]
    fn the_end_must_come_after_the_start() {
        assert!(!is_invalid(None, None));
        assert!(!is_invalid(Some(at(5, 9, 0)), None));
        assert!(!is_invalid(Some(at(5, 9, 0)), Some(at(5, 9, 1))));
        assert!(is_invalid(Some(at(5, 9, 0)), Some(at(5, 9, 0))));
        assert!(is_invalid(Some(at(6, 9, 0)), Some(at(5, 9, 0))));
    }

    #[test]
    fn a_bound_reads_like_the_web_picker() {
        assert_eq!(format_bound_field(at(5, 9, 5)), "Nov 5, 2026, 09:05");
    }
}
