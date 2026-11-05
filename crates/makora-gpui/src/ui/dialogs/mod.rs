//! Dialogs any screen opens by type, like the web UI's `openDialog(name, data)`:
//!
//! ```ignore
//! dialogs::open::<AddCategoryDialog>(AddCategoryData { parent: None, on_created }, window, cx);
//! ```
//!
//! A dialog is a type implementing [`AppDialog`] in its own module here; adding one takes that
//! module and its `pub mod` line below, and nothing else shared.

pub mod confirm;
pub mod provider;
pub mod remove_provider;

use gpui_kit::{App, Window};

use crate::runtime::window::with_main_window;

pub trait AppDialog: 'static {
    /// What the dialog is opened with; `()` for one that takes nothing.
    type Data: 'static;

    /// Shows the dialog in `window`, typically with `window.open_dialog`.
    fn open(data: Self::Data, window: &mut Window, cx: &mut App);
}

pub fn open<D: AppDialog>(data: D::Data, window: &mut Window, cx: &mut App) {
    D::open(data, window, cx);
}

/// [`open`] from code without a window at hand, such as a tray action. Does nothing while the
/// main window is closed.
pub fn open_in_main_window<D: AppDialog>(data: D::Data, cx: &mut App) {
    with_main_window(cx, move |window, cx| open::<D>(data, window, cx));
}
