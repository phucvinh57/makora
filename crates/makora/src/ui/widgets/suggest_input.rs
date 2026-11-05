//! A free-text input that offers matching suggestions below it, each with an optional hint on the
//! right. Typing filters by case-insensitive substring; up/down and enter pick one, escape closes
//! the list. The caller sets the suggestions, or loads them with [`SuggestInput::load`].
//!
//! ```ignore
//! let input = cx.new(|cx| SuggestInput::new("App name", window, cx));
//! input.update(cx, |input, cx| input.load(cx, |core| app_suggestions(core)));
//! cx.subscribe(&input, |this, _, SuggestInputEvent::Change(value), cx| this.app = value.clone());
//! ```

use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, Focusable, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Render, SharedString, Styled, Subscription,
    Task, Window, anchored, canvas, deferred, div, px,
};
use makora_lib::app_core::AppCore;

use crate::ui::data;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Suggestion {
    pub value: SharedString,
    pub hint: Option<SharedString>,
    /// More text the query is matched against, besides `value`.
    pub search: Option<SharedString>,
}

impl Suggestion {
    pub fn new(value: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            ..Self::default()
        }
    }

    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn search(mut self, search: impl Into<SharedString>) -> Self {
        self.search = Some(search.into());
        self
    }

    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let haystack = match &self.search {
            Some(search) => format!("{} {search}", self.value),
            None => self.value.to_string(),
        };
        haystack.to_lowercase().contains(&needle)
    }
}

/// The positions in `suggestions` that match `query`.
pub fn filter_suggestions(suggestions: &[Suggestion], query: &str) -> Vec<usize> {
    suggestions
        .iter()
        .enumerate()
        .filter(|(_, suggestion)| suggestion.matches(query))
        .map(|(ix, _)| ix)
        .collect()
}

/// The highlighted row after pressing up (`-1`) or down (`1`), wrapping at the ends.
pub fn step_highlight(current: Option<usize>, len: usize, step: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match current {
        None if step < 0 => len - 1,
        None => 0,
        Some(ix) => (ix as isize + step).rem_euclid(len as isize) as usize,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum SuggestInputEvent {
    Change(SharedString),
}

pub struct SuggestInput {
    input: Entity<InputState>,
    suggestions: Vec<Suggestion>,
    matched: Vec<usize>,
    highlighted: Option<usize>,
    open: bool,
    bounds: Bounds<Pixels>,
    load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SuggestInputEvent> for SuggestInput {}

impl SuggestInput {
    pub fn new(
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.into();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscriptions = vec![cx.subscribe_in(&input, window, Self::on_input_event)];
        Self {
            input,
            suggestions: Vec::new(),
            matched: Vec::new(),
            highlighted: None,
            open: false,
            bounds: Bounds::default(),
            load: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    pub fn value(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }

    /// Replaces the text without emitting a change.
    pub fn set_value(
        &mut self,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = value.into();
        self.input
            .update(cx, |input, cx| input.set_value(value, window, cx));
        self.refilter(cx);
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
    }

    pub fn set_suggestions(&mut self, suggestions: Vec<Suggestion>, cx: &mut Context<Self>) {
        self.suggestions = suggestions;
        self.refilter(cx);
    }

    /// Loads the suggestions from the core off the UI thread.
    pub fn load(
        &mut self,
        cx: &mut Context<Self>,
        load: impl FnOnce(&AppCore) -> Vec<Suggestion> + Send + 'static,
    ) {
        self.load = data::fetch(cx, load, |this, suggestions, cx| {
            this.set_suggestions(suggestions, cx);
        });
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.focus_handle(cx).focus(window, cx);
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.value(cx);
        self.matched = filter_suggestions(&self.suggestions, &query);
        self.highlighted = None;
        cx.notify();
    }

    fn on_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.refilter(cx);
                self.open = true;
                cx.emit(SuggestInputEvent::Change(self.value(cx)));
            }
            InputEvent::Blur => self.close(cx),
            _ => {}
        }
    }

    fn is_showing(&self) -> bool {
        self.open && !self.matched.is_empty()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.highlighted = None;
        cx.notify();
    }

    fn pick(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(suggestion) = self
            .matched
            .get(row)
            .and_then(|&ix| self.suggestions.get(ix))
        else {
            return;
        };
        let value = suggestion.value.clone();
        self.input
            .update(cx, |input, cx| input.set_value(value.clone(), window, cx));
        self.refilter(cx);
        self.close(cx);
        cx.emit(SuggestInputEvent::Change(value));
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        self.open = true;
        self.highlighted = step_highlight(self.highlighted, self.matched.len(), step);
        cx.notify();
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let rows = self.matched.iter().enumerate().map(|(row, &ix)| {
            let suggestion = &self.suggestions[ix];
            let highlighted = self.highlighted == Some(row);
            h_flex()
                .id(("suggestion", row))
                .justify_between()
                .gap_3()
                .rounded_md()
                .px_1p5()
                .py_1()
                .text_sm()
                .when(highlighted, |this| {
                    this.bg(theme.accent).text_color(theme.accent_foreground)
                })
                .hover(|this| this.bg(theme.accent).text_color(theme.accent_foreground))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.pick(row, window, cx);
                    }),
                )
                .child(div().truncate().child(suggestion.value.clone()))
                .when_some(suggestion.hint.clone(), |this, hint| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(hint),
                    )
                })
        });

        div().absolute().top_full().left_0().child(
            deferred(
                anchored().snap_to_window_with_margin(px(8.)).child(
                    v_flex()
                        .id("suggestions")
                        .occlude()
                        .mt_1()
                        .w(self.bounds.size.width)
                        .max_h(px(288.))
                        .p_1()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .text_color(theme.popover_foreground)
                        .shadow_md()
                        .overflow_y_scrollbar()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)))
                        .children(rows),
                ),
            )
            .with_priority(gpui_kit::base::POPUP_PRIORITY),
        )
    }
}

