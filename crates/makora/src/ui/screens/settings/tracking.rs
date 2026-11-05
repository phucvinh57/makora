use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Subscription, Task, Window, div, px,
};

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::widgets::card::Card;

/// What leaving the field does with the text typed into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftAction {
    /// Show the saved value again: the text is no whole number, or is the saved one.
    Revert,
    Save(i64),
}

pub fn draft_action(draft: &str, saved: i64) -> DraftAction {
    let draft = draft.trim();
    let seconds = draft.parse::<i64>().ok().or_else(|| {
        draft
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && value.fract() == 0.0)
            .map(|value| value as i64)
    });
    match seconds {
        Some(seconds) if seconds != saved => DraftAction::Save(seconds),
        _ => DraftAction::Revert,
    }
}

pub struct TrackingSection {
    input: Entity<InputState>,
    saved: i64,
    editing: bool,
    saving: bool,
    error: Option<String>,
    save: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl TrackingSection {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = AppState::read(cx).settings.tracking.min_session_seconds;
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(saved.to_string())
                .validate(|text, _| text.chars().all(|c| c.is_ascii_digit()))
        });
        let subscriptions = vec![
            cx.subscribe_in(&input, window, Self::on_input_event),
            cx.observe_in(&AppState::entity(cx), window, Self::on_state_changed),
        ];
        Self {
            input,
            saved,
            editing: false,
            saving: false,
            error: None,
            save: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    fn on_state_changed(
        &mut self,
        state: Entity<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = state.read(cx).settings.tracking.min_session_seconds;
        if saved != self.saved {
            self.saved = saved;
            if !self.editing {
                self.show_saved(window, cx);
            }
        }
    }

    fn on_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.editing = true;
                self.error = None;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {
                self.commit(window, cx);
                window.blur(cx);
            }
            InputEvent::Blur => self.commit(window, cx),
            _ => {}
        }
    }

    fn show_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = self.saved.to_string();
        self.input
            .update(cx, |input, cx| input.set_value(saved, window, cx));
        cx.notify();
    }

    fn revert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = false;
        self.error = None;
        self.show_saved(window, cx);
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing || self.saving {
            return;
        }
        let draft = self.input.read(cx).value();
        let seconds = match draft_action(&draft, self.saved) {
            DraftAction::Revert => return self.revert(window, cx),
            DraftAction::Save(seconds) => seconds,
        };
        self.saving = true;
        cx.notify();
        self.save = data::fetch_in(
            window,
            cx,
            move |core| {
                core.settings
                    .set_min_session_seconds(seconds)
                    .map_err(UiError::from)
            },
            |this, result, window, cx| {
                this.saving = false;
                match result {
                    Ok(settings) => {
                        this.saved = settings.tracking.min_session_seconds;
                        this.revert(window, cx);
                        AppState::update(cx, |state, cx| state.set_settings(settings, cx));
                    }
                    Err(err) => this.error = Some(err.to_string()),
                }
            },
        );
    }
}

impl Render for TrackingSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let invalid = self.error.is_some();

        Card::new().small().title("Tracking").child(
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .justify_between()
                        .gap_4()
                        .child(div().text_sm().child("Ignore sessions shorter than"))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(80.))
                                        .on_action(cx.listener(|this, _: &Escape, window, cx| {
                                            this.revert(window, cx)
                                        }))
                                        .child(
                                            Input::new(&self.input)
                                                .aria_label("Ignore sessions shorter than")
                                                .disabled(self.saving)
                                                .when(invalid, |this| this.border_color(danger)),
                                        ),
                                )
                                .child(div().text_sm().text_color(muted).child("seconds")),
                        ),
                )
                .child(div().text_xs().text_color(muted).child(
                    "Activity shorter than this is discarded when it ends. Already recorded \
                     history is kept.",
                ))
                .when_some(self.error.clone(), |this, error| {
                    this.child(div().text_xs().text_color(danger).child(error))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_whole_number_is_saved() {
        assert_eq!(draft_action("45", 5), DraftAction::Save(45));
        assert_eq!(draft_action(" 400 ", 5), DraftAction::Save(400));
        assert_eq!(draft_action("10.0", 5), DraftAction::Save(10));
    }

    #[test]
    fn blank_fractional_or_unchanged_text_reverts() {
        assert_eq!(draft_action("", 5), DraftAction::Revert);
        assert_eq!(draft_action("  ", 5), DraftAction::Revert);
        assert_eq!(draft_action("2.5", 5), DraftAction::Revert);
        assert_eq!(draft_action("abc", 5), DraftAction::Revert);
        assert_eq!(draft_action("5", 5), DraftAction::Revert);
    }
}
