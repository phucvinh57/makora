use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Enter, Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window, div,
};
use makora_lib::ai::privacy::{Kind, PrivacyPreview, PrivacySettings, UrlPolicy, account_names};

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::privacy::{
    account_names_hint, preview_summary, redaction_kind_label, url_policy_example, url_policy_label,
};
use crate::ui::widgets::card::Card;
use crate::ui::widgets::segmented::segmented;

pub struct PrivacySection {
    term_input: Entity<InputState>,
    term: SharedString,
    /// The settings the preview was made for.
    previewed: Option<PrivacySettings>,
    preview: Option<Vec<PrivacyPreview>>,
    preview_error: Option<String>,
    saving: bool,
    error: Option<String>,
    save: Task<()>,
    load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl PrivacySection {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let term_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("A name, client or project"));
        let subscriptions = vec![
            cx.subscribe_in(
                &term_input,
                window,
                |this, input, event: &InputEvent, _, cx| {
                    if let InputEvent::Change = event {
                        this.term = input.read(cx).value();
                        cx.notify();
                    }
                },
            ),
            cx.observe(&AppState::entity(cx), |this, _, cx| {
                this.refresh_preview(cx)
            }),
        ];
        let mut this = Self {
            term_input,
            term: SharedString::default(),
            previewed: None,
            preview: None,
            preview_error: None,
            saving: false,
            error: None,
            save: Task::ready(()),
            load: Task::ready(()),
            _subscriptions: subscriptions,
        };
        this.refresh_preview(cx);
        this
    }

    /// Reloads the preview when the saved settings differ from the ones it shows.
    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let privacy = AppState::read(cx).settings.ai.privacy.clone();
        if self.previewed.as_ref() == Some(&privacy) {
            return;
        }
        self.previewed = Some(privacy.clone());
        self.load = data::fetch(
            cx,
            move |core| {
                core.settings
                    .privacy_preview(&privacy)
                    .map_err(UiError::from)
            },
            |this, result, _| match result {
                Ok(rows) => {
                    this.preview = Some(rows);
                    this.preview_error = None;
                }
                Err(err) => this.preview_error = Some(err.to_string()),
            },
        );
    }

    fn change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut PrivacySettings),
    ) {
        let mut privacy = AppState::read(cx).settings.ai.privacy.clone();
        edit(&mut privacy);
        self.saving = true;
        self.error = None;
        cx.notify();
        self.save = data::fetch_in(
            window,
            cx,
            move |core| core.settings.set_privacy(privacy).map_err(UiError::from),
            |this, result, _, cx| {
                this.saving = false;
                match result {
                    Ok(settings) => {
                        AppState::update(cx, |state, cx| state.set_settings(settings, cx))
                    }
                    Err(err) => this.error = Some(err.to_string()),
                }
            },
        );
    }

    fn add_term(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let term = self.term.trim().to_string();
        if term.is_empty() || self.saving {
            return;
        }
        self.change(window, cx, |privacy| privacy.terms.push(term));
        self.term = SharedString::default();
        self.term_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn render_kinds(&self, privacy: &PrivacySettings, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_wrap()
            .gap_y_2()
            .children(Kind::ALL.into_iter().map(|kind| {
                let on = privacy.redact.contains(&kind);
                div().w_1_2().child(
                    Checkbox::new(SharedString::from(format!("redact-{}", kind.as_str())))
                        .label(redaction_kind_label(kind))
                        .checked(on)
                        .disabled(self.saving)
                        .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                            let checked = *checked;
                            this.change(window, cx, |privacy| {
                                privacy.redact.retain(|other| *other != kind);
                                if checked {
                                    privacy.redact.push(kind);
                                }
                            })
                        })),
                )
            }))
    }

    fn render_terms(&self, terms: &[String], cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_2()
            .child(div().text_sm().child("Words to hide"))
            .child(
                h_flex()
                    .gap_2()
                    .on_action(cx.listener(|this, _: &Enter, window, cx| this.add_term(window, cx)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.term_input).aria_label("Word to hide")),
                    )
                    .child(
                        Button::new("add-term")
                            .small()
                            .outline()
                            .icon(Icon::new(IconName::Plus))
                            .label("Add")
                            .disabled(self.term.trim().is_empty() || self.saving)
                            .on_click(cx.listener(|this, _, window, cx| this.add_term(window, cx))),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Matched as whole words, ignoring case, and sent as [term]."),
            )
            .when(!terms.is_empty(), |this| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(terms.iter().enumerate().map(|(ix, term)| {
                            let target = term.clone();
                            Tag::secondary().small().child(
                                h_flex().gap_1().child(term.clone()).child(
                                    Button::new(("remove-term", ix))
                                        .ghost()
                                        .xsmall()
                                        .icon(Icon::new(IconName::X))
                                        .accessibility_label(format!("Stop hiding {term}"))
                                        .disabled(self.saving)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            let target = target.clone();
                                            this.change(window, cx, |privacy| {
                                                privacy.terms.retain(|term| *term != target)
                                            })
                                        })),
                                ),
                            )
                        })),
                )
            })
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let danger = cx.theme().danger;
        let header = v_flex()
            .gap_0p5()
            .child(div().text_sm().font_medium().child("Preview"))
            .when_some(self.preview.as_ref(), |this, rows| {
                let changed = rows.iter().filter(|row| row.before != row.after).count();
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(preview_summary(changed, rows.len())),
                )
            });
        let body = match (&self.preview_error, &self.preview) {
            (Some(error), _) => div()
                .text_xs()
                .text_color(danger)
                .child(format!("Could not load the preview: {error}"))
                .into_any_element(),
            (None, None) => div()
                .text_xs()
                .text_color(muted)
                .child("Loading…")
                .into_any_element(),
            (None, Some(rows)) if rows.is_empty() => div().into_any_element(),
            (None, Some(rows)) => v_flex()
                .rounded_md()
                .border_1()
                .border_color(border)
                .text_xs()
                .child(
                    h_flex()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .text_color(muted)
                        .child(div().flex_1().child("Without redaction"))
                        .child(div().flex_1().child("Sent to the provider")),
                )
                .children(rows.iter().map(|row| {
                    let unchanged = row.before == row.after;
                    h_flex()
                        .items_start()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .border_t_1()
                        .border_color(border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(muted)
                                .child(row.before.clone()),
                        )
                        .child(
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .items_start()
                                .when(unchanged, |this| this.text_color(muted))
                                .child(div().min_w_0().child(row.after.clone()))
                                .when(row.masked, |this| {
                                    this.child(Tag::secondary().xsmall().child("never sent"))
                                }),
                        )
                }))
                .into_any_element(),
        };
        v_flex().gap_2().child(header).child(body)
    }
}

