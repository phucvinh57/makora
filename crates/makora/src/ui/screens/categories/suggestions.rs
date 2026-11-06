//! Taxonomy suggestions at the top of the Taxonomy tab: new categories for groups of activities
//! the model found none for. Accepting one, or changing what a category means, offers a run over
//! just the activities it concerns.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, ElementId, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window, div,
};
use makora_lib::categorization::application::ai_run::{RunMode, RunRequest};
use makora_lib::categorization::application::suggestions::SuggestionView;
use makora_lib::categorization::domain::suggestions::{NewCategoryProposal, TaxonomySuggestion};
use makora_lib::domain::types::{Category, NewCategory};
use makora_lib::shared::ids::{ActivityId, CategoryId};

use crate::ui::app_state::{AppState, Core};
use crate::ui::data;
use crate::ui::dialogs::add_category::{AddCategoryData, AddCategoryDialog};
use crate::ui::dialogs::{self};
use crate::ui::error::UiError;
use crate::ui::format::categories::{DEFAULT_CATEGORY_COLOR, category_paths};
use crate::ui::screens::categories::rules::rule_usage;
use crate::ui::widgets::suggestion_card::{SuggestionCard, suggestion_row};
use crate::ui::widgets::toast::Toast;

/// "Group of 12 activities like “Steam – Hades”", for a group the model could not name.
pub fn group_label(proposal: &NewCategoryProposal) -> String {
    let count = proposal.members.len();
    match proposal.sample_titles.first() {
        Some(title) => format!("Group of {count} activities like “{title}”"),
        None => format!("Group of {count} activities"),
    }
}

/// Offers a run over `members`, the activities a just-created category was made for.
pub fn offer_label_now(name: &str, members: Vec<ActivityId>, cx: &mut App) {
    if members.is_empty() {
        return;
    }
    let count = members.len();
    let noun = if count == 1 { "activity" } else { "activities" };
    Toast::success(format!("Created “{name}”"))
        .description(format!("Label the {count} {noun} it was made for now?"))
        .action("Label now", move |_, cx| {
            let request = RunRequest::scoped(RunMode::Label, members.clone());
            AppState::update(cx, |state, cx| state.start_run(request, cx));
        })
        .persistent()
        .show_in_main_window(cx);
}

/// After a category's name or description changed: offers to re-check the activities the AI
/// filed under it, if there are any.
pub fn offer_recheck(category: CategoryId, cx: &mut App) {
    let core = Core::get(cx);
    let job = cx
        .background_executor()
        .spawn(async move { core.labeling.get_ai_filed_ids(category) });
    cx.spawn(async move |cx| {
        let ids = match job.await {
            Ok(ids) => ids,
            Err(err) => {
                tracing::warn!(error = %err, "could not count the AI labels of a category");
                return;
            }
        };
        if ids.is_empty() {
            return;
        }
        cx.update(|cx| {
            let count = ids.len();
            let noun = if count == 1 { "activity" } else { "activities" };
            Toast::info("Category changed")
                .description(format!("Re-check the {count} {noun} the AI filed here?"))
                .action("Re-check", move |_, cx| {
                    let request = RunRequest::scoped(RunMode::Recheck, ids.clone());
                    AppState::update(cx, |state, cx| state.start_run(request, cx));
                })
                .persistent()
                .show_in_main_window(cx);
        });
    })
    .detach();
}

