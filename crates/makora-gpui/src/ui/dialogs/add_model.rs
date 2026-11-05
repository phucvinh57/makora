//! Downloads a model from the Ollama library. The download outlives the dialog: closing it keeps
//! the download going, and reopening it shows its progress and Stop again.

use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::link::Link;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, Global, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Task, WeakEntity, Window, div, px,
};
use makora_lib::ai::llm::ModelRole;

use crate::runtime::window::with_main_window;
use crate::ui::app_state::{AppState, Revision};
use crate::ui::data;
use crate::ui::dialogs::AppDialog;
use crate::ui::error::UiError;
use crate::ui::format::bytes::format_bytes;
use crate::ui::widgets::suggest_input::{SuggestInput, SuggestInputEvent, Suggestion};
use crate::ui::widgets::toast::Toast;

const OLLAMA_LIBRARY: &str = "https://ollama.com/library";

/// Called with the model's role and name once it has downloaded and is installed.
pub type OnDownloaded = Rc<dyn Fn(ModelRole, &str, &mut Window, &mut App)>;

#[derive(Default)]
pub struct AddModelData {
    /// The role and name to start from, e.g. the model a run needs and does not have yet.
    pub role: Option<ModelRole>,
    pub model: Option<String>,
    pub on_downloaded: Option<OnDownloaded>,
}

struct RoleOption {
    role: ModelRole,
    label: &'static str,
    hint: &'static str,
    suggestions: &'static [&'static str],
}

const ROLE_OPTIONS: [RoleOption; 2] = [
    RoleOption {
        role: ModelRole::Embedding,
        label: "Embedding model",
        hint: "Small and fast. Finds the activities you filed that are most like each new one. \
               Runs on any machine.",
        suggestions: &[
            "all-minilm",
            "nomic-embed-text",
            "mxbai-embed-large",
            "paraphrase-multilingual",
        ],
    },
    RoleOption {
        role: ModelRole::Llm,
        label: "LLM",
        hint: "A language model that labels each activity, shown the ones you filed that look \
               like it. Accurate, but needs a capable machine.",
        suggestions: &["llama3.2", "llama3.2:1b", "qwen2.5:7b-instruct"],
    },
];

fn role_option(role: ModelRole) -> &'static RoleOption {
    ROLE_OPTIONS
        .iter()
        .find(|option| option.role == role)
        .unwrap_or(&ROLE_OPTIONS[0])
}

/// The share of the download done, out of 100.
pub fn download_percent(downloaded: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.;
    }
    (downloaded as f64 / total as f64 * 100.) as f32
}

/// The download in flight and the dialog showing it, if one is open.
#[derive(Default)]
struct Host {
    form: Option<WeakEntity<AddModelForm>>,
    on_downloaded: Option<OnDownloaded>,
    pending: bool,
    download: Option<Task<()>>,
}

impl Global for Host {}

fn host(cx: &mut App) -> &mut Host {
    cx.default_global::<Host>()
}

/// Whether a download is starting or running, here or from another dialog.
fn download_busy(cx: &App) -> bool {
    cx.try_global::<Host>().is_some_and(|host| host.pending) || AppState::read(cx).download.running
}

fn open_form(cx: &App) -> Option<Entity<AddModelForm>> {
    cx.try_global::<Host>()?.form.as_ref()?.upgrade()
}

pub struct AddModelDialog;

impl AddModelDialog {
    /// Whether the dialog is showing, so a second one is not stacked on it.
    pub fn is_open(cx: &App) -> bool {
        open_form(cx).is_some()
    }
}

impl AppDialog for AddModelDialog {
    type Data = AddModelData;

    fn open(data: AddModelData, window: &mut Window, cx: &mut App) {
        if !download_busy(cx) {
            host(cx).on_downloaded = data.on_downloaded;
        }
        if let Some(form) = open_form(cx) {
            form.update(cx, |form, cx| form.reset(data.role, data.model, window, cx));
            return;
        }
        let role = data.role.unwrap_or(ModelRole::Embedding);
        let model = data.model.unwrap_or_default();
        let form = cx.new(|cx| AddModelForm::new(role, model, window, cx));
        host(cx).form = Some(form.downgrade());
        let name = form.read(cx).name.clone();
        let closing = form.downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let closing = closing.clone();
            dialog
                .w(px(448.))
                .child(form.clone())
                .on_close(move |_, _, cx| {
                    let host = host(cx);
                    if host
                        .form
                        .as_ref()
                        .is_some_and(|form| form.entity_id() == closing.entity_id())
                    {
                        host.form = None;
                    }
                })
        });
        window.defer(cx, move |window, cx| {
            name.update(cx, |input, cx| input.focus(window, cx))
        });
    }
}

