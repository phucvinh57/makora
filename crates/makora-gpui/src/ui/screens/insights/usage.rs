use std::collections::HashMap;

use chrono::{Local, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, ElementId, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window, div, px,
};
use makora_lib::categorization::application::usage::{AiRunView, AiUsageSummary, ModelUsage};
use makora_lib::categorization::domain::usage::{RunStatus, RunTrigger};
use makora_lib::settings::ModelPrice;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs::{self, model_price::ModelPriceData, model_price::ModelPriceDialog};
use crate::ui::error::UiError;
use crate::ui::format::ai_usage::{
    average_latency_ms, cache_hit_ratio, estimate_cost, find_price, format_cost, format_cost_exact,
    format_ms, format_percent, format_rate, format_tokens, items_per_minute,
    output_tokens_per_second, overall_output_tokens_per_second, speed_detail,
};
use crate::ui::format::date_range::parse_instant;
use crate::ui::prefs::Prefs;
use crate::ui::screens::insights::cost_chart::{cost_chart, cost_points};
use crate::ui::screens::insights::segmented;
use crate::ui::shell::{SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_WIDTH};
use crate::ui::widgets::stat_tile::StatTile;

const RECENT_RUNS: usize = 8;
/// Below this content width the six tiles wrap into two rows of three.
const WIDE_CONTENT: f32 = 896.;
const NUMERIC_WIDTH: f32 = 88.;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UsagePeriod {
    Week,
    #[default]
    Month,
    All,
}

impl UsagePeriod {
    const ALL: [UsagePeriod; 3] = [UsagePeriod::Week, UsagePeriod::Month, UsagePeriod::All];

    fn label(self) -> &'static str {
        match self {
            Self::Week => "7d",
            Self::Month => "30d",
            Self::All => "All",
        }
    }

    pub fn days(self) -> Option<u32> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::All => None,
        }
    }
}

/// What the priced remote models cost, and how many remote models have no price.
pub fn total_cost(models: &[ModelUsage], prices: &[ModelPrice]) -> (Option<f64>, usize) {
    let mut cost = None;
    let mut unpriced = 0;
    for model in models {
        let Some(provider) = model.provider.as_deref() else {
            continue;
        };
        match find_price(prices, Some(provider), &model.model) {
            Some(price) => *cost.get_or_insert(0.0) += estimate_cost(&model.totals, price),
            None => unpriced += 1,
        }
    }
    (cost, unpriced)
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn cost_detail(has_remote: bool, unpriced: usize) -> String {
    if !has_remote {
        "local model only".to_owned()
    } else if unpriced > 0 {
        format!(
            "{unpriced} {} no price",
            if unpriced == 1 {
                "model has"
            } else {
                "models have"
            }
        )
    } else {
        "remote models".to_owned()
    }
}

pub struct UsageTab {
    period: UsagePeriod,
    summary: Option<AiUsageSummary>,
    summary_error: Option<String>,
    runs: Option<Vec<AiRunView>>,
    revision: u64,
    summary_load: Task<()>,
    runs_load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl UsageTab {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = AppState::entity(cx);
        let subscriptions = vec![cx.observe(&state, |tab: &mut Self, state, cx| {
            let revision = state.read(cx).revisions.ai_usage;
            if revision != tab.revision {
                tab.revision = revision;
                tab.load_summary(cx);
                tab.load_runs(cx);
            }
            cx.notify();
        })];
        let mut tab = Self {
            period: UsagePeriod::default(),
            summary: None,
            summary_error: None,
            runs: None,
            revision: AppState::read(cx).revisions.ai_usage,
            summary_load: Task::ready(()),
            runs_load: Task::ready(()),
            _subscriptions: subscriptions,
        };
        tab.load_summary(cx);
        tab.load_runs(cx);
        tab
    }

    fn set_period(&mut self, period: UsagePeriod, cx: &mut Context<Self>) {
        if self.period != period {
            self.period = period;
            self.load_summary(cx);
        }
    }

    fn load_summary(&mut self, cx: &mut Context<Self>) {
        let days = self.period.days();
        self.summary_load = data::fetch(
            cx,
            move |core| {
                core.usage
                    .get_summary(
                        makora_lib::categorization::application::usage::since_days_ago(
                            days,
                            Utc::now(),
                        ),
                    )
                    .map_err(UiError::from)
            },
            |tab, result, _| match result {
                Ok(summary) => {
                    tab.summary = Some(summary);
                    tab.summary_error = None;
                }
                Err(err) => {
                    tab.summary = None;
                    tab.summary_error = Some(err.message);
                }
            },
        );
    }

    fn load_runs(&mut self, cx: &mut Context<Self>) {
        self.runs_load = data::fetch(
            cx,
            |core| core.usage.get_recent_runs(RECENT_RUNS),
            |tab, result, _| match result {
                Ok(runs) => tab.runs = Some(runs),
                Err(err) => tracing::warn!(error = format!("{err:#}"), "failed to load AI runs"),
            },
        );
    }
}

struct Lookup<'a> {
    prices: &'a [ModelPrice],
    provider_names: HashMap<&'a str, &'a str>,
}

