use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use chrono::{NaiveDate, TimeDelta};
use gpui_kit::component::chart::BarChart;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::{App, Corners, IntoElement, ParentElement, SharedString, Styled, Window, div, px};
use makora_lib::categorization::application::usage::DailyModelTokens;
use makora_lib::settings::ModelPrice;

use crate::ui::format::ai_usage::{estimate_cost, find_price, format_cost};
use crate::ui::format::number::to_fixed;
use crate::ui::screens::insights::usage::UsagePeriod;

/// A day on the chart's axis: one bar per calendar day, labelled `Nov 5`.
#[derive(Debug, Clone)]
pub struct DayBand {
    pub day: NaiveDate,
    pub label: SharedString,
}

impl PartialEq for DayBand {
    fn eq(&self, other: &Self) -> bool {
        self.day == other.day
    }
}

impl Eq for DayBand {}

impl Hash for DayBand {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.day.hash(state);
    }
}

impl From<DayBand> for SharedString {
    fn from(band: DayBand) -> Self {
        band.label
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CostPoint {
    pub band: DayBand,
    pub cost: f64,
    /// The priced models that day, the costliest first.
    pub models: Vec<(String, f64)>,
}

/// Every day from the period's first to `today`, so days without runs show as gaps.
fn day_keys(daily: &[DailyModelTokens], days: Option<u32>, today: NaiveDate) -> Vec<NaiveDate> {
    let first = match days {
        Some(days) => today - TimeDelta::days(days.into()),
        None => daily
            .first()
            .and_then(|row| NaiveDate::parse_from_str(&row.day, "%Y-%m-%d").ok())
            .unwrap_or(today),
    };
    first.iter_days().take_while(|day| *day <= today).collect()
}

pub fn cost_points(
    daily: &[DailyModelTokens],
    prices: &[ModelPrice],
    days: Option<u32>,
    today: NaiveDate,
) -> Vec<CostPoint> {
    let mut by_day: BTreeMap<&str, BTreeMap<&str, f64>> = BTreeMap::new();
    for row in daily {
        let Some(price) = find_price(prices, Some(&row.provider), &row.model) else {
            continue;
        };
        *by_day
            .entry(row.day.as_str())
            .or_default()
            .entry(row.model.as_str())
            .or_default() += estimate_cost(row, price);
    }
    day_keys(daily, days, today)
        .into_iter()
        .map(|day| {
            let key = day.format("%Y-%m-%d").to_string();
            let mut models: Vec<(String, f64)> = by_day
                .get(key.as_str())
                .map(|models| {
                    models
                        .iter()
                        .map(|(name, cost)| ((*name).to_owned(), *cost))
                        .collect()
                })
                .unwrap_or_default();
            models.sort_by(|a, b| b.1.total_cmp(&a.1));
            CostPoint {
                band: DayBand {
                    day,
                    label: day.format("%b %-d").to_string().into(),
                },
                cost: models.iter().map(|(_, cost)| cost).sum(),
                models,
            }
        })
        .collect()
}

pub fn format_tick(value: f64) -> String {
    if value == 0.0 {
        "$0".to_owned()
    } else if value < 0.01 {
        format!("${}", to_fixed(value, 3))
    } else {
        format_cost(Some(value))
    }
}

pub fn cost_chart(points: Vec<CostPoint>, period: UsagePeriod, cx: &App) -> impl IntoElement {
    let color = cx.theme().chart_1;
    let count = points.len();
    div().h(px(176.)).w_full().child(
        BarChart::new(points)
            .band(|point: &CostPoint| point.band.clone())
            .value(|point: &CostPoint| point.cost)
            .fill(move |_, _, _, _| color)
            .value_axis(true)
            .value_tick_format(format_tick)
            .band_tick_count(count.min(8))
            .max_band_width(px(24.))
            .appear_key(period)
            .corner_radii(Corners {
                top_left: px(4.),
                top_right: px(4.),
                bottom_left: px(0.),
                bottom_right: px(0.),
            })
            .tooltip_content(|point: &CostPoint, _: &mut Window, cx: &mut App| {
                cost_tooltip(point, cx).into_any_element()
            }),
    )
}

fn cost_tooltip(point: &CostPoint, cx: &App) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .min_w(px(144.))
        .gap_1()
        .text_xs()
        .child(
            h_flex()
                .justify_between()
                .gap_4()
                .font_medium()
                .child(point.band.label.clone())
                .child(format_cost(Some(point.cost))),
        )
        .children(point.models.iter().map(|(name, cost)| {
            h_flex()
                .justify_between()
                .gap_4()
                .text_color(muted)
                .child(div().max_w(px(192.)).truncate().child(name.clone()))
                .child(
                    div()
                        .text_color(cx.theme().foreground)
                        .child(format_cost(Some(*cost))),
                )
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn tokens(day: &str, model: &str, input: u64) -> DailyModelTokens {
        DailyModelTokens {
            day: day.into(),
            provider: "openai".into(),
            model: model.into(),
            input_tokens: input,
            cached_input_tokens: 0,
            output_tokens: 0,
        }
    }

    fn price(model: &str) -> ModelPrice {
        ModelPrice {
            provider: "openai".into(),
            model: model.into(),
            input: 1.0,
            cached_input: None,
            output: 1.0,
        }
    }

    #[test]
    fn fills_every_day_of_the_period() {
        let points = cost_points(&[], &[], Some(3), day(5));
        let labels: Vec<&str> = points.iter().map(|p| p.band.label.as_ref()).collect();
        assert_eq!(labels, ["Nov 2", "Nov 3", "Nov 4", "Nov 5"]);
        assert!(points.iter().all(|p| p.cost == 0.0 && p.models.is_empty()));
    }

    #[test]
    fn all_time_starts_at_the_first_day_with_runs() {
        let daily = [tokens("2026-11-03", "a", 1)];
        let points = cost_points(&daily, &[], None, day(5));
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].band.day, day(3));
        assert_eq!(cost_points(&[], &[], None, day(5)).len(), 1);
    }

    #[test]
    fn sums_priced_models_per_day_costliest_first() {
        let daily = [
            tokens("2026-11-05", "a", 1_000_000),
            tokens("2026-11-05", "b", 3_000_000),
            tokens("2026-11-05", "unpriced", 9_000_000),
            tokens("2026-11-04", "a", 500_000),
        ];
        let points = cost_points(&daily, &[price("a"), price("b")], Some(1), day(5));
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].cost, 0.5);
        assert_eq!(points[1].cost, 4.0);
        assert_eq!(
            points[1].models,
            vec![("b".to_owned(), 3.0), ("a".to_owned(), 1.0)]
        );
    }

    #[test]
    fn ticks_read_as_dollars() {
        assert_eq!(format_tick(0.0), "$0");
        assert_eq!(format_tick(0.005), "$0.005");
        assert_eq!(format_tick(0.25), "$0.25");
        assert_eq!(format_tick(150.0), "$150");
    }
}
