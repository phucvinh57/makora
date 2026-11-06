//! What a remote model charges per million tokens, for the AI usage cost estimate:
//!
//! ```ignore
//! dialogs::open::<ModelPriceDialog>(ModelPriceData { provider, provider_name, model, price }, window, cx);
//! ```

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, AsyncWindowContext, ClickEvent, Context, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window, div, px,
};
use makora_lib::settings::ModelPrice;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs::AppDialog;
use crate::ui::error::UiError;
use crate::ui::widgets::toast::Toast;

#[derive(Debug, Clone)]
pub struct ModelPriceData {
    pub provider: String,
    pub provider_name: String,
    pub model: String,
    pub price: Option<ModelPrice>,
}

pub struct ModelPriceDialog;

impl AppDialog for ModelPriceDialog {
    type Data = ModelPriceData;

    fn open(data: Self::Data, window: &mut Window, cx: &mut App) {
        let title = SharedString::from(format!("Price of {}", data.model));
        let form = cx.new(|cx| PriceForm::new(data, window, cx));
        let input = form.read(cx).inputs[0].clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(title.clone()).w(px(480.)).child(form.clone())
        });
        window.defer(cx, move |window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
        });
    }
}

const LABELS: [&str; 3] = ["Input", "Cached input", "Output"];

/// A price as typed: `Ok(None)` when the field is empty, `Err` when it is not a price.
fn parse_price(text: &str) -> Result<Option<f64>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() && value >= 0.0 => Ok(Some(value)),
        _ => Err(()),
    }
}

/// The price the three fields describe, when input and output are set and none is invalid.
fn price_from(texts: [&str; 3], provider: &str, model: &str) -> Option<ModelPrice> {
    let [input, cached_input, output] = texts.map(parse_price);
    Some(ModelPrice {
        provider: provider.to_owned(),
        model: model.to_owned(),
        input: input.ok()??,
        cached_input: cached_input.ok()?,
        output: output.ok()??,
    })
}

fn to_text(value: Option<f64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

struct PriceForm {
    data: ModelPriceData,
    inputs: [Entity<InputState>; 3],
    saving: bool,
    _subscriptions: Vec<Subscription>,
}

impl PriceForm {
    fn new(data: ModelPriceData, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let price = data.price.as_ref();
        let fields = [
            (to_text(price.map(|price| price.input)), "0.00"),
            (
                to_text(price.and_then(|price| price.cached_input)),
                "Same as input",
            ),
            (to_text(price.map(|price| price.output)), "0.00"),
        ];
        let inputs = fields.map(|(value, placeholder)| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        });
        let subscriptions =
            inputs
                .iter()
                .map(|input| {
                    cx.subscribe_in(input, window, |form, _, event: &InputEvent, window, cx| {
                        match event {
                            InputEvent::Change => cx.notify(),
                            InputEvent::PressEnter { .. } => form.save(window, cx),
                            _ => {}
                        }
                    })
                })
                .collect();
        Self {
            data,
            inputs,
            saving: false,
            _subscriptions: subscriptions,
        }
    }

    fn texts(&self, cx: &App) -> [SharedString; 3] {
        [0, 1, 2].map(|ix| self.inputs[ix].read(cx).value())
    }

    fn price(&self, cx: &App) -> Option<ModelPrice> {
        let texts = self.texts(cx);
        price_from(
            [&texts[0], &texts[1], &texts[2]].map(|text| text.as_ref()),
            &self.data.provider,
            &self.data.model,
        )
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(price) = self.price(cx) else {
            return;
        };
        self.run(move |core| core.settings.set_model_price(price), window, cx);
    }

    fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (provider, model) = (self.data.provider.clone(), self.data.model.clone());
        self.run(
            move |core| core.settings.remove_model_price(&provider, &model),
            window,
            cx,
        );
    }

    fn run<E: Into<UiError> + Send + 'static>(
        &mut self,
        call: impl FnOnce(&makora_lib::app_core::ClientCore) -> Result<makora_lib::settings::Settings, E>
        + Send
        + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.saving = true;
        cx.notify();
        let job = data::background(cx, move |core| call(core).map_err(Into::<UiError>::into));
        cx.spawn_in(window, async move |form, cx: &mut AsyncWindowContext| {
            let result = job.await;
            let _ = cx.update(|window, cx| {
                let open = form
                    .update(cx, |form, cx| {
                        form.saving = false;
                        cx.notify();
                    })
                    .is_ok();
                match result {
                    Ok(settings) => {
                        AppState::update(cx, |state, cx| state.set_settings(settings, cx));
                        if open {
                            window.close_dialog(cx);
                        }
                    }
                    Err(err) => Toast::error(err.message).show(window, cx),
                }
            });
        })
        .detach();
    }
}

