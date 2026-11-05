//! A row of buttons in a bordered box, one of them picked, like a period switch.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, Window, px,
};

pub fn segmented(
    id: impl Into<ElementId>,
    labels: Vec<SharedString>,
    selected: usize,
    on_select: impl Fn(&usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let on_select = Rc::new(on_select);
    h_flex()
        .id(id)
        .gap_1()
        .p(px(2.))
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .children(labels.into_iter().enumerate().map(|(ix, label)| {
            let on_select = on_select.clone();
            Button::new(ix)
                .small()
                .map(|this| {
                    if ix == selected {
                        this.secondary()
                    } else {
                        this.ghost()
                    }
                })
                .label(label)
                .on_click(move |_: &ClickEvent, window, cx| on_select(&ix, window, cx))
        }))
}