pub struct TaxonomySuggestions {
    suggestions: Vec<SuggestionView>,
    revision: Option<(u64, u64)>,
    load: Task<()>,
    dismiss: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl TaxonomySuggestions {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&AppState::entity(cx), |this, _, cx| this.reload(cx))];
        let mut this = Self {
            suggestions: Vec::new(),
            revision: None,
            load: Task::ready(()),
            dismiss: Task::ready(()),
            _subscriptions: subscriptions,
        };
        this.reload(cx);
        this
    }

    pub fn count(&self) -> usize {
        self.suggestions.len()
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.revision = None;
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let revisions = AppState::read(cx).revisions;
        let revision = (revisions.categories, revisions.labels);
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.load = data::fetch(
            cx,
            |core| core.suggestions.list(),
            |this, result, _| match result {
                Ok(suggestions) => this.suggestions = suggestions,
                Err(err) => tracing::warn!(error = %err, "failed to load taxonomy suggestions"),
            },
        );
    }

    fn dismiss(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.suggestions.retain(|suggestion| suggestion.id != id);
        self.load = Task::ready(());
        self.dismiss = data::fetch_in(
            window,
            cx,
            move |core| core.suggestions.dismiss(id).map_err(UiError::from),
            |this, result, window, cx| {
                if let Err(err) = result {
                    Toast::error(err.to_string()).show(window, cx);
                }
                this.refresh(cx);
            },
        );
        cx.notify();
    }

    fn render_new(
        &self,
        id: i64,
        proposal: &NewCategoryProposal,
        paths: &HashMap<CategoryId, String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let title = proposal.name.clone().unwrap_or_else(|| group_label(proposal));
        let parent = proposal.parent_id.and_then(|parent| paths.get(&parent)).cloned();
        let usage = rule_usage(proposal.members.len() as i64, proposal.seconds);
        let samples = proposal
            .sample_titles
            .iter()
            .map(|title| format!("“{title}”"))
            .collect::<Vec<_>>()
            .join(" · ");
        let create = proposal.clone();
        let button = |kind: &str| ElementId::Name(SharedString::from(format!("{kind}-suggestion-{id}")));

        suggestion_row(cx)
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(div().truncate().text_sm().font_medium().child(title))
                            .when_some(parent, |this, parent| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(format!("under {parent}")),
                                )
                            }),
                    )
                    .child(div().flex_shrink_0().text_xs().text_color(muted).child(usage))
                    .child(
                        Button::new(button("create"))
                            .primary()
                            .xsmall()
                            .label("Create…")
                            .on_click(move |_, window, cx| create_category(id, &create, window, cx)),
                    )
                    .child(
                        Button::new(button("dismiss"))
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .tooltip("Dismiss")
                            .on_click(cx.listener(move |this, _, window, cx| this.dismiss(id, window, cx))),
                    ),
            )
            .when_some(proposal.description.clone(), |this, description| {
                this.child(div().text_xs().child(description))
            })
            .when(!samples.is_empty(), |this| {
                this.child(div().text_xs().text_color(muted).truncate().child(samples))
            })
            .into_any_element()
    }
}

/// Opens the add-category dialog filled in from `proposal`; once the category exists, marks the
/// suggestion accepted and offers to label its activities.
fn create_category(id: i64, proposal: &NewCategoryProposal, window: &mut Window, cx: &mut App) {
    let parent: Option<Category> = proposal.parent_id.and_then(|parent| {
        AppState::read(cx)
            .categories
            .iter()
            .find(|category| category.id == parent)
            .cloned()
    });
    let initial = NewCategory {
        name: proposal.name.clone().unwrap_or_default(),
        color: parent
            .as_ref()
            .map_or_else(|| DEFAULT_CATEGORY_COLOR.to_string(), |parent| parent.color.clone()),
        description: proposal.description.clone(),
        parent_id: parent.as_ref().map(|parent| parent.id),
    };
    let data = AddCategoryData::new(parent).prefill(initial).on_created(move |created, _, cx| {
        let core = Core::get(cx);
        let name = created.name.clone();
        let job = cx.background_executor().spawn(async move { core.suggestions.accept_new(id) });
        cx.spawn(async move |cx| match job.await {
            Ok(members) => cx.update(|cx| offer_label_now(&name, members, cx)),
            Err(err) => tracing::warn!(error = %err, "could not mark the suggestion accepted"),
        })
        .detach();
    });
    dialogs::open::<AddCategoryDialog>(data, window, cx);
}

impl Render for TaxonomySuggestions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.suggestions.is_empty() {
            return div().into_any_element();
        }
        let paths = category_paths(&AppState::read(cx).categories);
        let rows: Vec<AnyElement> = self
            .suggestions
            .clone()
            .iter()
            .filter_map(|view| match &view.suggestion {
                TaxonomySuggestion::New(proposal) => Some(self.render_new(view.id, proposal, &paths, cx)),
                _ => None,
            })
            .collect();
        if rows.is_empty() {
            return div().into_any_element();
        }
        v_flex()
            .child(SuggestionCard::new(
                "Suggested categories",
                "From activity the AI could not file under any category. Creating one does not file \
                 anything until you say so.",
            )
            .children(rows))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unnamed_group_is_called_after_its_first_title() {
        let mut proposal = NewCategoryProposal {
            name: None,
            description: None,
            parent_id: None,
            members: (1..=12).map(ActivityId).collect(),
            seconds: 0.,
            sample_titles: vec!["Steam – Hades".into()],
        };
        assert_eq!(group_label(&proposal), "Group of 12 activities like “Steam – Hades”");
        proposal.sample_titles.clear();
        assert_eq!(group_label(&proposal), "Group of 12 activities");
    }
}
