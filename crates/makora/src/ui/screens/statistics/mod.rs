//! Where the time went over today, the last 7 or the last 30 days.

mod breakdown;
mod sections;

use std::collections::HashMap;

use chrono::Local;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task,
    Window,
};
use makora_lib::domain::types::ActivityStats;
use makora_lib::reporting::query;
use tracing::warn;

use crate::ui::app_state::{AppState, Revisions};
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::stats_date::{RANGE_PRESETS, RangePreset, compute_range};
use crate::ui::screens::Screen;
use crate::ui::widgets::feedback::ErrorBanner;
use crate::ui::widgets::page_header::PageHeader;
use crate::ui::widgets::segmented::segmented;

/// The revisions the statistics are built from.
fn watched(revisions: &Revisions) -> (u64, u64, u64) {
    (revisions.sessions, revisions.labels, revisions.categories)
}

pub struct StatisticsScreen {
    range: RangePreset,
    /// The last stats loaded for each range, shown again at once when it is picked.
    loaded: HashMap<RangePreset, ActivityStats>,
    /// The range whose stats stay on screen while another one loads.
    previous: Option<RangePreset>,
    loading: bool,
    error: Option<(RangePreset, String)>,
    seen: (u64, u64, u64),
    load: Task<()>,
    _app_state: Subscription,
}

impl Screen for StatisticsScreen {
    type Location = ();

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_state = AppState::entity(cx);
        let seen = watched(&app_state.read(cx).revisions);
        let mut screen = Self {
            range: RangePreset::SevenDays,
            loaded: HashMap::new(),
            previous: None,
            loading: false,
            error: None,
            seen,
            load: Task::ready(()),
            _app_state: cx.observe(&app_state, Self::on_app_state),
        };
        screen.reload(cx);
        screen
    }

    fn shown(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }
}

impl StatisticsScreen {
    fn on_app_state(&mut self, state: Entity<AppState>, cx: &mut Context<Self>) {
        let revisions = watched(&state.read(cx).revisions);
        if revisions != self.seen {
            self.seen = revisions;
            self.reload(cx);
        }
        cx.notify();
    }

    fn select(&mut self, range: RangePreset, cx: &mut Context<Self>) {
        if range == self.range {
            return;
        }
        if self.loaded.contains_key(&self.range) {
            self.previous = Some(self.range);
        }
        self.range = range;
        self.reload(cx);
    }

    /// Loads the selected range, worked out from the time now: the app stays open for days, so
    /// "Today" must mean the day it is now, not the day the screen was built.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let range = self.range;
        let bounds = compute_range(range, Local::now());
        self.loading = true;
        self.load = data::fetch(
            cx,
            move |core| {
                query::activity_stats(&*core.history, &bounds.start, &bounds.end)
                    .map_err(UiError::from)
            },
            move |screen, result, _| {
                screen.loading = false;
                match result {
                    Ok(stats) => {
                        screen.loaded.insert(range, stats);
                        screen.previous = None;
                        screen.error = None;
                    }
                    Err(err) => {
                        warn!(error = %err, "failed to load the statistics");
                        screen.error = Some((range, format!("Failed to load statistics: {err}")));
                    }
                }
            },
        );
        cx.notify();
    }

    /// The stats to draw, whether they are another range's held over while this one loads, and
    /// the range they are for.
    fn displayed(&self) -> Option<(&ActivityStats, bool, RangePreset)> {
        match self.loaded.get(&self.range) {
            Some(stats) => Some((stats, false, self.range)),
            None => self
                .previous
                .and_then(|range| self.loaded.get(&range).map(|stats| (stats, true, range))),
        }
    }

    fn render_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = RANGE_PRESETS
            .iter()
            .position(|(preset, _)| *preset == self.range)
            .unwrap_or(0);
        h_flex()
            .gap_2()
            .child(segmented(
                "statistics-range",
                RANGE_PRESETS
                    .iter()
                    .map(|(_, label)| SharedString::from(*label))
                    .collect(),
                selected,
                cx.listener(|screen, ix: &usize, _, cx| screen.select(RANGE_PRESETS[*ix].0, cx)),
            ))
            .child(
                Button::new("refresh-statistics")
                    .outline()
                    .small()
                    .icon(IconName::RefreshCw)
                    .loading(self.loading)
                    .loading_icon(Icon::new(IconName::RefreshCw))
                    .tooltip("Refresh statistics")
                    .on_click(cx.listener(|screen, _, _, cx| screen.reload(cx))),
            )
    }
}

impl Render for StatisticsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self
            .error
            .as_ref()
            .filter(|(range, _)| *range == self.range)
            .map(|(_, message)| ErrorBanner::new(message.clone()));
        let categories = &AppState::read(cx).categories;
        let body = match self.displayed() {
            Some((stats, held_over, range)) => Some(
                v_flex()
                    .gap_6()
                    .when(held_over, |this| this.opacity(0.6))
                    .child(sections::body(stats, range, categories, cx))
                    .into_any_element(),
            ),
            None if self.loading => Some(sections::skeleton().into_any_element()),
            None => None,
        };
        let controls = self.render_controls(cx);
        v_flex()
            .gap_6()
            .child(PageHeader::new("Statistics", "Where your time went").action(controls))
            .children(error)
            .children(body)
    }
}
