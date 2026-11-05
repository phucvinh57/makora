use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, Subscription, Task, Window, div,
};
use makora_lib::settings::ProviderConfig;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs;
use crate::ui::dialogs::provider::ProviderDialog;
use crate::ui::dialogs::remove_provider::RemoveProviderDialog;
use crate::ui::error::UiError;
use crate::ui::format::providers::provider_kind_label;
use crate::ui::widgets::card::Card;
use crate::ui::widgets::toast::Toast;

/// `https://api.openai.com/v1 · Key saved`, or the address alone while the key is unknown.
pub fn provider_address(base_url: &str, has_key: Option<bool>) -> String {
    match has_key {
        Some(true) => format!("{base_url} · Key saved"),
        Some(false) => format!("{base_url} · No key"),
        None => base_url.to_string(),
    }
}

pub struct ProvidersSection {
    has_keys: HashMap<String, bool>,
    keys_for: Option<(u64, Vec<String>)>,
    testing: HashMap<String, Task<()>>,
    keys_load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ProvidersSection {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe_in(&AppState::entity(cx), window, |this, _, _, cx| {
                this.load_keys(cx);
                cx.notify();
            }),
        ];
        let mut this = Self {
            has_keys: HashMap::new(),
            keys_for: None,
            testing: HashMap::new(),
            keys_load: Task::ready(()),
            _subscriptions: subscriptions,
        };
        this.load_keys(cx);
        this
    }

    /// Asks the keychain again once a provider was saved, added or removed.
    fn load_keys(&mut self, cx: &mut Context<Self>) {
        let state = AppState::read(cx);
        let ids: Vec<String> = state
            .settings
            .ai
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect();
        let wanted = (state.revisions.providers, ids);
        if self.keys_for.as_ref() == Some(&wanted) {
            return;
        }
        let ids = wanted.1.clone();
        self.keys_for = Some(wanted);
        self.keys_load = data::fetch(
            cx,
            move |core| {
                ids.into_iter()
                    .map(|id| {
                        let has_key = core.providers.has_key(&id);
                        (id, has_key)
                    })
                    .collect::<HashMap<_, _>>()
            },
            |this, has_keys, _| this.has_keys = has_keys,
        );
    }

    fn test(&mut self, provider: &ProviderConfig, window: &mut Window, cx: &mut Context<Self>) {
        let id = provider.id.clone();
        let name = provider.name.clone();
        let task = data::fetch_in(
            window,
            cx,
            {
                let id = id.clone();
                move |core| core.providers.list_models(&id).map_err(UiError::from)
            },
            {
                let id = id.clone();
                move |this, result, window, cx| {
                    this.testing.remove(&id);
                    match result {
                        Ok(models) => Toast::success(format!("{name} is reachable"))
                            .description(format!("{} models available.", models.len()))
                            .show(window, cx),
                        Err(err) => Toast::error(format!("Could not reach {name}"))
                            .description(err.to_string())
                            .show(window, cx),
                    }
                }
            },
        );
        self.testing.insert(id, task);
        cx.notify();
    }

    fn render_row(
        &self,
        ix: usize,
        provider: &ProviderConfig,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let testing = self.testing.contains_key(&provider.id);
        let address =
            provider_address(&provider.base_url, self.has_keys.get(&provider.id).copied());
        let (to_test, to_edit, to_remove) = (provider.clone(), provider.clone(), provider.clone());

        h_flex()
            .justify_between()
            .gap_3()
            .px_3()
            .py_2()
            .text_sm()
            .when(ix > 0, |this| this.border_t_1().border_color(border))
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().truncate().font_medium().child(provider.name.clone()))
                            .child(
                                Tag::new()
                                    .outline()
                                    .small()
                                    .child(provider_kind_label(provider.kind)),
                            )
                            .when(active, |this| {
                                this.child(Tag::secondary().small().child("In use"))
                            }),
                    )
                    .child(div().truncate().text_xs().text_color(muted).child(address)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .child(
                        Button::new(("test", ix))
                            .small()
                            .outline()
                            .label(if testing { "Testing…" } else { "Test" })
                            .disabled(testing)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.test(&to_test, window, cx)
                            })),
                    )
                    .child(
                        Button::new(("edit", ix))
                            .small()
                            .ghost()
                            .icon(Icon::new(IconName::Pencil))
                            .tooltip("Edit")
                            .accessibility_label(format!("Edit {}", provider.name))
                            .on_click(move |_, window, cx| {
                                dialogs::open::<ProviderDialog>(Some(to_edit.clone()), window, cx)
                            }),
                    )
                    .child(
                        Button::new(("remove", ix))
                            .small()
                            .ghost()
                            .icon(Icon::new(IconName::Trash))
                            .tooltip("Remove")
                            .accessibility_label(format!("Remove {}", provider.name))
                            .on_click(move |_, window, cx| {
                                dialogs::open::<RemoveProviderDialog>(to_remove.clone(), window, cx)
                            }),
                    ),
            )
    }
}

impl Render for ProvidersSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let ai = AppState::read(cx).settings.ai.clone();
        let rows: Vec<_> = ai
            .providers
            .iter()
            .enumerate()
            .map(|(ix, provider)| {
                let active = ai.llm_provider.as_deref() == Some(provider.id.as_str());
                self.render_row(ix, provider, active, cx)
            })
            .collect();

        Card::new()
            .small()
            .title("AI providers")
            .action(
                Button::new("add-provider")
                    .small()
                    .primary()
                    .icon(Icon::new(IconName::Plus))
                    .label("Add provider")
                    .on_click(|_, window, cx| dialogs::open::<ProviderDialog>(None, window, cx)),
            )
            .child(
                v_flex()
                    .gap_3()
                    .map(|this| {
                        if rows.is_empty() {
                            this.child(div().text_sm().text_color(muted).child(
                                "Optional. AI categorization runs on a local model unless you \
                                 add a provider here, such as OpenAI, Anthropic or any \
                                 OpenAI-compatible server, and pick it under Categories.",
                            ))
                        } else {
                            this.child(
                                v_flex()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(border)
                                    .children(rows),
                            )
                        }
                    })
                    .child(div().text_xs().text_color(muted).child(
                        "When a remote provider categorizes, the titles and addresses of the \
                         activities being labeled and your category names are sent to it. \
                         Local servers like Ollama keep everything on this machine.",
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_says_whether_a_key_is_saved() {
        let url = "https://api.openai.com/v1";
        assert_eq!(
            provider_address(url, Some(true)),
            "https://api.openai.com/v1 · Key saved"
        );
        assert_eq!(
            provider_address(url, Some(false)),
            "https://api.openai.com/v1 · No key"
        );
        assert_eq!(provider_address(url, None), url);
    }
}
