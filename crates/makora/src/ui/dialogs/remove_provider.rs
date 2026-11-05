//! "Remove OpenAI?" for a saved AI provider; its key leaves the keychain with it.

use gpui_kit::{App, AppContext, Window};
use makora_lib::settings::ProviderConfig;

use crate::ui::app_state::{AppState, Core, Revision};
use crate::ui::dialogs::AppDialog;
use crate::ui::dialogs::confirm::Confirm;
use crate::ui::error::UiError;
use crate::ui::widgets::toast::Toast;

pub fn remove_description(in_use: bool) -> String {
    let mut description = "Its API key is deleted from your system keychain.".to_string();
    if in_use {
        description.push_str(
            " AI categorization uses this provider now, so it goes back to the local language \
             model.",
        );
    }
    description
}

pub struct RemoveProviderDialog;

impl AppDialog for RemoveProviderDialog {
    type Data = ProviderConfig;

    fn open(provider: ProviderConfig, window: &mut Window, cx: &mut App) {
        let in_use = AppState::read(cx).settings.ai.llm_provider.as_deref() == Some(&provider.id);
        Confirm::new(format!("Remove {}?", provider.name))
            .description(remove_description(in_use))
            .confirm_label("Remove")
            .destructive()
            .open(window, cx, move |window, cx| {
                remove(provider.id.clone(), window, cx)
            });
    }
}

fn remove(id: String, window: &mut Window, cx: &mut App) {
    let core = Core::get(cx);
    let job = cx.background_spawn(async move { core.providers.remove(&id).map_err(UiError::from) });
    window
        .spawn(cx, async move |cx| {
            let result = job.await;
            let _ = cx.update(|window, cx| match result {
                Ok(settings) => AppState::update(cx, |state, cx| {
                    state.set_settings(settings, cx);
                    state.bump(Revision::Providers, cx);
                }),
                Err(err) => Toast::error(err.to_string()).show(window, cx),
            });
        })
        .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_when_the_provider_is_in_use() {
        assert_eq!(
            remove_description(false),
            "Its API key is deleted from your system keychain."
        );
        assert!(remove_description(true).ends_with("goes back to the local language model."));
    }
}
