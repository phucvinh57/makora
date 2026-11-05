//! The category tree as cards, each indented under its parent.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{App, ElementId, IntoElement, ParentElement, SharedString, Styled, div, px};
use makora_lib::domain::types::Category;

use crate::ui::app_state::AppState;
use crate::ui::dialogs::add_category::{AddCategoryData, AddCategoryDialog};
use crate::ui::dialogs::delete_category::DeleteCategoryDialog;
use crate::ui::dialogs::edit_category::EditCategoryDialog;
use crate::ui::dialogs::{self};
use crate::ui::format::categories::{build_category_tree, flatten_category_tree};
use crate::ui::widgets::card::Card;
use crate::ui::widgets::category_dot::category_color;
use crate::ui::widgets::feedback::ErrorBanner;

const INDENT_PER_LEVEL: f32 = 24.;

pub fn subcategory_label(count: usize) -> String {
    let noun = if count == 1 {
        "subcategory"
    } else {
        "subcategories"
    };
    format!("{count} {noun}")
}

pub fn render(cx: &App) -> impl IntoElement + use<> {
    let state = AppState::read(cx);
    let tree = build_category_tree(&state.categories);
    let cards: Vec<_> = flatten_category_tree(&tree)
        .into_iter()
        .map(|node| category_card(&node.category, node.depth, node.children.len(), cx))
        .collect();
    let empty = state.has_no_categories();

    v_flex()
        .gap_4()
        .when_some(state.categories_error.clone(), |this, error| {
            this.child(ErrorBanner::new(error))
        })
        .child(
            h_flex().justify_end().child(
                Button::new("add-category")
                    .primary()
                    .small()
                    .label("Add category")
                    .on_click(|_, window, cx| {
                        dialogs::open::<AddCategoryDialog>(AddCategoryData::new(None), window, cx)
                    }),
            ),
        )
        .when(empty, |this| {
            this.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "No categories yet. Use “Add category” above to begin; the AI sorts \
                         your activities into them.",
                    ),
            )
        })
        .child(v_flex().gap_3().children(cards))
}

fn category_card(
    category: &Category,
    depth: usize,
    subcategories: usize,
    cx: &App,
) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let id = category.id.0;
    let button_id = |kind: &str| ElementId::Name(SharedString::from(format!("{kind}-{id}")));
    let add = category.clone();
    let edit = category.clone();
    let delete = category.clone();

    Card::new()
        .small()
        .ml(px(depth as f32 * INDENT_PER_LEVEL))
        .child(
            h_flex()
                .gap_2()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .size(px(10.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(category_color(&category.color)),
                                )
                                .child(div().font_medium().truncate().child(category.name.clone()))
                                .when(subcategories > 0, |this| {
                                    this.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(subcategory_label(subcategories)),
                                    )
                                }),
                        )
                        .when_some(category.description.clone(), |this, description| {
                            this.child(div().text_xs().text_color(muted).child(description))
                        }),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new(button_id("add"))
                                .ghost()
                                .small()
                                .icon(IconName::Plus)
                                .tooltip("Add subcategory")
                                .on_click(move |_, window, cx| {
                                    dialogs::open::<AddCategoryDialog>(
                                        AddCategoryData::new(Some(add.clone())),
                                        window,
                                        cx,
                                    )
                                }),
                        )
                        .child(
                            Button::new(button_id("edit"))
                                .ghost()
                                .small()
                                .icon(IconName::Pencil)
                                .tooltip("Edit category")
                                .on_click(move |_, window, cx| {
                                    dialogs::open::<EditCategoryDialog>(edit.clone(), window, cx)
                                }),
                        )
                        .child(
                            Button::new(button_id("delete"))
                                .danger()
                                .small()
                                .icon(IconName::Trash)
                                .tooltip("Delete category")
                                .on_click(move |_, window, cx| {
                                    dialogs::open::<DeleteCategoryDialog>(
                                        delete.clone(),
                                        window,
                                        cx,
                                    )
                                }),
                        ),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcategories_are_counted_in_words() {
        assert_eq!(subcategory_label(1), "1 subcategory");
        assert_eq!(subcategory_label(4), "4 subcategories");
    }
}
