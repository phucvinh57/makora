//! Adds an AI provider, or edits one: a preset or any OpenAI-compatible server, its address and
//! its API key. Saving tests the connection and says how it went.

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{
    ActiveTheme, Disableable, IndexPath, Sizable, StyledExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window, div, px,
};
use makora_lib::ai::providers::ProviderDraft;
use makora_lib::settings::{ProviderConfig, ProviderKind};

use crate::ui::app_state::{AppState, Core, Revision};
use crate::ui::data;
use crate::ui::dialogs::AppDialog;
use crate::ui::error::UiError;
use crate::ui::format::providers::{
    CUSTOM_PRESET, CUSTOM_PRESET_LABEL, PROVIDER_KINDS, PROVIDER_PRESETS, ProviderPreset,
    parse_provider_kind, preset_by_id, preset_of, provider_kind_id, provider_kind_label,
};
use crate::ui::widgets::select_item::SelectItem;
use crate::ui::widgets::toast::Toast;

const DIALOG_WIDTH: f32 = 448.;

/// The preset the form opens on: the one a saved provider was made from, or the first.
pub fn initial_preset(provider: Option<&ProviderConfig>) -> &'static str {
    match provider {
        Some(provider) => preset_of(provider).map_or(CUSTOM_PRESET, |preset| preset.id),
        None => PROVIDER_PRESETS[0].id,
    }
}

/// The API, address and default name a preset fills in; "Other" clears them.
pub fn preset_fields(preset_id: &str) -> (ProviderKind, &'static str, &'static str) {
    match preset_by_id(preset_id) {
        Some(preset) => (preset.kind, preset.base_url, preset.label),
        None => (ProviderKind::OpenAi, "", ""),
    }
}

pub fn key_placeholder(
    has_key: bool,
    clear_key: bool,
    preset: Option<&ProviderPreset>,
) -> &'static str {
    if has_key {
        if clear_key {
            "The saved key will be removed"
        } else {
            "Leave blank to keep the saved key"
        }
    } else if preset.is_some_and(|preset| !preset.needs_key) {
        "Not needed for a local server"
    } else {
        "sk-…"
    }
}

/// The key to save: a typed one, `""` to remove the saved one, or `None` to keep it.
pub fn api_key_change(typed: &str, clear_key: bool) -> Option<String> {
    let key = typed.trim();
    if !key.is_empty() {
        Some(key.to_string())
    } else if clear_key {
        Some(String::new())
    } else {
        None
    }
}

/// The id the save gave a new provider: the one that was not there before.
pub fn added_provider_id(before: &[ProviderConfig], after: &[ProviderConfig]) -> Option<String> {
    after
        .iter()
        .find(|item| !before.iter().any(|old| old.id == item.id))
        .map(|item| item.id.clone())
}

pub struct ProviderDialog;

impl AppDialog for ProviderDialog {
    /// The provider to edit; `None` to add one.
    type Data = Option<ProviderConfig>;

    fn open(provider: Option<ProviderConfig>, window: &mut Window, cx: &mut App) {
        let title: SharedString = match &provider {
            Some(provider) => format!("Edit {}", provider.name).into(),
            None => "Add an AI provider".into(),
        };
        let form = cx.new(|cx| ProviderForm::new(provider, window, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            let submit = form.clone();
            dialog
                .title(title.clone())
                .w(px(DIALOG_WIDTH))
                .on_ok(move |_, window, cx| {
                    submit.update(cx, |form, cx| form.submit(window, cx));
                    false
                })
                .child(form.clone())
        });
    }
}