impl Render for SuggestInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        v_flex()
            .w_full()
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                cx.stop_propagation();
                this.step(1, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                cx.stop_propagation();
                this.step(-1, cx);
            }))
            .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                if this.is_showing()
                    && let Some(row) = this.highlighted
                {
                    cx.stop_propagation();
                    this.pick(row, window, cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                if this.is_showing() {
                    cx.stop_propagation();
                    this.close(cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.open = true;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .relative()
                    .child(Input::new(&self.input))
                    .child(
                        canvas(
                            move |bounds, _, cx| entity.update(cx, |this, _| this.bounds = bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .when(self.is_showing(), |this| this.child(self.render_list(cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestions() -> Vec<Suggestion> {
        vec![
            Suggestion::new("Firefox").hint("app"),
            Suggestion::new("github.com").search("GitHub code"),
            Suggestion::new("Slack"),
        ]
    }

    #[test]
    fn an_empty_query_shows_everything() {
        assert_eq!(filter_suggestions(&suggestions(), "  "), [0, 1, 2]);
    }

    #[test]
    fn matching_ignores_case_and_searches_extra_text() {
        assert_eq!(filter_suggestions(&suggestions(), "FIRE"), [0]);
        assert_eq!(filter_suggestions(&suggestions(), "code"), [1]);
        assert_eq!(filter_suggestions(&suggestions(), " sl "), [2]);
        assert!(filter_suggestions(&suggestions(), "zzz").is_empty());
    }

    #[test]
    fn the_highlight_wraps() {
        assert_eq!(step_highlight(None, 3, 1), Some(0));
        assert_eq!(step_highlight(None, 3, -1), Some(2));
        assert_eq!(step_highlight(Some(2), 3, 1), Some(0));
        assert_eq!(step_highlight(Some(0), 3, -1), Some(2));
        assert_eq!(step_highlight(Some(0), 0, 1), None);
    }
}
