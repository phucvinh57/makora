//! When the user works: the hour-by-weekday table, the per-day timeline and the habits summary.

use chrono::{Local, Timelike};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px, relative,
};
use makora_lib::domain::types::{
    ActivityStats, MIN_DAYS_FOR_HABITS, TimelineBlock, format_clock as clock, format_hour_ranges,
};

use super::sections::{NOTHING_TO_SHOW, tile_grid};
use crate::ui::format::categories::{UNCATEGORIZED_COLOR, UNCATEGORIZED_LABEL};
use crate::ui::format::duration::format_seconds;
use crate::ui::format::stats_date::format_day;
use crate::ui::widgets::card::Card;
use crate::ui::widgets::category_dot::category_color;
use crate::ui::widgets::feedback::EmptyState;
use crate::ui::widgets::stat_tile::StatTile;

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const LABEL_WIDTH: f32 = 44.;
const DAY_SECONDS: f64 = 86400.;

pub fn time_table_card(stats: &ActivityStats, cx: &App) -> Card {
    let card = Card::new()
        .small()
        .h_full()
        .title("Time table")
        .description("Tracked time per weekday and hour; darker is busier.");
    let max = stats.hour_table.iter().flatten().copied().fold(0., f64::max);
    if max <= 0. {
        return card.child(EmptyState::new(NOTHING_TO_SHOW));
    }
    let theme = cx.theme();
    let (primary, empty, muted) = (theme.primary, theme.muted, theme.muted_foreground);
    let rows = stats.hour_table.iter().enumerate().map(|(day, hours)| {
        h_flex()
            .gap(px(2.))
            .child(row_label(WEEKDAYS[day], muted))
            .children(hours.iter().enumerate().map(|(hour, &seconds)| {
                let fill = if seconds > 0. {
                    primary.opacity(0.15 + 0.85 * (seconds / max) as f32)
                } else {
                    empty
                };
                let tip: SharedString = format!(
                    "{} {hour:02}:00–{:02}:00 · {}",
                    WEEKDAYS[day],
                    hour + 1,
                    format_seconds(seconds)
                )
                .into();
                div()
                    .id(("hour-cell", day * 24 + hour))
                    .flex_1()
                    .min_w_0()
                    .h(px(18.))
                    .rounded(px(3.))
                    .bg(fill)
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            }))
    });
    let active = if stats.habits.active_hours.is_empty() {
        "—".to_owned()
    } else {
        format_hour_ranges(&stats.habits.active_hours)
    };
    card.child(
        v_flex()
            .gap(px(2.))
            .children(rows)
            .child(hour_axis(muted))
            .child(
                div()
                    .pt_2()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("Active hours: {active}")),
            ),
    )
}

/// One row per day, blocks placed by time of day and coloured by category. `None` when the range
/// is too long for a timeline.
pub fn timeline_card(stats: &ActivityStats, cx: &App) -> Option<Card> {
    if stats.timeline.is_empty() {
        return None;
    }
    let theme = cx.theme();
    let (track, grid, muted, marker) = (
        theme.muted.opacity(0.5),
        theme.border,
        theme.muted_foreground,
        theme.danger,
    );
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let now_fraction = f64::from(now.num_seconds_from_midnight()) / DAY_SECONDS;
    let mut block_ix = 0;
    let rows: Vec<AnyElement> = stats
        .timeline
        .iter()
        .map(|day| {
            let blocks: Vec<AnyElement> = day
                .blocks
                .iter()
                .map(|block| {
                    block_ix += 1;
                    timeline_block(block, block_ix)
                })
                .collect();
            h_flex()
                .gap(px(2.))
                .child(row_label(format_day(&day.day), muted))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h(px(22.))
                        .rounded(px(4.))
                        .bg(track)
                        .children([6., 12., 18.].map(|hour: f32| {
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(hour / 24.))
                                .w(px(1.))
                                .bg(grid)
                        }))
                        .children(blocks)
                        .when(day.day == today, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top(px(-3.))
                                    .bottom(px(-3.))
                                    .left(relative(now_fraction as f32))
                                    .w(px(2.))
                                    .rounded_full()
                                    .bg(marker),
                            )
                        }),
                )
                .into_any_element()
        })
        .collect();
    Some(
        Card::new()
            .small()
            .title("Day timeline")
            .description("When each day's time was tracked, coloured by category.")
            .child(v_flex().gap(px(6.)).children(rows).child(hour_axis(muted))),
    )
}

