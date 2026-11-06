//! The fields shared by adding and editing a category: color, name, description and parent. Each
//! open builds a fresh form, so it starts from its initial values.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme, Disableable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, Hsla, IntoElement, ParentElement, Render, Rgba, SharedString,
    Styled, Subscription, Task, Window, div, px,
};
use makora_lib::domain::types::{Category, NewCategory};
use makora_lib::shared::ids::CategoryId;

use crate::runtime::window::with_main_window;
use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::categories::category_paths;
use crate::ui::screens::categories::suggestions::offer_recheck;
use crate::ui::widgets::category_combobox::{
    CategoryChoice, CategoryCombobox, CategoryComboboxEvent,
};
use crate::ui::widgets::category_dot::hex_color;

const NO_PARENT: &str = "none";

/// Called with the category just created, in the main window, once `AppState::categories` lists it.
pub type OnCreated = Rc<dyn Fn(&Category, &mut Window, &mut App)>;
/// Called once an edited category is saved.
pub type OnSaved = Rc<dyn Fn(&mut App)>;

pub enum Submit {
    Create { on_created: Option<OnCreated> },
    Update { id: CategoryId, on_saved: Option<OnSaved> },
}

pub struct CategoryForm {
    submit: Submit,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    color: Entity<ColorPickerState>,
    parent: Entity<CategoryCombobox>,
    color_hex: String,
    parent_id: Option<CategoryId>,
    /// The name and description an edited category has now, to tell whether its meaning changes.
    saved_meaning: Option<(String, Option<String>)>,
    pending: bool,
    error: Option<String>,
    save: Task<()>,
    _subscriptions: Vec<Subscription>,
}

/// `#rrggbb`, as a color input gives it.
pub fn hex_of(color: Hsla) -> String {
    let rgb = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgb.r),
        channel(rgb.g),
        channel(rgb.b)
    )
}

/// The trimmed fields, or `None` while the name is blank.
pub fn draft_of(
    name: &str,
    color: &str,
    description: &str,
    parent_id: Option<CategoryId>,
) -> Option<NewCategory> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let description = description.trim();
    Some(NewCategory {
        name: name.to_string(),
        color: color.to_string(),
        description: (!description.is_empty()).then(|| description.to_string()),
        parent_id,
    })
}

impl CategoryForm {
    /// `excluded` are the categories that may not become the parent.
    pub fn new(
        initial: NewCategory,
        excluded: HashSet<CategoryId>,
        submit: Submit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Category name")
                .default_value(initial.name.clone())
        });
        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Description (optional)")
                .auto_grow(2, 6)
                .default_value(initial.description.clone().unwrap_or_default())
        });
        let color = cx.new(|cx| {
            let state = ColorPickerState::new(window, cx);
            match hex_color(&initial.color) {
                Some(color) => state.default_value(color),
                None => state,
            }
        });
        let parent_value = SharedString::from(
            initial
                .parent_id
                .map_or_else(|| NO_PARENT.to_string(), |id| id.0.to_string()),
        );
        let parent = cx.new(|cx| {
            let mut combobox = CategoryCombobox::new(
                Some(parent_value),
                vec![CategoryChoice::new(NO_PARENT, "No parent (top level)")],
                window,
                cx,
            );
            combobox.set_excluded(excluded, window, cx);
            combobox
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                },
            ),
            cx.subscribe(&color, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                this.color_hex = hex_of(*color);
                cx.notify();
            }),
            cx.subscribe(&parent, |this, _, event: &CategoryComboboxEvent, cx| {
                if let CategoryComboboxEvent::Change(value) = event {
                    this.parent_id = value.parse().ok().map(CategoryId);
                    cx.notify();
                }
            }),
        ];
        let saved_meaning = match &submit {
            Submit::Update { id, .. } => AppState::read(cx)
                .categories
                .iter()
                .find(|category| category.id == *id)
                .map(|category| (category.name.clone(), category.description.clone())),
            Submit::Create { .. } => None,
        };
        Self {
            submit,
            name,
            description,
            color,
            parent,
            color_hex: initial.color,
            parent_id: initial.parent_id,
            saved_meaning,
            pending: false,
            error: None,
            save: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    /// Shows the form in a dialog with the name focused.
    pub fn open(form: Entity<Self>, window: &mut Window, cx: &mut App) {
        let name = form.read(cx).name.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(448.)).child(form.clone())
        });
        window.defer(cx, move |window, cx| {
            name.update(cx, |name, cx| name.focus(window, cx));
        });
    }

    fn title(&self, cx: &App) -> String {
        match &self.submit {
            Submit::Update { .. } => "Edit category".into(),
            Submit::Create { .. } => {
                let path = self
                    .parent_id
                    .and_then(|id| category_paths(&AppState::read(cx).categories).remove(&id));
                match path {
                    Some(path) => format!("Add subcategory of {path}"),
                    None => "Add category".into(),
                }
            }
        }
    }

    fn draft(&self, cx: &App) -> Option<NewCategory> {
        draft_of(
            &self.name.read(cx).value(),
            &self.color_hex,
            &self.description.read(cx).value(),
            self.parent_id,
        )
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let Some(draft) = self.draft(cx) else {
            return;
        };
        self.pending = true;
        self.error = None;
        let update = match &self.submit {
            Submit::Update { id, .. } => Some(*id),
            Submit::Create { .. } => None,
        };
        let meaning_changed = self
            .saved_meaning
            .as_ref()
            .is_some_and(|(name, description)| *name != draft.name || *description != draft.description);
        self.save = data::fetch_in(
            window,
            cx,
            move |core| match update {
                Some(id) => core.categories.update(id, &draft).map_err(UiError::from),
                None => core.categories.create(&draft).map_err(UiError::from),
            },
            move |this, result, window, cx| {
                this.pending = false;
                match result {
                    Ok(saved) => {
                        AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
                        window.close_dialog(cx);
                        if meaning_changed {
                            offer_recheck(saved.id, cx);
                        }
                        match &this.submit {
                            Submit::Create {
                                on_created: Some(on_created),
                            } => when_listed(saved, on_created.clone(), cx),
                            Submit::Update {
                                on_saved: Some(on_saved), ..
                            } => on_saved(cx),
                            _ => {}
                        }
                    }
                    Err(err) => this.error = Some(err.to_string()),
                }
            },
        );
        cx.notify();
    }
}

