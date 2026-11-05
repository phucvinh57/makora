//! Clears every pin and AI label, keeping categories, rules and tracked time.

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Disableable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Task, Window, div, px,
};

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs::AppDialog;
use crate::ui::error::UiError;
use crate::ui::widgets::toast::Toast;

pub struct ResetCategorizationDialog;

impl AppDialog for ResetCategorizationDialog {
    type Data = ();

    fn open(_: (), window: &mut Window, cx: &mut App) {
        let view = cx.new(|_| ResetView {
            pending: false,
            error: None,
            reset: Task::ready(()),
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(448.)).child(view.clone())
        });
    }
}

struct ResetView {
    pending: bool,
    error: Option<String>,
    reset: Task<()>,
}

impl ResetView {
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending = true;
        self.error = None;
        self.reset = data::fetch_in(
            window,
            cx,
            |core| core.classification.reset().map_err(UiError::from),
            |this, result, window, cx| {
                this.pending = false;
                match result {
                    Ok(cleared) => {
                        window.close_dialog(cx);
                        Toast::success("Categorization data reset")
                            .description(format!("{cleared} labels were cleared."))
                            .show(window, cx);
                        AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
                    }
                    Err(err) => this.error = Some(err.to_string()),
                }
            },
        );
        cx.notify();
    }
}

impl Render for ResetView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .pr_6()
                            .child("Reset categorization data?"),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(
                        "This action cannot be undone. It will keep your categories, rules \
                             and tracking history, but delete all manual and AI categorizations.",
                    ))
                    .when_some(self.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(theme.danger).child(error))
                    }),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel")
                            .ghost()
                            .label("Cancel")
                            .disabled(self.pending)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("reset")
                            .danger()
                            .label(if self.pending {
                                "Resetting…"
                            } else {
                                "Reset data"
                            })
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
                    ),
            )
    }
}