impl Render for PrivacySection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let privacy = AppState::read(cx).settings.ai.privacy.clone();
        let policy_index = UrlPolicy::ALL
            .iter()
            .position(|policy| *policy == privacy.url_policy)
            .unwrap_or(0);

        Card::new()
            .small()
            .title("Privacy")
            .description(
                "What is taken out of activities before a remote AI provider sees them. The \
                 local model always sees everything.",
            )
            .child(
                v_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(div().text_sm().child("Replace with a placeholder"))
                            .child(self.render_kinds(&privacy, cx)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(div().text_sm().child("Redact my account name"))
                                    .child(
                                        Switch::new("redact-account")
                                            .accessibility_label("Redact my account name")
                                            .checked(privacy.redact_account)
                                            .disabled(self.saving)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, window, cx| {
                                                    let checked = *checked;
                                                    this.change(window, cx, |privacy| {
                                                        privacy.redact_account = checked
                                                    })
                                                },
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(account_names_hint(account_names())),
                            ),
                    )
                    .child(self.render_terms(&privacy.terms, cx))
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .gap_3()
                                    .child(div().text_sm().child("Page addresses"))
                                    .child(segmented(
                                        "url-policy",
                                        UrlPolicy::ALL
                                            .iter()
                                            .map(|policy| url_policy_label(*policy).into())
                                            .collect(),
                                        policy_index,
                                        cx.listener(|this, index: &usize, window, cx| {
                                            let Some(policy) = UrlPolicy::ALL.get(*index).copied()
                                            else {
                                                return;
                                            };
                                            if this.saving {
                                                return;
                                            }
                                            this.change(window, cx, |privacy| {
                                                privacy.url_policy = policy
                                            })
                                        }),
                                    )),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(url_policy_example(privacy.url_policy)),
                            ),
                    )
                    .when_some(self.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(danger).child(error))
                    })
                    .child(self.render_preview(cx)),
            )
    }
}
