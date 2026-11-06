//! The cards of the statistics screen. The layouts wrap instead of measuring: two blocks with a
//! flex basis of half the breakpoint sit side by side once the content is that wide, which is
//! what the web UI's container queries did.

use gpui_kit::component::chart::{BarChart, PieChart};
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Corners, Div, IntoElement, ParentElement, Pixels, SharedString, Styled, div,
    px,
};
use makora_lib::domain::types::{ActivityStats, Category};

use crate::ui::format::stats_date::RangePreset;

use super::breakdown::{busiest_day, category_breakdown};
use super::habits::{habits_section, time_table_card, timeline_card};
use crate::ui::motion;
use crate::ui::format::categories::UNCATEGORIZED_COLOR;
use crate::ui::format::duration::format_seconds;
use crate::ui::format::number::to_locale_string;
use crate::ui::format::stats_date::format_day;
use crate::ui::widgets::card::Card;
use crate::ui::widgets::category_dot::category_color;
use crate::ui::widgets::feedback::EmptyState;
use crate::ui::widgets::ranked_bar::{RankedBar, bar_fraction};
use crate::ui::widgets::stat_tile::StatTile;

pub(super) const NOTHING_TO_SHOW: &str = "Nothing to show for this range.";
/// Wide enough for the two breakdown cards side by side (`@4xl`, 56rem).
const CARDS_SIDE_BY_SIDE: f32 = 896.;
/// Wide enough for the four tiles in one row (`@3xl`, 48rem).
const TILES_IN_ONE_ROW: f32 = 768.;
/// Wide enough for the donut beside its legend (`@2xl`, 42rem).
const LEGEND_BESIDE_DONUT: f32 = 672.;
const DONUT_SIZE: f32 = 208.;
/// Most day labels under the daily chart, so 30 or 90 days stay readable.
const DAY_LABELS: usize = 7;
const GAP_3: f32 = 12.;
const GAP_4: f32 = 16.;
const GAP_6: f32 = 24.;

/// Blocks of `basis` that share a row while it fits them and each take a whole row otherwise.
fn wrapping_row(gap: f32) -> Div {
    h_flex().w_full().flex_wrap().items_stretch().gap(px(gap))
}

fn half(breakpoint: f32, gap: f32) -> Pixels {
    px((breakpoint - gap) / 2.)
}

pub fn body(
    stats: &ActivityStats,
    range: RangePreset,
    categories: &[Category],
    cx: &App,
) -> AnyElement {
    if stats.session_count == 0 {
        return div()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .p_8()
            .text_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("No activity tracked in this range yet.")
            .into_any_element();
    }
    v_flex()
        .gap(px(GAP_6))
        .child(summary_tiles(stats))
        .child(
            wrapping_row(GAP_6)
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(category_card(stats, range, categories, cx)))
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(top_apps_card(stats))),
        )
        .child(
            wrapping_row(GAP_6)
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(daily_trend_card(stats, range, cx)))
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(motion::enter(
                    ("statistics-time-table", range as usize),
                    time_table_card(stats, cx),
                    cx,
                ))),
        )
        .children(
            timeline_card(stats, cx)
                .map(|card| motion::enter(("statistics-timeline", range as usize), card, cx)),
        )
        .child(motion::enter(
            ("statistics-habits", range as usize),
            div().child(habits_section(stats, cx)),
            cx,
        ))
        .into_any_element()
}

fn cell(breakpoint: f32, gap: f32) -> Div {
    v_flex()
        .flex_grow(1.)
        .flex_shrink(1.)
        .flex_basis(half(breakpoint, gap))
        .min_w_0()
}

/// Two pairs of tiles: one row of four when wide, two rows of two otherwise.
pub(super) fn tile_grid(tiles: Vec<AnyElement>) -> Div {
    let mut tiles = tiles.into_iter();
    let pair = |tiles: &mut dyn Iterator<Item = AnyElement>| {
        cell(TILES_IN_ONE_ROW, GAP_3).child(
            h_flex().gap(px(GAP_3)).items_stretch().children(
                tiles
                    .take(2)
                    .map(|tile| div().flex_1().min_w_0().child(tile)),
            ),
        )
    };
    wrapping_row(GAP_3)
        .child(pair(&mut tiles))
        .child(pair(&mut tiles))
}

fn summary_tiles(stats: &ActivityStats) -> Div {
    let busiest_app = stats
        .apps
        .first()
        .map_or_else(|| "—".to_owned(), |app| app.label.clone());
    let busiest_day =
        busiest_day(&stats.daily).map_or_else(|| "—".to_owned(), |d| format_day(&d.day));
    tile_grid(vec![
        StatTile::new("Total tracked time", format_seconds(stats.total_seconds)).into_any_element(),
        StatTile::new("Sessions", to_locale_string(stats.session_count as f64)).into_any_element(),
        StatTile::new("Busiest app/site", busiest_app).into_any_element(),
        StatTile::new("Busiest day", busiest_day).into_any_element(),
    ])
}

