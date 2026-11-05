//! A category dropdown you can type into. The categories come from [`AppState`] and stay current;
//! `extra` rows ("Uncategorized", "All categories"…) go first. With [`CategoryCombobox::creatable`]
//! the list ends in a "New category" button, which closes the list and emits
//! [`CategoryComboboxEvent::Create`] for the caller to open the add-category dialog.
//!
//! ```ignore
//! let picker = cx.new(|cx| CategoryCombobox::new(Some(id.into()), vec![], window, cx).creatable());
//! cx.subscribe_in(&picker, window, |this, _, event, window, cx| match event {
//!     CategoryComboboxEvent::Change(value) => this.category = value.clone(),
//!     CategoryComboboxEvent::Create => open_add_category(window, cx),
//! })
//! ```

use gpui_kit::base::actions::Cancel;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::combobox::{Combobox, ComboboxEvent, ComboboxState};
use gpui_kit::component::searchable_list::{SearchableListItem, SearchableVec};
use gpui_kit::component::{ActiveTheme, Icon, IconName, IndexPath, Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window, div, px,
};
use makora_lib::domain::types::Category;

use crate::ui::app_state::AppState;
use crate::ui::format::categories::{category_paths, ordered_categories};
use crate::ui::widgets::category_dot::CategoryDot;

pub const INDENT_PER_LEVEL: f32 = 12.;

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryChoice {
    pub value: SharedString,
    /// The full path, shown on the trigger and searched.
    pub label: SharedString,
    /// The category's own name, for the indented list where `label` would repeat its parents.
    pub name: Option<SharedString>,
    pub color: Option<SharedString>,
    pub depth: usize,
}

impl CategoryChoice {
    /// A row that is not a category, such as "Uncategorized".
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            name: None,
            color: None,
            depth: 0,
        }
    }

    pub fn color(mut self, color: impl Into<SharedString>) -> Self {
        self.color = Some(color.into());
        self
    }
}

/// The categories in tree order, each valued by its id.
pub fn category_choices(categories: &[Category]) -> Vec<CategoryChoice> {
    let paths = category_paths(categories);
    ordered_categories(categories)
        .into_iter()
        .map(|entry| {
            let category = entry.category;
            CategoryChoice {
                value: category.id.to_string().into(),
                label: paths
                    .get(&category.id)
                    .cloned()
                    .unwrap_or_else(|| category.name.clone())
                    .into(),
                name: Some(category.name.clone().into()),
                color: Some(category.color.clone().into()),
                depth: entry.depth,
            }
        })
        .collect()
}