impl Lookup<'_> {
    fn source(&self, provider: Option<&str>) -> String {
        match provider {
            None => "Local".to_owned(),
            Some(id) => self
                .provider_names
                .get(id)
                .copied()
                .unwrap_or(id)
                .to_owned(),
        }
    }
}

impl Render for UsageTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = AppState::read(cx);
        let ai = state.settings.ai.clone();
        let lookup = Lookup {
            prices: &ai.prices,
            provider_names: ai
                .providers
                .iter()
                .map(|provider| (provider.id.as_str(), provider.name.as_str()))
                .collect(),
        };
        let muted = cx.theme().muted_foreground;
        let period = self.period;
        let empty = matches!((&self.summary, &self.runs), (Some(summary), Some(runs))
            if runs.is_empty() && summary.totals.runs == 0);

        let header = h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .mr_auto()
                    .text_sm()
                    .text_color(muted)
                    .child("What AI categorization used, how fast it ran and what it cost"),
            )
            .child(segmented(
                "usage-period",
                UsagePeriod::ALL
                    .iter()
                    .map(|period| SharedString::from(period.label()))
                    .collect(),
                UsagePeriod::ALL
                    .iter()
                    .position(|p| *p == period)
                    .unwrap_or(1),
                cx.listener(|tab: &mut Self, ix: &usize, _, cx| {
                    tab.set_period(UsagePeriod::ALL[*ix], cx);
                    cx.notify();
                }),
                cx,
            ));

        let mut body: Vec<AnyElement> = Vec::new();
        if empty {
            body.push(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .p_8()
                    .text_center()
                    .text_sm()
                    .text_color(muted)
                    .child(
                        "No AI runs yet. Tokens, speed, cache use and cost show up here once \
                         activities have been categorized.",
                    )
                    .into_any_element(),
            );
        } else {
            if let Some(summary) = &self.summary {
                body.push(stat_grid(summary, &lookup, window, cx).into_any_element());
                if !summary.models.is_empty() {
                    body.push(
                        section("By model", model_table(&summary.models, &lookup, cx), cx)
                            .into_any_element(),
                    );
                }
                let has_priced_day = summary
                    .daily
                    .iter()
                    .any(|row| find_price(&ai.prices, Some(&row.provider), &row.model).is_some());
                if has_priced_day {
                    let points = cost_points(
                        &summary.daily,
                        &ai.prices,
                        period.days(),
                        Local::now().date_naive(),
                    );
                    body.push(
                        section("Estimated cost per day", cost_chart(points, cx), cx)
                            .into_any_element(),
                    );
                }
            }
            if let Some(runs) = self.runs.as_ref().filter(|runs| !runs.is_empty()) {
                body.push(
                    section("Recent runs", runs_table(runs, &lookup, cx), cx).into_any_element(),
                );
            }
            body.push(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(
                        "Speed is output tokens per second, timed over decoding for the local \
                         model and over the whole request for a remote one. Costs are estimated \
                         from the prices you set per model, not billed amounts.",
                    )
                    .into_any_element(),
            );
        }

        v_flex()
            .gap_4()
            .child(header)
            .when_some(self.summary_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(cx.theme().danger).child(error))
            })
            .children(body)
    }
}

