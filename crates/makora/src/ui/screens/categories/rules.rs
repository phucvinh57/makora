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
use makora_lib::categorization::application::rules::CategoryRuleView;
use makora_lib::categorization::domain::rules::{PatternField, RuleKind};
use makora_lib::shared::ids::CategoryId;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs::rule::RuleDialog;
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

pub struct RulesTab {
    rules: Option<Vec<CategoryRuleView>>,
    error: Option<String>,
    revision: Option<u64>,
    deleting: bool,
    load: Task<()>,
    delete: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl RulesTab {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&AppState::entity(cx), |this, _, cx| this.reload(cx))];
        let mut tab = Self {
            rules: None,
            error: None,
            revision: None,
            deleting: false,
            load: Task::ready(()),
            delete: Task::ready(()),
            _subscriptions: subscriptions,
        };
        tab.reload(cx);
        tab
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
                    dialogs::open::<RuleDialog>(Some(edit.clone()), window, cx)
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

        v_flex()
            .gap_4()
            .child(how_it_works(cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(ErrorBanner::new(error))
            })
            .child(
                h_flex().justify_end().child(
                    Button::new("add-rules")
                        .primary()
                        .small()
                        .label("Add rules")
                        .disabled(loading || no_categories)
                        .on_click(|_, window, cx| dialogs::open::<RuleDialog>(None, window, cx)),
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
    fn usage_counts_activities_and_time() {
        assert_eq!(rule_usage(1, 59.), "1 activity · 59s");
        assert_eq!(rule_usage(3, 3600.), "3 activities · 1h 0m");
    }
}
