//! Short messages in the window's corner, like the web UI's sonner toasts:
//!
//! ```ignore
//! Toast::success("Category saved").show(window, cx);
//! Toast::error(err.to_string()).persistent().show(window, cx);
//! Toast::success("Moved 3 sessions").action("Undo", move |window, cx| undo(window, cx)).show(window, cx);
//! ```

use std::rc::Rc;

use gpui_kit::component::Sizable;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::{App, SharedString, Window};

use crate::runtime::window::with_main_window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
    Warning,
}

type ToastAction = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub struct Toast {
    kind: ToastKind,
    message: SharedString,
    description: Option<SharedString>,
    action: Option<(SharedString, ToastAction)>,
    persistent: bool,
}

impl Toast {
    pub fn new(kind: ToastKind, message: impl Into<SharedString>) -> Self {
        Self {
            kind,
            message: message.into(),
            description: None,
            action: None,
            persistent: false,
        }
    }

    pub fn success(message: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Success, message)
    }

    pub fn error(message: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Error, message)
    }

    pub fn info(message: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Info, message)
    }

    pub fn warning(message: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Warning, message)
    }

    /// A second, muted line under the message.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A button on the toast, such as Undo; clicking it runs `on_click` and closes the toast.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Rc::new(on_click)));
        self
    }

    /// Stays until the user closes it.
    pub fn persistent(mut self) -> Self {
        self.persistent = true;
        self
    }

    pub fn show(self, window: &mut Window, cx: &mut App) {
        window.push_notification(self.into_notification(), cx);
    }

    /// Shows the toast in the main window, if it is open.
    pub fn show_in_main_window(self, cx: &mut App) {
        with_main_window(cx, move |window, cx| self.show(window, cx));
    }

    pub fn into_notification(self) -> Notification {
        let kind = match self.kind {
            ToastKind::Success => NotificationType::Success,
            ToastKind::Error => NotificationType::Error,
            ToastKind::Info => NotificationType::Info,
            ToastKind::Warning => NotificationType::Warning,
        };
        let mut notification = match self.description {
            Some(description) => Notification::new().title(self.message).message(description),
            None => Notification::new().message(self.message),
        }
        .with_type(kind);
        if let Some((label, on_click)) = self.action {
            notification = notification.action(move |_, _, cx| {
                let on_click = on_click.clone();
                Button::new("toast-action")
                    .label(label.clone())
                    .small()
                    .primary()
                    .on_click(cx.listener(move |note, _, window, cx| {
                        on_click(window, cx);
                        note.dismiss(window, cx);
                    }))
            });
        }
        notification.autohide(!self.persistent)
    }
}
