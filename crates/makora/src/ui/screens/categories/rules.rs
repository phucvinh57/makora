//! The rules that file activity under a category, with how much each one decides.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Context, ElementId, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window, div, px,
};
use makora_lib::categorization::application::rules::{CategoryRuleView, RuleSuggestionView};
use makora_lib::categorization::domain::rules::{PatternField, RuleKind};
use makora_lib::shared::ids::CategoryId;

use crate::ui::app_state::{AppState, Core};
use crate::ui::data;
use crate::ui::dialogs::rule::{RuleDialog, RuleDialogInput};
use crate::ui::dialogs::{self};
use crate::ui::error::UiError;
use crate::ui::format::categories::category_paths;
use crate::ui::format::duration::format_seconds;
use crate::ui::format::rules::{pattern_field_label, rule_kind_label};
use crate::ui::widgets::category_dot::category_color;
use crate::ui::widgets::feedback::ErrorBanner;
use crate::ui::widgets::toast::Toast;

pub fn rule_tag(kind: RuleKind, field: Option<PatternField>) -> &'static str {
    match kind {
        RuleKind::Pattern => pattern_field_label(field.unwrap_or(PatternField::Url)),
        _ => rule_kind_label(kind),
    }
}

pub fn rule_usage(activities: i64, seconds: f64) -> String {
    let noun = if activities == 1 {
        "activity"
    } else {
        "activities"
    };
    format!("{activities} {noun} · {}", format_seconds(seconds))
}

/// "Rule files this under Fun, but you pinned 9 of 10 to Work".
pub fn change_reason(suggestion: &RuleSuggestionView, current: &str, suggested: &str) -> String {
    format!(
        "Rule files this under {current}, but you pinned {} of {} to {suggested}",
        suggestion.activity_count, suggestion.pinned_count
    )
}

/// Adds the suggested rule, or changes the rule it corrects, in the background.
pub(crate) fn apply_suggestion(suggestion: RuleSuggestionView, cx: &mut App) {
    let core = Core::get(cx);
    let job = cx.background_executor().spawn(async move {
        let draft = suggestion.draft();
        match suggestion.change {
            Some((id, _)) => core.rules.update(id, &draft).map(|()| "Rule changed"),
            None => core.rules.create(&draft).map(|_| "Rule added"),
        }
    });
    cx.spawn(async move |cx| {
        let result = job.await.map_err(UiError::from);
        cx.update(|cx| match result {
            Ok(message) => {
                Toast::success(message).show_in_main_window(cx);
                AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
            }
            Err(err) => Toast::error(err.to_string()).show_in_main_window(cx),
        });
    })
    .detach();
}

/// Opens the rule dialog with the suggestion filled in, to adjust it before saving.
fn edit_suggestion(suggestion: &RuleSuggestionView, window: &mut Window, cx: &mut App) {
    let input = match suggestion.change {
        Some((id, _)) => RuleDialogInput::Edit(CategoryRuleView {
            id,
            kind: suggestion.kind,
            field: suggestion.field,
            value: suggestion.value.clone(),
            category_id: suggestion.category_id,
            activity_count: 0,
            total_seconds: 0.,
        }),
        None => RuleDialogInput::Prefill(suggestion.draft()),
    };
    dialogs::open::<RuleDialog>(input, window, cx);
}