fn stat_grid(
    summary: &AiUsageSummary,
    lookup: &Lookup,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let totals = &summary.totals;
    let (cost, unpriced) = total_cost(&summary.models, lookup.prices);
    let has_remote = summary.models.iter().any(|model| model.provider.is_some());
    let sidebar = if Prefs::get(cx).sidebar_collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        SIDEBAR_WIDTH
    };
    let content = window.viewport_size().width.as_f32() - sidebar - 48.;
    let columns = if content >= WIDE_CONTENT { 6 } else { 3 };

    div()
        .grid()
        .grid_cols(columns)
        .gap_2()
        .child(
            StatTile::new(
                "Estimated cost",
                if has_remote {
                    format_cost(cost)
                } else {
                    "Free".to_owned()
                },
            )
            .detail(cost_detail(has_remote, unpriced)),
        )
        .child(
            StatTile::new("Tokens in", format_tokens(totals.input_tokens)).detail(plural(
                totals.runs,
                "run",
                "runs",
            )),
        )
        .child(
            StatTile::new("Tokens out", format_tokens(totals.output_tokens))
                .detail(format!("{} requests", totals.requests)),
        )
        .child(
            StatTile::new("Cache hit", format_percent(cache_hit_ratio(totals)))
                .detail("of input tokens"),
        )
        .child(StatTile::new(
            "Output speed",
            format_rate(overall_output_tokens_per_second(&summary.models), "tok/s"),
        ))
        .child(
            StatTile::new(
                "Throughput",
                format_rate(items_per_minute(totals.items, totals.llm_ms), "/min"),
            )
            .detail(format!("{} activities", totals.items)),
        )
}

fn section(label: &'static str, content: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(content)
}

fn table(header: Vec<Cell>, rows: Vec<Vec<Cell>>, cx: &App) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    v_flex()
        .rounded_md()
        .border_1()
        .border_color(border)
        .text_xs()
        .child(
            h_flex()
                .h_8()
                .px_2()
                .gap_2()
                .font_medium()
                .text_color(muted)
                .children(header.into_iter().map(Cell::into_element)),
        )
        .children(rows.into_iter().map(|row| {
            h_flex()
                .px_2()
                .py_2()
                .gap_2()
                .border_t_1()
                .border_color(border)
                .children(row.into_iter().map(Cell::into_element))
        }))
}

/// One table cell: the first column stretches, the rest are fixed and right-aligned.
struct Cell {
    content: AnyElement,
    first: bool,
    tooltip: Option<(ElementId, SharedString)>,
}

impl Cell {
    fn first(content: impl IntoElement) -> Self {
        Self {
            content: content.into_any_element(),
            first: true,
            tooltip: None,
        }
    }

    fn num(content: impl IntoElement) -> Self {
        Self {
            content: content.into_any_element(),
            first: false,
            tooltip: None,
        }
    }

    fn tooltip(mut self, id: impl Into<ElementId>, tooltip: Option<String>) -> Self {
        self.tooltip = tooltip.map(|tooltip| (id.into(), tooltip.into()));
        self
    }

    fn into_element(self) -> AnyElement {
        let cell = div()
            .flex()
            .items_center()
            .map(|this| {
                if self.first {
                    this.flex_1().min_w_0()
                } else {
                    this.flex_shrink_0().w(px(NUMERIC_WIDTH)).justify_end()
                }
            })
            .child(self.content);
        match self.tooltip {
            Some((id, tooltip)) => cell
                .id(id)
                .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                .into_any_element(),
            None => cell.into_any_element(),
        }
    }
}