fn category_card(
    stats: &ActivityStats,
    range: RangePreset,
    categories: &[Category],
    cx: &App,
) -> Card {
    let card = Card::new()
        .small()
        .h_full()
        .title("Time per category")
        .description(
            "How tracked time splits across your categories; subcategory time counts towards its parent.",
        );
    let breakdown = category_breakdown(&stats.categories, categories);
    if breakdown.rows.is_empty() {
        return card.child(EmptyState::new(NOTHING_TO_SHOW));
    }
    let muted = cx.theme().muted_foreground;
    let donut = PieChart::new(breakdown.top_level)
        .value(|slice| slice.value as f32)
        .color(|slice| category_color(&slice.color))
        .inner_radius(55.)
        .outer_radius(85.)
        .pad_angle(2f32.to_radians())
        .appear_key(range)
        .tooltip_name(|slice| slice.name.clone().into())
        .tooltip_value(|slice, _, _| format_seconds(slice.value).into());
    let legend = breakdown.rows.into_iter().map(|row| {
        h_flex()
            .justify_between()
            .gap_3()
            .text_sm()
            .pl(px(row.depth as f32 * 14.))
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .size(px(10.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(category_color(&row.color)),
                    )
                    .child(
                        div()
                            .truncate()
                            .when(row.depth > 0, |this| this.text_color(muted))
                            .child(row.name),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(muted)
                    .child(format_seconds(row.value)),
            )
    });
    card.child(
        wrapping_row(GAP_4)
            .justify_center()
            .items_start()
            .child(div().size(px(DONUT_SIZE)).flex_shrink_0().child(donut))
            .child(
                v_flex()
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .flex_basis(px(LEGEND_BESIDE_DONUT - DONUT_SIZE - GAP_4))
                    .min_w_0()
                    .gap(px(6.))
                    .children(legend),
            ),
    )
}

fn top_apps_card(stats: &ActivityStats) -> Card {
    let card = Card::new()
        .small()
        .h_full()
        .title("Top apps & sites")
        .description("Ranked by time spent, tinted by category.");
    if stats.apps.is_empty() {
        return card.child(EmptyState::new(NOTHING_TO_SHOW));
    }
    let max = stats
        .apps
        .iter()
        .map(|app| app.total_seconds)
        .fold(0.0, f64::max);
    card.child(v_flex().gap(px(10.)).children(stats.apps.iter().map(|app| {
        RankedBar::new(
            app.label.clone(),
            bar_fraction(app.total_seconds, max),
            format_seconds(app.total_seconds),
        )
        .color(category_color(
            app.category_color.as_deref().unwrap_or(UNCATEGORIZED_COLOR),
        ))
    })))
}

struct Day {
    label: SharedString,
    seconds: f64,
}

fn daily_trend_card(stats: &ActivityStats, range: RangePreset, cx: &App) -> Card {
    let card = Card::new()
        .small()
        .h_full()
        .title("Daily activity")
        .description("Total tracked time per day.");
    if stats.daily.is_empty() {
        return card.child(EmptyState::new(NOTHING_TO_SHOW));
    }
    let days = stats.daily.iter().map(|day| Day {
        label: format_day(&day.day).into(),
        seconds: day.total_seconds,
    });
    let bar = cx.theme().chart_1;
    let radius = px(4.);
    let chart = BarChart::new(days)
        .band(|day| day.label.clone())
        .value(|day| day.seconds)
        .fill(move |_, _, _, _| bar)
        .corner_radii(Corners {
            top_left: radius,
            top_right: radius,
            bottom_right: px(0.),
            bottom_left: px(0.),
        })
        .max_band_width(px(28.))
        .band_tick_count(stats.daily.len().min(DAY_LABELS))
        .grid_dashed(false)
        .appear_key(range)
        .name("Tracked time")
        .tooltip_title(|day| day.label.clone())
        .tooltip_value(|_, seconds| format_seconds(seconds).into());
    card.child(div().h(px(224.)).w_full().child(chart))
}

pub fn skeleton() -> impl IntoElement {
    let block = |height: f32| Skeleton::new().h(px(height)).rounded_md();
    v_flex()
        .gap(px(GAP_6))
        .child(tile_grid(
            (0..4).map(|_| block(80.).into_any_element()).collect(),
        ))
        .child(
            wrapping_row(GAP_6)
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(block(256.)))
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(block(256.))),
        )
        .child(
            wrapping_row(GAP_6)
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(block(256.)))
                .child(cell(CARDS_SIDE_BY_SIDE, GAP_6).child(block(256.))),
        )
}