pub struct AddModelForm {
    role: ModelRole,
    name: Entity<SuggestInput>,
    typed: String,
    installed: Option<(String, bool)>,
    installed_revision: u64,
    error: Option<String>,
    check: Task<()>,
    stop: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl AddModelForm {
    fn new(role: ModelRole, model: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let option = role_option(role);
        let name = cx.new(|cx| {
            let mut input = SuggestInput::new(option.suggestions[0], window, cx);
            input.set_value(model.clone(), window, cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe(&name, |this, _, SuggestInputEvent::Change(value), cx| {
                this.typed = value.to_string();
                this.check_installed(cx);
            }),
            cx.observe(&AppState::entity(cx), |this, app, cx| {
                let revision = app.read(cx).revisions.installed_models;
                if this.installed_revision != revision {
                    this.installed_revision = revision;
                    this.check_installed(cx);
                }
                cx.notify();
            }),
        ];
        let mut form = Self {
            role,
            name,
            typed: model,
            installed: None,
            installed_revision: AppState::read(cx).revisions.installed_models,
            error: None,
            check: Task::ready(()),
            stop: Task::ready(()),
            _subscriptions: subscriptions,
        };
        form.set_role(role, window, cx);
        form.check_installed(cx);
        form
    }

    fn reset(
        &mut self,
        role: Option<ModelRole>,
        model: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.downloading(cx) {
            return;
        }
        if let Some(role) = role {
            self.set_role(role, window, cx);
        }
        if let Some(model) = model {
            self.typed = model.clone();
            self.name
                .update(cx, |input, cx| input.set_value(model, window, cx));
            self.check_installed(cx);
        }
    }

    fn set_role(&mut self, role: ModelRole, window: &mut Window, cx: &mut Context<Self>) {
        self.role = role;
        let option = role_option(role);
        let suggestions = option
            .suggestions
            .iter()
            .map(|name| Suggestion::new(*name))
            .collect();
        self.name.update(cx, |input, cx| {
            input.set_placeholder(option.suggestions[0], window, cx);
            input.set_suggestions(suggestions, cx);
        });
        cx.notify();
    }

    fn downloading(&self, cx: &App) -> bool {
        download_busy(cx)
    }

    /// The name in the field, or the one downloading.
    fn name(&self, cx: &App) -> String {
        let download = &AppState::read(cx).download;
        if self.downloading(cx) && download.running && !download.model.is_empty() {
            return download.model.clone();
        }
        self.typed.trim().to_string()
    }

    fn check_installed(&mut self, cx: &mut Context<Self>) {
        let name = self.name(cx);
        if name.is_empty() {
            self.installed = None;
            self.check = Task::ready(());
            cx.notify();
            return;
        }
        let checked = name.clone();
        self.check = data::fetch(
            cx,
            move |core| core.downloads.is_installed(&checked),
            move |this, installed, _| this.installed = Some((name, installed)),
        );
    }

    fn installed_now(&self, cx: &App) -> Option<bool> {
        let name = self.name(cx);
        self.installed
            .as_ref()
            .filter(|(checked, _)| *checked == name)
            .map(|(_, installed)| *installed)
    }

    fn download(&mut self, cx: &mut Context<Self>) {
        let model = self.name(cx);
        if model.is_empty() || self.installed_now(cx) != Some(false) || self.downloading(cx) {
            return;
        }
        let role = self.role;
        self.error = None;
        AppState::update(cx, |state, cx| state.mark_download_started(&model, cx));
        let job = {
            let model = model.clone();
            data::blocking(cx, move |core| {
                core.downloads.download(&model).map_err(UiError::from)
            })
        };
        let task = cx.spawn(async move |_, cx| {
            let result = job.await;
            cx.update(|cx| finish_download(role, &model, result, cx));
        });
        let host = host(cx);
        host.pending = true;
        host.download = Some(task);
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.stop = data::fetch(cx, |core| core.downloads.cancel(), |_, _, _| {});
    }
}

fn finish_download(
    role: ModelRole,
    model: &str,
    result: Option<Result<bool, UiError>>,
    cx: &mut App,
) {
    let host = host(cx);
    host.pending = false;
    host.download = None;
    let on_downloaded = host.on_downloaded.clone();
    let form = open_form(cx);
    AppState::update(cx, |state, cx| {
        state.download.running = false;
        state.bump(Revision::InstalledModels, cx);
    });
    match result {
        Some(Ok(true)) => {
            let model = model.to_string();
            with_main_window(cx, move |window, cx| {
                Toast::success(format!("Downloaded {model}")).show(window, cx);
                if let Some(on_downloaded) = on_downloaded {
                    on_downloaded(role, &model, window, cx);
                }
                if open_form(cx).is_some() {
                    window.close_dialog(cx);
                }
            });
        }
        Some(Ok(false)) => {}
        Some(Err(err)) => match form {
            Some(form) => form.update(cx, |form, cx| {
                form.error = Some(err.to_string());
                cx.notify();
            }),
            None => Toast::error(err.to_string()).show_in_main_window(cx),
        },
        None => Toast::error("The download stopped unexpectedly.").show_in_main_window(cx),
    }
    if let Some(form) = open_form(cx) {
        form.update(cx, |form, cx| form.check_installed(cx));
    }
}

impl Render for AddModelForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let downloading = self.downloading(cx);
        let download = AppState::read(cx).download.clone();
        let name = self.name(cx);
        let installed = self.installed_now(cx);
        let can_download = !name.is_empty() && installed == Some(false);
        let option = role_option(self.role);
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_lg().font_semibold().pr_6().child("Add a model"))
                    .child(
                        h_flex()
                            .flex_wrap()
                            .text_sm()
                            .text_color(muted)
                            .child("Download a model from the\u{a0}")
                            .child(
                                Link::new("ollama-library")
                                    .href(OLLAMA_LIBRARY)
                                    .child("Ollama library"),
                            )
                            .child("\u{a0}to use for AI categorization."),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(h_flex().gap_2().children(ROLE_OPTIONS.iter().map(|option| {
                        let role = option.role;
                        let button = Button::new(SharedString::from(option.label))
                            .small()
                            .label(option.label)
                            .disabled(downloading)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_role(role, window, cx)
                            }));
                        if role == self.role {
                            button.primary()
                        } else {
                            button.outline()
                        }
                    })))
                    .child(div().text_xs().text_color(muted).child(option.hint))
                    .map(|this| {
                        if downloading {
                            this.child(
                                div()
                                    .h_8()
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .rounded(theme.radius)
                                    .border_1()
                                    .border_color(theme.input)
                                    .text_sm()
                                    .opacity(0.5)
                                    .child(name.clone()),
                            )
                        } else {
                            this.child(self.name.clone())
                        }
                    })
                    .when(!name.is_empty() && installed == Some(true), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("Already installed."),
                        )
                    })
                    .when(
                        downloading && download.running && download.total > 0,
                        |this| {
                            this.child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        Progress::new("download-progress")
                                            .value(download_percent(
                                                download.downloaded,
                                                download.total,
                                            ))
                                            .accessibility_label("Model download progress"),
                                    )
                                    .child(div().text_xs().text_color(muted).child(format!(
                                        "{} of {}",
                                        format_bytes(download.downloaded),
                                        format_bytes(download.total)
                                    ))),
                            )
                        },
                    )
                    .when_some(self.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(theme.danger).child(error))
                    }),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .map(|this| {
                        if downloading {
                            this.child(
                                Button::new("stop")
                                    .danger()
                                    .label("Stop")
                                    .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
                            )
                        } else {
                            this.child(
                                Button::new("cancel")
                                    .ghost()
                                    .label("Cancel")
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                        }
                    })
                    .child(
                        Button::new("download")
                            .primary()
                            .label(if downloading {
                                "Downloading…"
                            } else {
                                "Download"
                            })
                            .disabled(!can_download || downloading)
                            .on_click(cx.listener(|this, _, _, cx| this.download(cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_a_percentage() {
        assert_eq!(download_percent(0, 0), 0.);
        assert_eq!(download_percent(50, 200), 25.);
        assert_eq!(download_percent(200, 200), 100.);
    }

    #[test]
    fn each_role_suggests_its_own_models() {
        assert_eq!(
            role_option(ModelRole::Embedding).suggestions[0],
            "all-minilm"
        );
        assert_eq!(role_option(ModelRole::Llm).suggestions[0], "llama3.2");
        assert_eq!(role_option(ModelRole::Llm).label, "LLM");
    }
}