fn model_name(
    id: impl Into<ElementId>,
    provider: Option<&str>,
    model: &str,
    lookup: &Lookup,
    cx: &App,
) -> impl IntoElement {
    let model = SharedString::from(model.to_owned());
    v_flex()
        .max_w(px(224.))
        .min_w_0()
        .child(
            div()
                .id(id)
                .truncate()
                .tooltip({
                    let model = model.clone();
                    move |window, cx| Tooltip::new(model.clone()).build(window, cx)
                })
                .child(model),
        )
        .child(
            div()
                .truncate()
                .text_color(cx.theme().muted_foreground)
                .child(lookup.source(provider)),
        )
}

fn tokens_cell(input: u64, output: u64, cx: &App) -> impl IntoElement {
    h_flex()
        .child(format_tokens(input))
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child("\u{a0}/\u{a0}"),
        )
        .child(format_tokens(output))
}

fn model_table(models: &[ModelUsage], lookup: &Lookup, cx: &App) -> impl IntoElement {
    let header = vec![
        Cell::first("Model"),
        Cell::num("Runs"),
        Cell::num("Items"),
        Cell::num("Tokens in / out"),
        Cell::num("Speed"),
        Cell::num("Avg latency"),
        Cell::num("Est. cost"),
    ];
    let rows = models
        .iter()
        .enumerate()
        .map(|(ix, usage)| {
            let totals = &usage.totals;
            let local = usage.provider.is_none();
            let latency = average_latency_ms(totals);
            vec![
                Cell::first(model_name(
                    ("model-name", ix),
                    usage.provider.as_deref(),
                    &usage.model,
                    lookup,
                    cx,
                )),
                Cell::num(totals.runs.to_string()),
                Cell::num(totals.items.to_string()),
                Cell::num(tokens_cell(totals.input_tokens, totals.output_tokens, cx)),
                Cell::num(format_rate(
                    output_tokens_per_second(totals, local),
                    "tok/s",
                ))
                .tooltip(("model-speed", ix), speed_detail(totals, local)),
                Cell::num(latency.map_or_else(|| "—".to_owned(), format_ms)),
                Cell::num(cost_cell(ix, usage, lookup, cx)),
            ]
        })
        .collect();
    table(header, rows, cx)
}

fn cost_cell(ix: usize, usage: &ModelUsage, lookup: &Lookup, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let Some(provider) = usage.provider.clone() else {
        return div().text_color(muted).child("Free").into_any_element();
    };
    let price = find_price(lookup.prices, Some(&provider), &usage.model).cloned();
    let data = ModelPriceData {
        provider_name: lookup.source(Some(&provider)),
        provider,
        model: usage.model.clone(),
        price: price.clone(),
    };
    let edit = move |window: &mut Window, cx: &mut App| {
        dialogs::open::<ModelPriceDialog>(data.clone(), window, cx);
    };
    match price {
        None => Button::new(("set-price", ix))
            .link()
            .xsmall()
            .label("Set price")
            .on_click(move |_, window, cx| edit(window, cx))
            .into_any_element(),
        Some(price) => {
            let cost = estimate_cost(&usage.totals, &price);
            let exact = SharedString::from(format_cost_exact(cost));
            h_flex()
                .gap_1()
                .child(
                    div()
                        .id(("model-cost", ix))
                        .tooltip(move |window, cx| Tooltip::new(exact.clone()).build(window, cx))
                        .child(format_cost(Some(cost))),
                )
                .child(
                    Button::new(("edit-price", ix))
                        .ghost()
                        .xsmall()
                        .compact()
                        .icon(Icon::new(IconName::Pencil).xsmall())
                        .tooltip(format!("Edit the price of {}", usage.model))
                        .on_click(move |_, window, cx| edit(window, cx)),
                )
                .into_any_element()
        }
    }
}