struct ProviderForm {
    provider: Option<ProviderConfig>,
    preset_select: Entity<SelectState<Vec<SelectItem>>>,
    kind_select: Entity<SelectState<Vec<SelectItem>>>,
    name: Entity<InputState>,
    base_url: Entity<InputState>,
    api_key: Entity<InputState>,
    preset_id: &'static str,
    kind: ProviderKind,
    name_touched: bool,
    has_key: bool,
    clear_key: bool,
    saving: bool,
    error: Option<String>,
    has_key_load: Task<()>,
    save: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ProviderForm {
    fn new(provider: Option<ProviderConfig>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preset_id = initial_preset(provider.as_ref());
        let first = &PROVIDER_PRESETS[0];
        let kind = provider
            .as_ref()
            .map_or(first.kind, |provider| provider.kind);
        let name = provider
            .as_ref()
            .map_or(first.label.to_string(), |provider| provider.name.clone());
        let base_url = provider
            .as_ref()
            .map_or(first.base_url.to_string(), |provider| {
                provider.base_url.clone()
            });

        let presets: Vec<SelectItem> = PROVIDER_PRESETS
            .iter()
            .map(|preset| SelectItem::new(preset.id, preset.label))
            .chain([SelectItem::new(CUSTOM_PRESET, CUSTOM_PRESET_LABEL)])
            .collect();
        let preset_ix = presets.iter().position(|choice| choice.value == preset_id);
        let kinds: Vec<SelectItem> = PROVIDER_KINDS
            .into_iter()
            .map(|kind| SelectItem::new(provider_kind_id(kind), provider_kind_label(kind)))
            .collect();
        let kind_ix = PROVIDER_KINDS.iter().position(|item| *item == kind);

        let preset_select = cx.new(|cx| {
            SelectState::new(
                presets,
                preset_ix.map(|ix| IndexPath::default().row(ix)),
                window,
                cx,
            )
        });
        let kind_select = cx.new(|cx| {
            SelectState::new(
                kinds,
                kind_ix.map(|ix| IndexPath::default().row(ix)),
                window,
                cx,
            )
        });
        let name = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let base_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://example.com/v1")
                .default_value(base_url)
        });
        let api_key = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(key_placeholder(false, false, preset_by_id(preset_id)))
        });

        let subscriptions = vec![
            cx.subscribe_in(&preset_select, window, Self::on_preset_selected),
            cx.subscribe_in(&kind_select, window, |this, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = event
                    && let Some(kind) = parse_provider_kind(value)
                {
                    this.kind = kind;
                    cx.notify();
                }
            }),
            cx.subscribe(&name, |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.name_touched = true;
                    cx.notify();
                }
            }),
            cx.subscribe(&base_url, Self::on_input_event),
            cx.subscribe(&api_key, Self::on_input_event),
        ];

        let mut this = Self {
            provider,
            preset_select,
            kind_select,
            name,
            base_url,
            api_key,
            preset_id,
            kind,
            name_touched: false,
            has_key: false,
            clear_key: false,
            saving: false,
            error: None,
            has_key_load: Task::ready(()),
            save: Task::ready(()),
            _subscriptions: subscriptions,
        };
        this.name_touched = this.provider.is_some();
        this.load_has_key(window, cx);
        this
    }

    fn load_has_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.provider.as_ref().map(|provider| provider.id.clone()) else {
            return;
        };
        self.has_key_load = data::fetch_in(
            window,
            cx,
            move |core| core.providers.has_key(&id),
            |this, has_key, window, cx| {
                this.has_key = has_key;
                this.sync_key_placeholder(window, cx);
            },
        );
    }

    fn sync_key_placeholder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder =
            key_placeholder(self.has_key, self.clear_key, preset_by_id(self.preset_id));
        self.api_key.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
    }

    fn on_input_event(
        &mut self,
        _: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Change = event {
            cx.notify();
        }
    }

    fn on_preset_selected(
        &mut self,
        _: &Entity<SelectState<Vec<SelectItem>>>,
        event: &SelectEvent<Vec<SelectItem>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(Some(preset_id)) = event else {
            return;
        };
        let preset_id = preset_by_id(preset_id).map_or(CUSTOM_PRESET, |preset| preset.id);
        self.preset_id = preset_id;
        let (kind, base_url, name) = preset_fields(preset_id);
        self.kind = kind;
        self.kind_select.update(cx, |select, cx| {
            select.set_selected_value(&provider_kind_id(kind).into(), window, cx)
        });
        self.base_url
            .update(cx, |input, cx| input.set_value(base_url, window, cx));
        if !self.name_touched {
            self.name
                .update(cx, |input, cx| input.set_value(name, window, cx));
        }
        self.sync_key_placeholder(window, cx);
        cx.notify();
    }

    fn toggle_clear_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_key = !self.clear_key;
        self.api_key
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.sync_key_placeholder(window, cx);
        cx.notify();
    }

    fn can_save(&self, cx: &App) -> bool {
        !self.name.read(cx).value().trim().is_empty()
            && !self.base_url.read(cx).value().trim().is_empty()
            && !self.saving
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save(cx) {
            return;
        }
        let name = self.name.read(cx).value().to_string();
        let draft = ProviderDraft {
            id: self.provider.as_ref().map(|provider| provider.id.clone()),
            name: name.clone(),
            kind: self.kind,
            base_url: self.base_url.read(cx).value().to_string(),
        };
        let api_key = api_key_change(&self.api_key.read(cx).value(), self.clear_key);
        self.saving = true;
        self.error = None;
        cx.notify();
        self.save = data::fetch_in(
            window,
            cx,
            move |core| core.providers.save(draft, api_key).map_err(UiError::from),
            move |this, result, window, cx| {
                this.saving = false;
                let saved = match result {
                    Ok(saved) => saved,
                    Err(err) => {
                        this.error = Some(err.to_string());
                        return;
                    }
                };
                let id = match &this.provider {
                    Some(provider) => Some(provider.id.clone()),
                    None => added_provider_id(
                        &AppState::read(cx).settings.ai.providers,
                        &saved.ai.providers,
                    ),
                };
                AppState::update(cx, |state, cx| {
                    state.set_settings(saved, cx);
                    state.bump(Revision::Providers, cx);
                });
                window.close_dialog(cx);
                if let Some(id) = id {
                    test_connection(id, name.trim().to_string(), window, cx);
                }
            },
        );
    }
}