pub struct RulesTab {
    rules: Option<Vec<CategoryRuleView>>,
    suggestions: Vec<RuleSuggestionView>,
    error: Option<String>,
    revision: Option<u64>,
    deleting: bool,
    load: Task<()>,
    suggestions_load: Task<()>,
    dismiss: Task<()>,
    delete: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl RulesTab {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&AppState::entity(cx), |this, _, cx| this.reload(cx))];
        let mut tab = Self {
            rules: None,
            suggestions: Vec::new(),
            error: None,
            revision: None,
            deleting: false,
            load: Task::ready(()),
            suggestions_load: Task::ready(()),
            dismiss: Task::ready(()),
            delete: Task::ready(()),
            _subscriptions: subscriptions,
        };
        tab.reload(cx);
        tab
    }

    pub fn suggestion_count(&self) -> usize {
        self.suggestions.len()
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.revision = None;
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let revision = AppState::read(cx).revisions.categories;
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.load = data::fetch(
            cx,
            |core| core.rules.list().map_err(UiError::from),
            |this, result, _| match result {
                Ok(rules) => {
                    this.rules = Some(rules);
                    this.error = None;
                }
                Err(err) => {
                    this.rules.get_or_insert_default();
                    this.error = Some(err.to_string());
                }
            },
        );
        self.suggestions_load = data::fetch(
            cx,
            |core| core.rules.suggestions(),
            |this, result, cx| {
                match result {
                    Ok(suggestions) => this.suggestions = suggestions,
                    Err(err) => tracing::warn!(error = %err, "failed to load rule suggestions"),
                }
                cx.notify();
            },
        );
    }

    fn dismiss(&mut self, suggestion: &RuleSuggestionView, window: &mut Window, cx: &mut Context<Self>) {
        let draft = suggestion.draft();
        let dismissed = draft.clone();
        self.suggestions.retain(|s| {
            !(s.kind == dismissed.kind
                && s.field == dismissed.field
                && s.value == dismissed.value
                && s.category_id == dismissed.category_id)
        });
        self.suggestions_load = Task::ready(());
        self.dismiss = data::fetch_in(
            window,
            cx,
            move |core| core.rules.dismiss_suggestion(&draft).map_err(UiError::from),
            |this, result, window, cx| {
                if let Err(err) = result {
                    Toast::error(err.to_string()).show(window, cx);
                }
                this.refresh(cx);
            },
        );
        cx.notify();
    }

    fn render_suggestion(
        &self,
        ix: usize,
        suggestion: &RuleSuggestionView,
        paths: &HashMap<CategoryId, String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let state = AppState::read(cx);
        let category = state.categories.iter().find(|c| c.id == suggestion.category_id);
        let path = paths
            .get(&suggestion.category_id)
            .cloned()
            .unwrap_or_else(|| "Unknown category".into());
        let theme = cx.theme();
        let dot = category.map_or(theme.muted_foreground, |category| {
            category_color(&category.color)
        });
        let reason = suggestion.change.map(|(_, current)| {
            let current = paths
                .get(&current)
                .map_or("another category", String::as_str);
            change_reason(suggestion, current, &path)
        });
        let add = suggestion.clone();
        let edit = suggestion.clone();
        let dismiss = suggestion.clone();
        let usage = rule_usage(suggestion.activity_count as i64, suggestion.total_seconds);

        v_flex()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .px_3()
            .py_2()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        Tag::secondary()
                            .small()
                            .child(rule_tag(suggestion.kind, suggestion.field)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(theme.mono_font_family.clone())
                            .text_sm()
                            .child(suggestion.value.clone()),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .min_w_0()
                            .gap_1p5()
                            .text_sm()
                            .child(div().size_2().flex_shrink_0().rounded_full().bg(dot))
                            .child(div().truncate().child(path)),
                    )
                    .child(
                        div()
                            .w(px(128.))
                            .flex_shrink_0()
                            .text_right()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(usage),
                    )
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "apply-suggestion-{ix}"
                        ))))
                        .primary()
                        .xsmall()
                        .label(if suggestion.change.is_some() {
                            "Change rule"
                        } else {
                            "Add rule"
                        })
                        .on_click(move |_, _, cx| apply_suggestion(add.clone(), cx)),
                    )
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "edit-suggestion-{ix}"
                        ))))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Pencil)
                        .tooltip("Edit before saving")
                        .on_click(move |_, window, cx| edit_suggestion(&edit, window, cx)),
                    )
                    .child(
                        Button::new(ElementId::Name(SharedString::from(format!(
                            "dismiss-suggestion-{ix}"
                        ))))
                        .ghost()
                        .xsmall()
                        .icon(IconName::X)
                        .tooltip("Dismiss")
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.dismiss(&dismiss, window, cx)),
                        ),
                    ),
            )
            .when_some(reason, |this, reason| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(reason),
                )
            })
    }

    fn render_suggestions(
        &self,
        paths: &HashMap<CategoryId, String>,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.suggestions.is_empty() {
            return None;
        }
        let rows: Vec<_> = self
            .suggestions
            .iter()
            .enumerate()
            .map(|(ix, suggestion)| self.render_suggestion(ix, suggestion, paths, cx))
            .collect();
        let theme = cx.theme();
        Some(
            v_flex()
                .gap_2()
                .rounded_lg()
                .border_1()
                .border_color(theme.primary.opacity(0.4))
                .bg(theme.primary.opacity(0.05))
                .p_3()
                .child(
                    v_flex()
                        .child(div().text_sm().font_medium().child("Suggested rules"))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            "From how you filed activity yourself. Adding one files the rest of \
                             that activity too; your pins stay as they are.",
                        )),
                )
                .children(rows),
        )
    }

    fn delete(&mut self, rule: &CategoryRuleView, window: &mut Window, cx: &mut Context<Self>) {
        let id = rule.id;
        self.deleting = true;
        self.delete = data::fetch_in(
            window,
            cx,
            move |core| core.rules.delete(id).map_err(UiError::from),
            |this, result, window, cx| {
                this.deleting = false;
                match result {
                    Ok(()) => {
                        Toast::success("Rule deleted").show(window, cx);
                        AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
                    }
                    Err(err) => Toast::error(err.to_string()).show(window, cx),
                }
            },
        );
        cx.notify();
    }

    fn render_rule(
        &self,
        rule: &CategoryRuleView,
        paths: &HashMap<CategoryId, String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let state = AppState::read(cx);
        let category = state.categories.iter().find(|c| c.id == rule.category_id);
        let path = paths.get(&rule.category_id).cloned();
        let theme = cx.theme();
        let dot = category.map_or(theme.muted_foreground, |category| {
            category_color(&category.color)
        });
        let id = rule.id.0;
        let edit = rule.clone();
        let delete = rule.clone();

        h_flex()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .px_3()
            .py_2()
            .child(
                Tag::secondary()
                    .small()
                    .child(rule_tag(rule.kind, rule.field)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme.mono_font_family.clone())
                    .text_sm()
                    .child(rule.value.clone()),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .min_w_0()
                    .gap_1p5()
                    .text_sm()
                    .child(div().size_2().flex_shrink_0().rounded_full().bg(dot))
                    .child(
                        div()
                            .truncate()
                            .child(path.unwrap_or_else(|| "Unknown category".into())),
                    ),
            )
            .child(
                div()
                    .w(px(128.))
                    .flex_shrink_0()
                    .text_right()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(rule_usage(rule.activity_count, rule.total_seconds)),
            )
            .child(
                Button::new(ElementId::Name(SharedString::from(format!(
                    "edit-rule-{id}"
                ))))
                .ghost()
                .xsmall()
                .icon(IconName::Pencil)
                .tooltip("Edit rule")
                .on_click(move |_, window, cx| {
                    dialogs::open::<RuleDialog>(RuleDialogInput::Edit(edit.clone()), window, cx)
                }),
            )
            .child(
                Button::new(ElementId::Name(SharedString::from(format!(
                    "delete-rule-{id}"
                ))))
                .ghost()
                .xsmall()
                .icon(IconName::Trash)
                .tooltip("Delete rule")
                .disabled(self.deleting)
                .on_click(cx.listener(move |this, _, window, cx| this.delete(&delete, window, cx))),
            )
    }
}