impl SearchableListItem for CategoryChoice {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(
            h_flex()
                .min_w_0()
                .gap_1p5()
                .when_some(self.color.as_ref(), |this, color| {
                    this.child(CategoryDot::new(color))
                })
                .child(div().truncate().child(self.label.clone()))
                .into_any_element(),
        )
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_flex()
            .min_w_0()
            .gap_1p5()
            .pl(px(self.depth as f32 * INDENT_PER_LEVEL))
            .when_some(self.color.as_ref(), |this, color| {
                this.child(CategoryDot::new(color))
            })
            .child(
                div()
                    .truncate()
                    .child(self.name.clone().unwrap_or_else(|| self.label.clone())),
            )
    }

    fn value(&self) -> &SharedString {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CategoryComboboxEvent {
    Change(SharedString),
    Create,
}

type State = ComboboxState<SearchableVec<CategoryChoice>>;

pub struct CategoryCombobox {
    state: Entity<State>,
    extra: Vec<CategoryChoice>,
    choices: Vec<CategoryChoice>,
    value: Option<SharedString>,
    placeholder: SharedString,
    creatable: bool,
    disabled: bool,
    small: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<CategoryComboboxEvent> for CategoryCombobox {}

impl CategoryCombobox {
    pub fn new(
        value: Option<SharedString>,
        extra: Vec<CategoryChoice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let choices = all_choices(&extra, &AppState::read(cx).categories);
        let selected = selected_index(&choices, value.as_ref());
        let state = cx.new(|cx| {
            ComboboxState::new(
                SearchableVec::from(choices.clone()),
                selected.into_iter().collect(),
                window,
                cx,
            )
            .searchable(true)
        });
        let subscriptions = vec![
            cx.subscribe_in(&state, window, |this, _, event, _, cx| {
                if let ComboboxEvent::Change(values) = event
                    && let Some(value) = values.first()
                {
                    this.value = Some(value.clone());
                    cx.emit(CategoryComboboxEvent::Change(value.clone()));
                }
            }),
            cx.observe_in(&AppState::entity(cx), window, |this, app, window, cx| {
                let choices = all_choices(&this.extra, &app.read(cx).categories);
                this.set_choices(choices, window, cx);
            }),
        ];
        Self {
            state,
            extra,
            choices,
            value,
            placeholder: "Choose a category".into(),
            creatable: false,
            disabled: false,
            small: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Ends the list with "New category"; see [`CategoryComboboxEvent::Create`].
    pub fn creatable(mut self) -> Self {
        self.creatable = true;
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn value(&self) -> Option<&SharedString> {
        self.value.as_ref()
    }

    /// Picks `value` without emitting a change; a value matching no row shows the placeholder.
    pub fn set_value(
        &mut self,
        value: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.value = value;
        self.sync_selection(window, cx);
    }

    pub fn set_extra(
        &mut self,
        extra: Vec<CategoryChoice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.extra = extra;
        let choices = all_choices(&self.extra, &AppState::read(cx).categories);
        self.set_choices(choices, window, cx);
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }

    fn set_choices(
        &mut self,
        choices: Vec<CategoryChoice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if choices == self.choices {
            return;
        }
        self.choices = choices;
        let items = SearchableVec::from(self.choices.clone());
        self.state
            .update(cx, |state, cx| state.set_items(items, window, cx));
        self.sync_selection(window, cx);
    }

    fn sync_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values: Vec<SharedString> = self.value.iter().cloned().collect();
        self.state.update(cx, |state, cx| {
            state.set_selected_values(&values, window, cx)
        });
        cx.notify();
    }
}

fn all_choices(extra: &[CategoryChoice], categories: &[Category]) -> Vec<CategoryChoice> {
    extra
        .iter()
        .cloned()
        .chain(category_choices(categories))
        .collect()
}

fn selected_index(choices: &[CategoryChoice], value: Option<&SharedString>) -> Option<IndexPath> {
    let value = value?;
    choices
        .iter()
        .position(|choice| &choice.value == value)
        .map(|row| IndexPath::default().row(row))
}

impl Render for CategoryCombobox {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let muted = cx.theme().muted_foreground;
        Combobox::new(&self.state)
            .placeholder(self.placeholder.clone())
            .search_placeholder("Search categories…")
            .menu_width(px(224.))
            .disabled(self.disabled)
            .when(self.small, |this| this.small())
            .render_trigger(move |trigger, _, _| {
                let body = match trigger.selection().first() {
                    Some((_, choice)) => choice
                        .display_title()
                        .unwrap_or_else(|| choice.title().into_any_element()),
                    None => div()
                        .text_color(muted)
                        .child(trigger.placeholder().cloned().unwrap_or_default())
                        .into_any_element(),
                };
                h_flex()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(body))
                    .child(Icon::new(IconName::ChevronDown).small().text_color(muted))
            })
            .empty(move |_, _| {
                div()
                    .px_2()
                    .py_3()
                    .text_center()
                    .text_sm()
                    .text_color(muted)
                    .child("No category found.")
            })
            .when(self.creatable, |combobox| {
                combobox.footer(move |_, _| {
                    let this = this.clone();
                    Button::new("new-category")
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .icon(IconName::Plus)
                        .label("New category")
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(Box::new(Cancel), cx);
                            let _ = this.update(cx, |_, cx| cx.emit(CategoryComboboxEvent::Create));
                        })
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use makora_lib::shared::ids::CategoryId;

    use super::*;

    fn category(id: i64, name: &str, parent: Option<i64>) -> Category {
        Category {
            id: CategoryId(id),
            name: name.into(),
            color: "#123456".into(),
            parent_id: parent.map(CategoryId),
            description: None,
        }
    }

    #[test]
    fn choices_follow_the_tree_with_full_paths() {
        let categories = [
            category(2, "Code", Some(1)),
            category(1, "Work", None),
            category(3, "Leisure", None),
        ];
        let choices = category_choices(&categories);
        let rows: Vec<_> = choices
            .iter()
            .map(|c| (c.value.as_ref(), c.label.as_ref(), c.depth))
            .collect();
        assert_eq!(
            rows,
            [
                ("1", "Work", 0),
                ("2", "Work / Code", 1),
                ("3", "Leisure", 0)
            ]
        );
        assert_eq!(choices[1].name.as_deref(), Some("Code"));
        assert_eq!(choices[1].color.as_deref(), Some("#123456"));
    }

    #[test]
    fn extra_rows_come_first() {
        let extra = vec![CategoryChoice::new("uncategorized", "Uncategorized")];
        let choices = all_choices(&extra, &[category(1, "Work", None)]);
        assert_eq!(choices[0].value.as_ref(), "uncategorized");
        assert_eq!(choices[1].value.as_ref(), "1");
        assert_eq!(
            selected_index(&choices, Some(&"1".into())).map(|ix| ix.row),
            Some(1)
        );
        assert_eq!(selected_index(&choices, Some(&"9".into())), None);
    }

    #[test]
    fn search_matches_the_path() {
        let choices = category_choices(&[category(1, "Work", None), category(2, "Code", Some(1))]);
        assert!(choices[1].matches("work"));
        assert!(!choices[0].matches("code"));
    }
}
