use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Task, Window, div,
};
use tracing::warn;

use crate::runtime::autostart::{Autostart, AutostartService};
use crate::ui::prefs::ThemePreference;
use crate::ui::theme;
use crate::ui::widgets::card::Card;

pub struct GeneralSection {
    autostart: Option<Arc<dyn Autostart>>,
    enabled: Option<bool>,
    toggling: bool,
    error: Option<String>,
    task: Task<()>,
}

impl GeneralSection {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            autostart: AutostartService::get(cx),
            enabled: None,
            toggling: false,
            error: None,
            task: Task::ready(()),
        };
        this.load(cx);
        this
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(autostart) = self.autostart.clone() else {
            return;
        };
        let job = cx.background_spawn(async move { autostart.is_enabled() });
        self.task = cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(enabled) => this.enabled = Some(enabled),
                    Err(err) => {
                        warn!(
                            error = format!("{err:#}"),
                            "failed to read the autostart entry"
                        );
                        this.enabled = Some(false);
                    }
                }
                cx.notify();
            });
        });
    }

    fn toggle(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let Some(autostart) = self.autostart.clone() else {
            return;
        };
        self.toggling = true;
        self.error = None;
        cx.notify();
        let job = cx.background_spawn(async move { autostart.set_enabled(enabled) });
        self.task = cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.toggling = false;
                match result {
                    Ok(()) => this.load(cx),
                    Err(err) => this.error = Some(format!("{err:#}")),
                }
                cx.notify();
            });
        });
    }
}

impl Render for GeneralSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = theme::is_dark(cx);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let unavailable = self.autostart.is_none();
        let hint = unavailable.then_some(if cfg!(debug_assertions) {
            "Autostart is disabled in dev builds."
        } else {
            "Autostart is not available on this system."
        });

        Card::new().small().title("General").child(
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .justify_between()
                        .child(div().text_sm().child("Theme"))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(theme_button(
                                    "theme-light",
                                    IconName::Sun,
                                    "Light",
                                    !dark,
                                    ThemePreference::Light,
                                ))
                                .child(theme_button(
                                    "theme-dark",
                                    IconName::Moon,
                                    "Dark",
                                    dark,
                                    ThemePreference::Dark,
                                )),
                        ),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .child(div().text_sm().child("Run when system starts"))
                        .child(
                            Switch::new("autostart")
                                .accessibility_label("Run when system starts")
                                .checked(self.enabled.unwrap_or(false))
                                .disabled(unavailable || self.enabled.is_none() || self.toggling)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.toggle(*checked, cx)
                                })),
                        ),
                )
                .when_some(hint, |this, hint| {
                    this.child(div().text_xs().text_color(muted).child(hint))
                })
                .when_some(self.error.clone(), |this, error| {
                    this.child(div().text_xs().text_color(danger).child(error))
                }),
        )
    }
}

fn theme_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    active: bool,
    preference: ThemePreference,
) -> Button {
    Button::new(id)
        .small()
        .icon(Icon::new(icon))
        .label(label)
        .map(|this| {
            if active {
                this.primary()
            } else {
                this.outline()
            }
        })
        .on_click(move |_, window, cx| {
            if !active {
                theme::set_preference(preference, Some(window), cx);
            }
        })
}