impl Render for CategoryForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let can_submit = self.draft(cx).is_some() && !self.pending;
        let submit_label = match self.submit {
            Submit::Update { .. } => "Save",
            Submit::Create { .. } => "Add category",
        };

        v_flex()
            .gap_4()
            .child(div().text_lg().font_semibold().pr_6().child(self.title(cx)))
            .child(
                h_flex()
                    .gap_2()
                    .child(ColorPicker::new(&self.color).accessibility_label("Category color"))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.name).aria_label("Category name")),
                    ),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .child(Textarea::new(&self.description).aria_label("Category description"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Describe what belongs here."),
                    ),
            )
            .child(self.parent.clone())
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_xs().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("submit")
                            .primary()
                            .label(submit_label)
                            .disabled(!can_submit)
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}

/// Runs `on_created` in the main window once the reloaded category list holds `category`; drops
/// it if the reload fails.
fn when_listed(category: Category, on_created: OnCreated, cx: &mut App) {
    let slot: Rc<RefCell<Option<Subscription>>> = Rc::default();
    let held = slot.clone();
    let subscription = cx.observe(&AppState::entity(cx), move |app, cx| {
        let state = app.read(cx);
        let listed = state.categories.iter().any(|known| known.id == category.id);
        if !listed && state.categories_error.is_none() {
            return;
        }
        if let Some(subscription) = held.borrow_mut().take() {
            cx.defer(move |_| drop(subscription));
        }
        if listed {
            let category = category.clone();
            let on_created = on_created.clone();
            with_main_window(cx, move |window, cx| on_created(&category, window, cx));
        }
    });
    *slot.borrow_mut() = Some(subscription);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_hsla() {
        for hex in ["#6366f1", "#000000", "#ffffff", "#9ca3af"] {
            assert_eq!(hex_of(hex_color(hex).unwrap()), hex);
        }
    }

    #[test]
    fn a_blank_name_cannot_be_saved() {
        assert!(draft_of("   ", "#fff", "", None).is_none());
    }

    #[test]
    fn fields_are_trimmed_and_a_blank_description_is_none() {
        let draft = draft_of("  Work ", "#123456", "  ", Some(CategoryId(3))).unwrap();
        assert_eq!(draft.name, "Work");
        assert_eq!(draft.description, None);
        assert_eq!(draft.parent_id, Some(CategoryId(3)));
        let draft = draft_of("Work", "#123456", " Deep focus ", None).unwrap();
        assert_eq!(draft.description.as_deref(), Some("Deep focus"));
    }
}