fn timeline_block(block: &TimelineBlock, ix: usize) -> AnyElement {
    let color = category_color(block.category_color.as_deref().unwrap_or(UNCATEGORIZED_COLOR));
    let name = block.category_name.as_deref().unwrap_or(UNCATEGORIZED_LABEL);
    let tip: SharedString = format!(
        "{}–{} · {name} · {}",
        clock((block.start / 60.) as u32),
        clock((block.end / 60.) as u32),
        format_seconds(block.end - block.start)
    )
    .into();
    div()
        .id(("timeline-block", ix))
        .absolute()
        .top(px(3.))
        .bottom(px(3.))
        .left(relative((block.start / DAY_SECONDS) as f32))
        .w(relative(((block.end - block.start) / DAY_SECONDS) as f32))
        .min_w(px(2.))
        .rounded(px(2.))
        .bg(color)
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .into_any_element()
}

/// Tiles for the typical day, then the observations; a hint until there are enough days.
pub fn habits_section(stats: &ActivityStats, cx: &App) -> AnyElement {
    let habits = &stats.habits;
    let card = Card::new()
        .small()
        .title("Habits")
        .description("Patterns in when you work, with a suggestion for each.");
    if habits.active_days < MIN_DAYS_FOR_HABITS {
        return card
            .child(EmptyState::new("Track a few more days to see your habits."))
            .into_any_element();
    }
    let muted = cx.theme().muted_foreground;
    let minute = |minute: Option<u32>| minute.map_or_else(|| "—".to_owned(), clock);
    let focus = habits.longest_focus.as_ref();
    let tiles = tile_grid(vec![
        StatTile::new("Typical start", minute(habits.typical_start)).into_any_element(),
        StatTile::new("Typical end", minute(habits.typical_end)).into_any_element(),
        StatTile::new(
            "Longest focus",
            focus.map_or_else(|| "—".to_owned(), |f| format_seconds(f.seconds)),
        )
        .when_some(focus, |tile, f| tile.detail(format_day(&f.day)))
        .into_any_element(),
        StatTile::new("Late-night share", format!("{:.0}%", habits.late_share * 100.))
            .detail("22:00–04:00")
            .into_any_element(),
    ]);
    let observations = if stats.observations.is_empty() {
        div()
            .text_sm()
            .text_color(muted)
            .child("No strong patterns in this range.")
            .into_any_element()
    } else {
        v_flex()
            .gap_3()
            .children(stats.observations.iter().map(|observation| {
                v_flex()
                    .gap_0p5()
                    .text_sm()
                    .child(observation.text.clone())
                    .when_some(observation.suggestion.clone(), |this, suggestion| {
                        this.child(div().text_color(muted).child(suggestion))
                    })
            }))
            .into_any_element()
    };
    v_flex()
        .gap_3()
        .child(tiles)
        .child(card.child(observations))
        .into_any_element()
}

fn row_label(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .w(px(LABEL_WIDTH))
        .flex_shrink_0()
        .text_xs()
        .text_color(color)
        .child(text.into())
}

/// 0, 6, 12, 18 and 24 under a 24-hour track, after the row-label column.
fn hour_axis(color: Hsla) -> impl IntoElement {
    h_flex().gap(px(2.)).child(div().w(px(LABEL_WIDTH)).flex_shrink_0()).child(
        div()
            .relative()
            .flex_1()
            .h(px(16.))
            .text_xs()
            .text_color(color)
            .children([0u8, 6, 12, 18, 24].map(|hour| {
                let at = div().absolute().top_0().child(hour.to_string());
                match hour {
                    0 => at.left_0(),
                    24 => at.right_0(),
                    _ => at.left(relative(f32::from(hour) / 24.)),
                }
            })),
    )
}

