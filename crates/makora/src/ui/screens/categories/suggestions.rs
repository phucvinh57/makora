//! Taxonomy suggestions at the top of the Taxonomy tab: new categories for groups of activities
//! the model found none for, sharper descriptions for categories the model keeps getting wrong,
//! and merges of categories the user keeps swapping between. Accepting one, or changing what a
//! category means, offers a run over just the activities it concerns.

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
use makora_lib::categorization::domain::suggestions::{
    DescribeProposal, MergeProposal, NewCategoryProposal, TaxonomySuggestion,
};
use makora_lib::domain::types::{Category, NewCategory};
use makora_lib::shared::ids::{ActivityId, CategoryId};

use crate::ui::app_state::{AppState, Core};
use crate::ui::data;
use crate::ui::dialogs::add_category::{AddCategoryData, AddCategoryDialog};
use crate::ui::dialogs::confirm::Confirm;
use crate::ui::dialogs::edit_category::{EditCategoryData, EditCategoryDialog};
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

/// "You moved 5 of its 12 AI labels elsewhere lately."
pub fn moved_line(proposal: &DescribeProposal) -> String {
    let noun = if proposal.reviewed == 1 { "label" } else { "labels" };
    format!(
        "You moved {} of its {} AI {noun} elsewhere lately.",
        proposal.corrected, proposal.reviewed
    )
}

/// "They look alike, and you moved activities both ways: 3 into Code, 1 back."
pub fn merge_line(proposal: &MergeProposal, into: &str) -> String {
    format!(
        "They look alike, and you moved activities both ways: {} into {into}, {} back.",
        proposal.moved_out, proposal.moved_in
    )
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
    apply: Task<()>,
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
            apply: Task::ready(()),
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

    /// Saves the proposed description, marks the suggestion accepted and offers a re-check.
    fn apply_description(&mut self, id: i64, proposal: &DescribeProposal, window: &mut Window, cx: &mut Context<Self>) {
        let Some(category) = AppState::read(cx)
            .categories
            .iter()
            .find(|category| category.id == proposal.category_id)
            .cloned()
        else {
            return;
        };
        let draft = NewCategory {
            name: category.name,
            color: category.color,
            description: Some(proposal.proposed.clone()),
            parent_id: category.parent_id,
        };
        let category_id = category.id;
        self.suggestions.retain(|suggestion| suggestion.id != id);
        self.apply = data::fetch_in(
            window,
            cx,
            move |core| {
                core.categories.update(category_id, &draft).map_err(UiError::from)?;
                core.suggestions.accept(id).map_err(UiError::from)
            },
            move |this, result, window, cx| {
                match result {
                    Ok(()) => {
                        AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
                        offer_recheck(category_id, cx);
                    }
                    Err(err) => Toast::error(err.to_string()).show(window, cx),
                }
                this.refresh(cx);
            },
        );
        cx.notify();
    }

    fn render_describe(
        &self,
        id: i64,
        proposal: &DescribeProposal,
        paths: &HashMap<CategoryId, String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(path) = paths.get(&proposal.category_id).cloned() else {
            return div().into_any_element();
        };
        let current = proposal.current.clone().unwrap_or_else(|| "No description".to_string());
        let button = |kind: &str| ElementId::Name(SharedString::from(format!("{kind}-suggestion-{id}")));
        let (apply, edit) = (proposal.clone(), proposal.clone());

        suggestion_row(cx)
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(div().truncate().text_sm().font_medium().child(path))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(moved_line(proposal)),
                            ),
                    )
                    .child(
                        Button::new(button("apply"))
                            .primary()
                            .xsmall()
                            .label("Apply")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_description(id, &apply, window, cx)
                            })),
                    )
                    .child(
                        Button::new(button("edit"))
                            .outline()
                            .xsmall()
                            .label("Edit…")
                            .on_click(move |_, window, cx| edit_description(id, &edit, window, cx)),
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
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .child(div().flex_shrink_0().w_16().text_color(muted).child("Now"))
                    .child(div().flex_1().min_w_0().text_color(muted).child(current)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .child(div().flex_shrink_0().w_16().text_color(muted).child("Suggested"))
                    .child(div().flex_1().min_w_0().child(proposal.proposed.clone())),
            )
            .into_any_element()
    }

    /// Asks first, then folds `from` into `into` and marks the suggestion accepted.
    fn confirm_merge(&mut self, id: i64, proposal: &MergeProposal, window: &mut Window, cx: &mut Context<Self>) {
        let categories = &AppState::read(cx).categories;
        let paths = category_paths(categories);
        let (Some(from), Some(into)) = (paths.get(&proposal.from).cloned(), paths.get(&proposal.into).cloned()) else {
            return;
        };
        let children = categories
            .iter()
            .filter(|category| category.parent_id == Some(proposal.from))
            .count();
        let moved = if children == 0 {
            "Its pinned activities, rules and AI labels".to_string()
        } else {
            let noun = if children == 1 { "subcategory" } else { "subcategories" };
            format!("Its {children} {noun}, pinned activities, rules and AI labels")
        };
        let (from_id, into_id) = (proposal.from, proposal.into);
        let this = cx.entity().downgrade();
        Confirm::new(format!("Merge “{from}” into “{into}”?"))
            .description(format!("{moved} move to “{into}”, then “{from}” is deleted. This cannot be undone."))
            .confirm_label("Merge")
            .destructive()
            .open(window, cx, move |window, cx| {
                let (from, into) = (from.clone(), into.clone());
                let _ = this.update(cx, |this, cx| {
                    this.suggestions.retain(|suggestion| suggestion.id != id);
                    this.apply = data::fetch_in(
                        window,
                        cx,
                        move |core| {
                            core.categories.merge(from_id, into_id).map_err(UiError::from)?;
                            core.suggestions.accept(id).map_err(UiError::from)
                        },
                        move |this, result, window, cx| {
                            match result {
                                Ok(()) => {
                                    AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
                                    Toast::success(format!("Merged “{from}” into “{into}”")).show(window, cx);
                                }
                                Err(err) => Toast::error(err.to_string()).show(window, cx),
                            }
                            this.refresh(cx);
                        },
                    );
                    cx.notify();
                });
            });
    }

    fn render_merge(
        &self,
        id: i64,
        proposal: &MergeProposal,
        paths: &HashMap<CategoryId, String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let (Some(from), Some(into)) = (paths.get(&proposal.from), paths.get(&proposal.into)) else {
            return div().into_any_element();
        };
        let button = |kind: &str| ElementId::Name(SharedString::from(format!("{kind}-suggestion-{id}")));
        let merge = proposal.clone();

        suggestion_row(cx)
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .text_sm()
                            .child(div().truncate().font_medium().child(from.clone()))
                            .child(div().flex_shrink_0().text_color(muted).child("into"))
                            .child(div().truncate().font_medium().child(into.clone())),
                    )
                    .child(
                        Button::new(button("merge"))
                            .primary()
                            .xsmall()
                            .label("Merge…")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_merge(id, &merge, window, cx)
                            })),
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
            .child(div().text_xs().text_color(muted).child(merge_line(proposal, into)))
            .into_any_element()
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

