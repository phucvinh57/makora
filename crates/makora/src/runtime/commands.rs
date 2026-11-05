//! Requests from outside the UI thread (tray, second launch, notification click) to the app.

use async_channel::{Receiver, Sender};
use gpui_kit::App;
use makora_lib::events::NavTarget;

use crate::runtime::window;
use crate::ui::route::{CategoriesTab, Route};

#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    Show,
    ShowAt(Route),
    Quit,
}

impl From<NavTarget> for UiCommand {
    fn from(target: NavTarget) -> Self {
        match target {
            NavTarget::CategoriesReview => Self::ShowAt(Route::Categories(CategoriesTab::Review)),
        }
    }
}

pub type CommandSender = Sender<UiCommand>;
pub type CommandReceiver = Receiver<UiCommand>;

pub fn channel() -> (CommandSender, CommandReceiver) {
    async_channel::unbounded()
}

/// Runs each command on the UI thread as it arrives, for the life of the app.
pub fn start_pump(commands: CommandReceiver, cx: &mut App) {
    cx.spawn(async move |cx| {
        while let Ok(command) = commands.recv().await {
            cx.update(|cx| handle(command, cx));
        }
    })
    .detach();
}

pub fn handle(command: UiCommand, cx: &mut App) {
    match command {
        UiCommand::Show => window::show_main_window(None, cx),
        UiCommand::ShowAt(route) => window::show_main_window(Some(route), cx),
        UiCommand::Quit => cx.quit(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_click_opens_the_review_tab() {
        assert_eq!(
            UiCommand::from(NavTarget::CategoriesReview),
            UiCommand::ShowAt(Route::Categories(CategoriesTab::Review))
        );
    }
}
