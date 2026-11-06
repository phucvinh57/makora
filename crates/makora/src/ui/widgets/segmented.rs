//! A small segmented switch, like a period picker, whose highlight slides to the picked option.

use gpui_kit::component::Sizable;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::{App, ElementId, IntoElement, SharedString, Window};

pub fn segmented(
    id: impl Into<ElementId>,
    labels: Vec<SharedString>,
    selected: usize,
    on_select: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    TabBar::new(id)
        .segmented()
        .small()
        .selected_index(selected)
        .children(labels.into_iter().map(|label| Tab::new().label(label)))
        .on_click(on_select)
}