/// Opens the edit dialog with the proposed description in place; once saved, marks the suggestion
/// accepted.
fn edit_description(id: i64, proposal: &DescribeProposal, window: &mut Window, cx: &mut App) {
    let Some(mut category) = AppState::read(cx)
        .categories
        .iter()
        .find(|category| category.id == proposal.category_id)
        .cloned()
    else {
        return;
    };
    category.description = Some(proposal.proposed.clone());
    let data = EditCategoryData::from(category).on_saved(move |cx| {
        let core = Core::get(cx);
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = core.suggestions.accept(id) {
                    tracing::warn!(error = %err, "could not mark the suggestion accepted");
                }
            })
            .detach();
    });
    dialogs::open::<EditCategoryDialog>(data, window, cx);
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
        let mut new_rows: Vec<AnyElement> = Vec::new();
        let mut describe_rows: Vec<AnyElement> = Vec::new();
        let mut merge_rows: Vec<AnyElement> = Vec::new();
        for view in self.suggestions.clone() {
            match &view.suggestion {
                TaxonomySuggestion::New(proposal) => new_rows.push(self.render_new(view.id, proposal, &paths, cx)),
                TaxonomySuggestion::Describe(proposal) => {
                    describe_rows.push(self.render_describe(view.id, proposal, &paths, cx))
                }
                TaxonomySuggestion::Merge(proposal) => merge_rows.push(self.render_merge(view.id, proposal, &paths, cx)),
            }
        }
        v_flex()
            .gap_3()
            .when(!new_rows.is_empty(), |this| {
                this.child(
                    SuggestionCard::new(
                        "Suggested categories",
                        "From activity the AI could not file under any category. Creating one does \
                         not file anything until you say so.",
                    )
                    .children(new_rows),
                )
            })
            .when(!describe_rows.is_empty(), |this| {
                this.child(
                    SuggestionCard::new(
                        "Sharper descriptions",
                        "The AI files by name and description, and keeps filing activities here that \
                         you move elsewhere. A clearer description helps it.",
                    )
                    .children(describe_rows),
                )
            })
            .when(!merge_rows.is_empty(), |this| {
                this.child(
                    SuggestionCard::new(
                        "Categories to merge",
                        "Their activities look alike, and you keep moving activities between them.",
                    )
                    .children(merge_rows),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_merge_line_says_how_activities_moved() {
        let proposal = MergeProposal {
            from: CategoryId(1),
            into: CategoryId(2),
            similarity: 0.95,
            moved_out: 3,
            moved_in: 1,
        };
        assert_eq!(
            merge_line(&proposal, "Code"),
            "They look alike, and you moved activities both ways: 3 into Code, 1 back."
        );
    }

    #[test]
    fn the_moved_line_counts_labels() {
        let proposal = DescribeProposal {
            category_id: CategoryId(1),
            current: None,
            proposed: "Sharper.".into(),
            corrected: 5,
            reviewed: 12,
        };
        assert_eq!(moved_line(&proposal), "You moved 5 of its 12 AI labels elsewhere lately.");
    }

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
