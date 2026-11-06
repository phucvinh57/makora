//! Categories: the taxonomy, the rules that file activity, and the AI with its review queue.

pub(crate) mod ai_section;
pub(crate) mod review;
mod rules;
mod taxonomy;

use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Div, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window, div, px,
};
use makora_lib::domain::types::FeedbackStats;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::motion;
use crate::ui::prefs::Prefs;
use crate::ui::route::CategoriesTab;
use crate::ui::screens::Screen;
use crate::ui::screens::categories::ai_section::AiSection;
use crate::ui::screens::categories::review::ReviewQueue;
use crate::ui::screens::categories::rules::RulesTab;
use crate::ui::widgets::page_header::PageHeader;

const TABS: [CategoriesTab; 3] = [
    CategoriesTab::Taxonomy,
    CategoriesTab::Rules,
    CategoriesTab::Review,
];

pub struct CategoriesScreen {
    pub tab: CategoriesTab,
    rules: Entity<RulesTab>,
    ai: Entity<AiSection>,
    review: Entity<ReviewQueue>,
    stats: Option<FeedbackStats>,
    labels: Option<u64>,
    stats_load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl CategoriesScreen {
    /// Shows `tab` and remembers it for the next run.
    pub fn set_tab(&mut self, tab: CategoriesTab, cx: &mut Context<Self>) {
        self.tab = tab;
        Prefs::update(cx, |prefs| prefs.categories_tab = tab);
        cx.notify();
    }

    fn reload_stats(&mut self, cx: &mut Context<Self>) {
        let labels = AppState::read(cx).revisions.labels;
        if self.labels == Some(labels) {
            return;
        }
        self.labels = Some(labels);
        self.stats_load = data::fetch(
            cx,
            |core| core.labeling.get_feedback_stats(),
            |this, result, cx| match result {
                Ok(stats) => {
                    let pending = stats.pending_review;
                    this.stats = Some(stats);
                    this.ai
                        .update(cx, |ai, cx| ai.set_pending_review(Some(pending), cx));
                }
                Err(err) => tracing::warn!(error = %err, "failed to load the AI feedback stats"),
            },
        );
    }
}

impl Screen for CategoriesScreen {
    type Location = CategoriesTab;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rules = cx.new(RulesTab::new);
        let ai = cx.new(|cx| AiSection::new(window, cx));
        let review = cx.new(|cx| ReviewQueue::new(window, cx));
        let subscriptions = vec![
            cx.observe(&AppState::entity(cx), |this, _, cx| this.reload_stats(cx)),
            cx.observe(&rules, |_, _, cx| cx.notify()),
        ];
        let mut screen = Self {
            tab: Prefs::get(cx).categories_tab,
            rules,
            ai,
            review,
            stats: None,
            labels: None,
            stats_load: Task::ready(()),
            _subscriptions: subscriptions,
        };
        screen.reload_stats(cx);
        screen
    }

    fn navigate(&mut self, tab: CategoriesTab, _: &mut Window, cx: &mut Context<Self>) {
        self.set_tab(tab, cx);
    }

    fn shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        AppState::update(cx, |state, cx| state.reload_categories(cx));
        self.labels = None;
        self.reload_stats(cx);
        match self.tab {
            CategoriesTab::Taxonomy => {}
            CategoriesTab::Rules => self.rules.update(cx, |rules, cx| rules.refresh(cx)),
            CategoriesTab::Review => {
                self.ai.update(cx, |ai, cx| ai.refresh(window, cx));
                self.review
                    .update(cx, |review, cx| review.reload(window, cx));
            }
        }
    }
}

fn badge(count: impl std::fmt::Display, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .ml_1p5()
        .rounded_full()
        .bg(theme.primary)
        .px_1p5()
        .text_xs()
        .text_color(theme.primary_foreground)
        .child(count.to_string())
}

impl Render for CategoriesScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = self.stats.as_ref().map_or(0, |stats| stats.pending_review);
        let suggested = self.rules.read(cx).suggestion_count();
        let selected = TABS.iter().position(|tab| *tab == self.tab).unwrap_or(0);

        v_flex()
            .gap_4()
            .child(PageHeader::new(
                "Categories",
                "Organize activity with categories, rules and the AI",
            ))
            .child(
                h_flex().child(
                    TabBar::new("categories-tabs")
                        .segmented()
                        .selected_index(selected)
                        .child(Tab::new().label("Taxonomy"))
                        .child(
                            Tab::new()
                                .label("Rules")
                                .when(suggested > 0, |tab| tab.child(badge(suggested, cx))),
                        )
                        .child(
                            Tab::new()
                                .label("AI review")
                                .when(pending > 0, |tab| tab.child(badge(pending, cx))),
                        )
                        .on_click(cx.listener(|this, ix: &usize, _, cx| {
                            if let Some(tab) = TABS.get(*ix) {
                                this.set_tab(*tab, cx);
                            }
                        })),
                ),
            )
            .child(motion::enter(
                SharedString::from(format!("categories-{:?}", self.tab)),
                div().w_full().child(match self.tab {
                    CategoriesTab::Taxonomy => taxonomy::render(cx).into_any_element(),
                    CategoriesTab::Rules => self.rules.clone().into_any_element(),
                    CategoriesTab::Review => v_flex()
                        .gap_6()
                        .child(self.ai.clone())
                        .child(self.review.clone())
                        .into_any_element(),
                }),
                cx,
            ))
            .pb(px(8.))
    }
}