/// Lists the provider's models after a save, which shows whether its address and key work.
fn test_connection(id: String, name: String, window: &mut Window, cx: &mut App) {
    let core = Core::get(cx);
    let job =
        cx.background_spawn(async move { core.providers.list_models(&id).map_err(UiError::from) });
    window
        .spawn(cx, async move |cx| {
            let result = job.await;
            let toast = match result {
                Ok(models) => Toast::success(format!("Saved {name}"))
                    .description(format!("Connected; {} models available.", models.len())),
                Err(err) => Toast::warning(format!("Saved {name}, but could not connect"))
                    .description(err.to_string()),
            };
            let _ = cx.update(|window, cx| toast.show(window, cx));
        })
        .detach();
}

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xs().font_medium().child(label))
        .child(control)
}

impl Render for ProviderForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let can_save = self.can_save(cx);

        v_flex()
            .gap_4()
            .child(div().text_sm().text_color(muted).child(
                "A remote language model can categorize activities instead of the local one. \
                 The API key is kept in your system keychain.",
            ))
            .child(
                v_flex()
                    .gap_3()
                    .child(field(
                        "Provider",
                        Select::new(&self.preset_select)
                            .w_full()
                            .accessibility_label("Provider"),
                    ))
                    .when(self.preset_id == CUSTOM_PRESET, |this| {
                        this.child(field(
                            "API",
                            Select::new(&self.kind_select)
                                .w_full()
                                .accessibility_label("API"),
                        ))
                    })
                    .child(field("Name", Input::new(&self.name).aria_label("Name")))
                    .child(field(
                        "Base URL",
                        Input::new(&self.base_url).aria_label("Base URL"),
                    ))
                    .child(field(
                        "API key",
                        h_flex()
                            .gap_2()
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.api_key)
                                        .aria_label("API key")
                                        .disabled(self.clear_key),
                                ),
                            )
                            .when(self.has_key, |this| {
                                this.child(
                                    Button::new("clear-key")
                                        .small()
                                        .outline()
                                        .label(if self.clear_key {
                                            "Keep key"
                                        } else {
                                            "Clear key"
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.toggle_clear_key(window, cx)
                                        })),
                                )
                            }),
                    ))
                    .when_some(self.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(danger).child(error))
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
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save")
                            .primary()
                            .label(if self.saving { "Saving…" } else { "Save" })
                            .disabled(!can_save)
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, kind: ProviderKind, base_url: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            name: id.into(),
            kind,
            base_url: base_url.into(),
        }
    }

    #[test]
    fn opens_on_the_providers_preset() {
        assert_eq!(initial_preset(None), "openai");
        let groq = provider("g", ProviderKind::OpenAi, "https://api.groq.com/openai/v1");
        assert_eq!(initial_preset(Some(&groq)), "groq");
        let own = provider("o", ProviderKind::OpenAi, "http://box:8080/v1");
        assert_eq!(initial_preset(Some(&own)), CUSTOM_PRESET);
    }

    #[test]
    fn presets_fill_kind_url_and_name() {
        assert_eq!(
            preset_fields("anthropic"),
            (
                ProviderKind::Anthropic,
                "https://api.anthropic.com",
                "Anthropic"
            )
        );
        assert_eq!(preset_fields(CUSTOM_PRESET), (ProviderKind::OpenAi, "", ""));
    }

    #[test]
    fn the_key_placeholder_explains_the_key() {
        let openai = preset_by_id("openai");
        let ollama = preset_by_id("ollama");
        assert_eq!(key_placeholder(false, false, openai), "sk-…");
        assert_eq!(
            key_placeholder(false, false, ollama),
            "Not needed for a local server"
        );
        assert_eq!(key_placeholder(false, false, None), "sk-…");
        assert_eq!(
            key_placeholder(true, false, ollama),
            "Leave blank to keep the saved key"
        );
        assert_eq!(
            key_placeholder(true, true, openai),
            "The saved key will be removed"
        );
    }

    #[test]
    fn a_blank_key_keeps_or_clears_the_saved_one() {
        assert_eq!(api_key_change(" sk-1 ", false), Some("sk-1".into()));
        assert_eq!(api_key_change("sk-1", true), Some("sk-1".into()));
        assert_eq!(api_key_change("  ", true), Some(String::new()));
        assert_eq!(api_key_change("", false), None);
    }

    #[test]
    fn finds_the_added_provider() {
        let a = provider("a", ProviderKind::OpenAi, "x");
        let b = provider("b", ProviderKind::OpenAi, "y");
        assert_eq!(
            added_provider_id(std::slice::from_ref(&a), &[a.clone(), b]),
            Some("b".into())
        );
        assert_eq!(
            added_provider_id(std::slice::from_ref(&a), std::slice::from_ref(&a)),
            None
        );
    }
}