fn runs_table(runs: &[AiRunView], lookup: &Lookup, cx: &App) -> impl IntoElement {
    let header = vec![
        Cell::first("When"),
        Cell::first("Model"),
        Cell::num("Items"),
        Cell::num("Tokens in / out"),
        Cell::num("Duration"),
        Cell::num("Speed"),
        Cell::num("Status"),
    ];
    let muted = cx.theme().muted_foreground;
    let rows = runs
        .iter()
        .enumerate()
        .map(|(ix, run)| {
            let usage = &run.usage;
            let local = run.provider.is_none();
            let scheduled = run.trigger == RunTrigger::Scheduled;
            let when = parse_instant(&run.started_at).map_or_else(
                || run.started_at.clone(),
                |at| at.with_timezone(&Local).format("%b %-d, %H:%M").to_string(),
            );
            vec![
                Cell::first(
                    h_flex()
                        .gap_1p5()
                        .child(
                            div()
                                .id(("run-trigger", ix))
                                .flex()
                                .text_color(muted)
                                .tooltip(move |window, cx| {
                                    Tooltip::new(if scheduled {
                                        "Scheduled run"
                                    } else {
                                        "Started by hand"
                                    })
                                    .build(window, cx)
                                })
                                .child(
                                    Icon::new(if scheduled {
                                        IconName::CalendarClock
                                    } else {
                                        IconName::MousePointerClick
                                    })
                                    .xsmall(),
                                ),
                        )
                        .child(when),
                ),
                Cell::first(model_name(
                    ("run-model", ix),
                    run.provider.as_deref(),
                    &run.model,
                    lookup,
                    cx,
                )),
                Cell::num(run.items.to_string())
                    .tooltip(("run-items", ix), Some(format!("{} labeled", run.labeled))),
                Cell::num(tokens_cell(usage.input_tokens, usage.output_tokens, cx)).tooltip(
                    ("run-tokens", ix),
                    Some(format!(
                        "{} in, {} out",
                        usage.input_tokens, usage.output_tokens
                    )),
                ),
                Cell::num(format_ms((usage.embed_ms + usage.llm_ms) as f64)),
                Cell::num(format_rate(output_tokens_per_second(usage, local), "tok/s"))
                    .tooltip(("run-speed", ix), speed_detail(usage, local)),
                Cell::num(status_tag(ix, run)),
            ]
        })
        .collect();
    table(header, rows, cx)
}

fn status_tag(ix: usize, run: &AiRunView) -> AnyElement {
    match run.status {
        RunStatus::Completed => Tag::secondary().xsmall().child("Done").into_any_element(),
        RunStatus::Stopped => Tag::secondary()
            .outline()
            .xsmall()
            .child("Stopped")
            .into_any_element(),
        RunStatus::Failed => {
            let error = SharedString::from(
                run.error
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_owned()),
            );
            div()
                .id(("run-error", ix))
                .tooltip(move |window, cx| Tooltip::new(error.clone()).build(window, cx))
                .child(Tag::danger().xsmall().child("Failed"))
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use makora_lib::categorization::application::usage::UsageTotals;

    use super::*;

    fn model(provider: Option<&str>, name: &str, input: u64) -> ModelUsage {
        ModelUsage {
            provider: provider.map(str::to_owned),
            model: name.to_owned(),
            totals: UsageTotals {
                input_tokens: input,
                ..UsageTotals::default()
            },
        }
    }

    #[test]
    fn total_cost_skips_local_and_counts_unpriced() {
        let prices = vec![ModelPrice {
            provider: "openai".into(),
            model: "gpt".into(),
            input: 2.0,
            cached_input: None,
            output: 8.0,
        }];
        let models = vec![
            model(None, "local", 1_000_000),
            model(Some("openai"), "gpt", 1_000_000),
            model(Some("openai"), "other", 1_000_000),
        ];
        assert_eq!(total_cost(&models, &prices), (Some(2.0), 1));
        assert_eq!(total_cost(&models[..1], &prices), (None, 0));
        assert_eq!(total_cost(&models[2..], &prices), (None, 1));
    }

    #[test]
    fn cost_details() {
        assert_eq!(cost_detail(false, 0), "local model only");
        assert_eq!(cost_detail(true, 0), "remote models");
        assert_eq!(cost_detail(true, 1), "1 model has no price");
        assert_eq!(cost_detail(true, 2), "2 models have no price");
    }
}