fn how_it_works(cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let point = |text: &'static str| {
        h_flex()
            .w_full()
            .items_start()
            .gap_2()
            .child(div().flex_shrink_0().child("•"))
            .child(div().flex_1().min_w_0().child(text))
    };
    v_flex()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.3))
        .px_4()
        .py_3()
        .text_sm()
        .child(div().mb_1().font_medium().child("How categorization works"))
        .child(
            v_flex()
                .gap_1()
                .text_color(theme.muted_foreground)
                .child(point(
                    "An activity's category comes from a pin you set, then from a rule, then \
                     from the AI. A rule never overrides a pin, and the AI skips anything a rule \
                     already covers.",
                ))
                .child(point(
                    "A rule files every activity it matches, past and future, under one \
                     category.",
                ))
                .child(point(
                    "So write rules for apps and sites you use for one purpose, like an editor, \
                     a code host or a project tracker. Leave multi-purpose ones to pins and the \
                     AI: AI chats such as claude.ai or chatgpt.com, youtube.com, search engines \
                     and social feeds mix work and leisure. A narrower rule such as a single \
                     channel or project page can still fit.",
                )),
        )
}

impl Render for RulesTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = AppState::read(cx);
        let categories_loading = !state.categories_loaded;
        let no_categories = state.has_no_categories();
        let loading = categories_loading || self.rules.is_none();
        let muted = cx.theme().muted_foreground;
        let rules = self.rules.clone().unwrap_or_default();
        let paths = category_paths(&state.categories);
        let rows: Vec<_> = rules
            .iter()
            .map(|rule| self.render_rule(rule, &paths, cx))
            .collect();
        let suggestions = (!no_categories)
            .then(|| self.render_suggestions(&paths, cx))
            .flatten();

        v_flex()
            .gap_4()
            .child(how_it_works(cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(ErrorBanner::new(error))
            })
            .children(suggestions)
            .child(
                h_flex().justify_end().child(
                    Button::new("add-rules")
                        .primary()
                        .small()
                        .label("Add rules")
                        .disabled(loading || no_categories)
                        .on_click(|_, window, cx| {
                            dialogs::open::<RuleDialog>(RuleDialogInput::Add, window, cx)
                        }),
                ),
            )
            .when(loading, |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .children((0..3).map(|_| Skeleton::new().h_10().rounded_lg())),
                )
            })
            .when(no_categories, |this| {
                this.child(div().text_sm().text_color(muted).child(
                    "Add a category on the Taxonomy tab first; a rule files activity under one.",
                ))
            })
            .when(!loading && rules.is_empty() && !no_categories, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("No rules yet. Use “Add rules” above to begin."),
                )
            })
            .child(v_flex().gap_2().children(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_shows_its_field() {
        assert_eq!(rule_tag(RuleKind::Site, None), "Website");
        assert_eq!(
            rule_tag(RuleKind::Pattern, Some(PatternField::Title)),
            "Window title"
        );
        assert_eq!(rule_tag(RuleKind::Pattern, None), "URL");
    }

    #[test]
    fn a_change_says_where_the_rule_files_and_where_pins_went() {
        let suggestion = RuleSuggestionView {
            kind: RuleKind::Site,
            field: None,
            value: "github.com".into(),
            category_id: CategoryId(1),
            activity_count: 9,
            total_seconds: 3600.,
            pinned_count: 10,
            change: Some((makora_lib::shared::ids::RuleId(3), CategoryId(2))),
        };
        assert_eq!(
            change_reason(&suggestion, "Fun", "Work > Code"),
            "Rule files this under Fun, but you pinned 9 of 10 to Work > Code"
        );
    }

    #[test]
    fn usage_counts_activities_and_time() {
        assert_eq!(rule_usage(1, 59.), "1 activity · 59s");
        assert_eq!(rule_usage(3, 3600.), "3 activities · 1h 0m");
    }
}
