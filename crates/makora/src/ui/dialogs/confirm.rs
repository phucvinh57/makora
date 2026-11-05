//! "Are you sure?" with Cancel and a confirm button, red for a destructive action:
//!
//! ```ignore
//! Confirm::new("Delete rule?")
//!     .description("Activities it matched keep their category.")
//!     .confirm_label("Delete")
//!     .destructive()
//!     .open(window, cx, move |window, cx| delete(window, cx));
//! ```

use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::{App, SharedString, Window};

use crate::ui::dialogs::{self, AppDialog};

type OnConfirm = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct Confirm {
    title: SharedString,
    description: Option<SharedString>,
    confirm_label: SharedString,
    destructive: bool,
}

impl Confirm {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            confirm_label: "Confirm".into(),
            destructive: false,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn open(
        self,
        window: &mut Window,
        cx: &mut App,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    ) {
        dialogs::open::<ConfirmDialog>((self, Rc::new(on_confirm)), window, cx);
    }
}

pub struct ConfirmDialog;

impl AppDialog for ConfirmDialog {
    type Data = (Confirm, OnConfirm);

    fn open((confirm, on_confirm): Self::Data, window: &mut Window, cx: &mut App) {
        let variant = if confirm.destructive {
            ButtonVariant::Danger
        } else {
            ButtonVariant::Primary
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let on_confirm = on_confirm.clone();
            let alert = alert
                .title(confirm.title.clone())
                .ok_text(confirm.confirm_label.clone())
                .ok_variant(variant)
                .cancel_text("Cancel")
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    on_confirm(window, cx);
                    true
                });
            match confirm.description.clone() {
                Some(description) => alert.description(description),
                None => alert,
            }
        });
    }
}