impl Render for PriceForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let texts = self.texts(cx);
        let can_save = self.price(cx).is_some();
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let radius = cx.theme().radius;
        let fields = self.inputs.iter().enumerate().map(|(ix, input)| {
            let invalid = parse_price(&texts[ix]).is_err();
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .text_xs()
                .child(div().text_color(muted).child(LABELS[ix]))
                .child(
                    div()
                        .rounded(radius)
                        .when(invalid, |this| {
                            this.border_1().border_color(danger.opacity(0.6))
                        })
                        .child(Input::new(input).small()),
                )
        });

        v_flex()
            .gap_4()
            .child(div().text_sm().text_color(muted).child(format!(
                "What {} charges, in US dollars per million tokens. It is only used to \
                         estimate what categorization costs.",
                self.data.provider_name
            )))
            .child(h_flex().gap_3().items_start().children(fields))
            .child(div().text_xs().text_color(muted).child(
                "Cached input is a prompt the provider read from its cache, usually billed at \
                 a discount.",
            ))
            .child(
                DialogFooter::new()
                    .when(self.data.price.is_some(), |this| {
                        this.child(
                            div().mr_auto().child(
                                Button::new("remove-price")
                                    .ghost()
                                    .text_color(danger)
                                    .label("Remove price")
                                    .disabled(self.saving)
                                    .on_click(cx.listener(|form, _: &ClickEvent, window, cx| {
                                        form.remove(window, cx)
                                    })),
                            ),
                        )
                    })
                    .child(
                        Button::new("cancel")
                            .ghost()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save")
                            .primary()
                            .label("Save")
                            .disabled(!can_save || self.saving)
                            .on_click(cx.listener(|form, _: &ClickEvent, window, cx| {
                                form.save(window, cx)
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_parse_or_stay_empty() {
        assert_eq!(parse_price(""), Ok(None));
        assert_eq!(parse_price("  "), Ok(None));
        assert_eq!(parse_price(" 2.5 "), Ok(Some(2.5)));
        assert_eq!(parse_price("0"), Ok(Some(0.0)));
        assert_eq!(parse_price("-1"), Err(()));
        assert_eq!(parse_price("abc"), Err(()));
        assert_eq!(parse_price("inf"), Err(()));
        assert_eq!(parse_price("NaN"), Err(()));
    }

    #[test]
    fn a_price_needs_input_and_output() {
        let price = price_from(["2", "", "8"], "openai", "gpt").unwrap();
        assert_eq!(
            price,
            ModelPrice {
                provider: "openai".into(),
                model: "gpt".into(),
                input: 2.0,
                cached_input: None,
                output: 8.0,
            }
        );
        assert_eq!(
            price_from(["2", "0.5", "8"], "p", "m")
                .unwrap()
                .cached_input,
            Some(0.5)
        );
        assert!(price_from(["", "", "8"], "p", "m").is_none());
        assert!(price_from(["2", "", ""], "p", "m").is_none());
        assert!(price_from(["2", "x", "8"], "p", "m").is_none());
    }
}
