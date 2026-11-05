//! One entity per sidebar entry. The shell builds a screen on its first visit and keeps it, so
//! its filters and scroll survive switching away. Each screen lives in its own directory.

pub mod categories;
pub mod history;
pub mod insights;
pub mod settings;
pub mod statistics;

use gpui_kit::{Context, Render, Window};

pub trait Screen: Render + Sized {
    /// What a [`Route`](crate::ui::route::Route) can say about this screen: a tab, filters, or
    /// nothing.
    type Location;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self;

    /// Moves to `location`, from a link such as the AI notification's.
    fn navigate(&mut self, location: Self::Location, window: &mut Window, cx: &mut Context<Self>) {
        let _ = (location, window, cx);
    }
}
