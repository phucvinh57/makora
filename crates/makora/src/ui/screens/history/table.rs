//! The sessions table: a checkbox column and six resizable ones under a header that stays put
//! while the rows scroll. Site and Start drop out on a narrow window; Window title takes the
//! slack and never goes below its own width.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, DragMoveEvent, Empty, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use makora_lib::domain::types::{CategorySource, SessionRow};

use super::HistoryScreen;
use super::rows::{category_title, display_title, full_title};
use crate::ui::app_state::AppState;
use crate::ui::format::categories::category_paths;
use crate::ui::format::duration::{format_duration, format_timestamp};
use crate::ui::widgets::tri_checkbox::{CheckState, TriCheckbox};

const SELECT_WIDTH: f32 = 40.;
/// Container widths at which Start, then Site, fit: the web UI's `@2xl` and `@4xl`.
const START_FITS: f32 = 672.;
const SITE_FITS: f32 = 896.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    App,
    Category,
    Site,
    Title,
    Start,
    Duration,
}

impl Column {
    const ALL: [Column; 6] = [
        Column::App,
        Column::Category,
        Column::Site,
        Column::Title,
        Column::Start,
        Column::Duration,
    ];

    fn index(self) -> usize {
        self as usize
    }

    fn label(self) -> &'static str {
        match self {
            Self::App => "App",
            Self::Category => "Category",
            Self::Site => "Site",
            Self::Title => "Window title",
            Self::Start => "Start",
            Self::Duration => "Duration",
        }
    }

    fn default_width(self) -> f32 {
        match self {
            Self::App => 120.,
            Self::Category => 224.,
            Self::Site => 208.,
            Self::Title => 280.,
            Self::Start => 160.,
            Self::Duration => 80.,
        }
    }

    fn min_width(self) -> f32 {
        match self {
            Self::App => 96.,
            Self::Category => 144.,
            Self::Site => 128.,
            Self::Title => 160.,
            Self::Start => 128.,
            Self::Duration => 64.,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnWidths([f32; 6]);

impl Default for ColumnWidths {
    fn default() -> Self {
        Self(Column::ALL.map(Column::default_width))
    }
}

impl ColumnWidths {
    pub fn get(&self, column: Column) -> f32 {
        self.0[column.index()]
    }

    pub fn resize(&mut self, column: Column, width: f32) {
        self.0[column.index()] = width.max(column.min_width());
    }

    pub fn reset(&mut self, column: Column) {
        self.0[column.index()] = column.default_width();
    }

    /// The table's narrowest width: every shown column at its width.
    pub fn total(&self, columns: &[Column]) -> f32 {
        SELECT_WIDTH + columns.iter().map(|column| self.get(*column)).sum::<f32>()
    }
}

/// The columns that fit a table `width` wide.
pub fn visible_columns(width: f32) -> Vec<Column> {
    Column::ALL
        .into_iter()
        .filter(|column| match column {
            Column::Site => width >= SITE_FITS,
            Column::Start => width >= START_FITS,
            _ => true,
        })
        .collect()
}

/// Where a column resize started: the pointer and the column's width then.
#[derive(Debug, Clone, Copy)]
pub struct ResizeOrigin {
    column: Column,
    x: Pixels,
    width: f32,
}

#[derive(Clone)]
struct ResizeColumn(Column);

impl Render for ResizeColumn {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

fn cell(column: Column, widths: &ColumnWidths) -> gpui_kit::Div {
    let width = px(widths.get(column));
    match column {
        Column::Title => div().flex_1().min_w(width),
        _ => div().w(width).flex_shrink_0(),
    }
    .px_2()
    .overflow_hidden()
    .when(column == Column::Duration, |this| this.text_right())
}

fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut gpui_kit::App) -> gpui_kit::AnyView {
    let text = text.into();
    move |window, cx| Tooltip::new(text.clone()).build(window, cx)
}

impl HistoryScreen {
    fn resize_handle(&self, column: Column, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let resizing = self.resizing == Some(column);
        let group = SharedString::from(format!("resize-{}", column.index()));
        let (border, primary) = (theme.border, theme.primary);
        div()
            .id(("resize", column.index()))
            .group(group.clone())
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .w_2()
            .cursor_col_resize()
            .tooltip(tooltip("Drag to resize. Double-click to reset."))
            .child(
                div()
                    .absolute()
                    .top_2()
                    .bottom_2()
                    .left(px(3.5))
                    .w(px(1.))
                    .map(|this| {
                        if resizing {
                            this.bg(primary)
                        } else {
                            this.group_hover(group, move |style| style.bg(border))
                        }
                    }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if event.click_count == 2 {
                        this.widths.reset(column);
                        this.resize_origin = None;
                    } else {
                        this.resize_origin = Some(ResizeOrigin {
                            column,
                            x: event.position.x,
                            width: this.widths.get(column),
                        });
                    }
                    cx.notify();
                }),
            )
            .on_drag(ResizeColumn(column), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<ResizeColumn>, _, cx| {
                    if event.drag(cx).0 != column {
                        return;
                    }
                    let Some(origin) = this.resize_origin.filter(|o| o.column == column) else {
                        return;
                    };
                    this.resizing = Some(column);
                    let moved = f32::from(event.event.position.x - origin.x);
                    this.widths.resize(column, origin.width + moved);
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_resize(cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_resize(cx)),
            )
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        self.resize_origin = None;
        if self.resizing.take().is_some() {
            cx.notify();
        }
    }

    fn render_head(&self, columns: &[Column], cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rows = self.rows();
        let selected = rows
            .iter()
            .filter(|row| self.selected.contains(&row.id))
            .count();
        let state = CheckState::from_counts(selected, rows.len());
        let disabled = self.showing_previous() || rows.is_empty();
        let border = theme.border;
        let view = cx.entity().downgrade();
        let heads: Vec<AnyElement> = columns
            .iter()
            .map(|&column| {
                cell(column, &self.widths)
                    .relative()
                    .pr_3()
                    .whitespace_nowrap()
                    .child(column.label())
                    .child(self.resize_handle(column, cx))
                    .into_any_element()
            })
            .collect();
        h_flex()
            .w_full()
            .flex_shrink_0()
            .h_10()
            .border_b_1()
            .border_color(border)
            .text_sm()
            .font_medium()
            .child(
                div().w(px(SELECT_WIDTH)).flex_shrink_0().pl_3().child(
                    TriCheckbox::new("select-all", state)
                        .disabled(disabled)
                        .on_toggle(move |checked, _, cx| {
                            let _ = view.update(cx, |this, cx| this.toggle_all(checked, cx));
                        }),
                ),
            )
            .children(heads)
    }

    fn render_row(
        &self,
        ix: usize,
        row: &SessionRow,
        columns: &[Column],
        path: Option<&str>,
        now: chrono::DateTime<chrono::Local>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (muted, primary, mono) = (
            theme.muted_foreground,
            theme.primary,
            theme.mono_font_family.clone(),
        );
        let hover = theme.muted.opacity(0.5);
        let border = theme.border;
        let checked = self.selected.contains(&row.id);
        let id = row.id;
        let picker = self.pickers.get(&id).map(|(picker, _)| picker.clone());
        let view = cx.entity().downgrade();
        let cells: Vec<AnyElement> = columns
            .iter()
            .map(|&column| {
                let base = cell(column, &self.widths);
                match column {
                    Column::App => base
                        .id(("app", ix))
                        .truncate()
                        .tooltip(tooltip(row.source_key.clone()))
                        .child(row.app_name.clone())
                        .into_any_element(),
                    Column::Category => base
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    div()
                                        .id(("category", ix))
                                        .flex_1()
                                        .min_w_0()
                                        .tooltip(tooltip(category_title(row, path)))
                                        .children(picker.clone()),
                                )
                                .map(|this| {
                                    if row.category_source == Some(CategorySource::Ai) {
                                        this.child(
                                            Button::new(("accept", ix))
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::Check)
                                                .tooltip("Accept this AI label")
                                                .accessibility_label("Accept AI label")
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.accept_ai_label(id, window, cx)
                                                    },
                                                )),
                                        )
                                    } else {
                                        this.child(div().size_6().flex_shrink_0())
                                    }
                                }),
                        )
                        .into_any_element(),
                    Column::Site => match row.url.clone() {
                        Some(url) => base
                            .child(
                                div()
                                    .id(("site", ix))
                                    .truncate()
                                    .font_family(mono.clone())
                                    .text_xs()
                                    .text_color(primary)
                                    .cursor_pointer()
                                    .hover(|style| style.underline())
                                    .tooltip(tooltip(url.clone()))
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        super::open_site(&url, window, cx)
                                    }))
                                    .child(row.url.clone().unwrap_or_default()),
                            )
                            .into_any_element(),
                        None => base.into_any_element(),
                    },
                    Column::Title => base
                        .id(("title", ix))
                        .truncate()
                        .tooltip(tooltip(full_title(row)))
                        .child(display_title(row).to_owned())
                        .into_any_element(),
                    Column::Start => base
                        .truncate()
                        .text_color(muted)
                        .child(format_timestamp(&row.start_time))
                        .into_any_element(),
                    Column::Duration => base
                        .child(format_duration(
                            &row.start_time,
                            row.end_time.as_deref(),
                            now,
                        ))
                        .into_any_element(),
                }
            })
            .collect();
        h_flex()
            .id(("row", ix))
            .w_full()
            .flex_shrink_0()
            .min_h_10()
            .py_1()
            .border_b_1()
            .border_color(border)
            .text_sm()
            .hover(move |style| style.bg(hover))
            .child(
                div().w(px(SELECT_WIDTH)).flex_shrink_0().pl_3().child(
                    TriCheckbox::new(
                        ("select", ix),
                        if checked {
                            CheckState::Checked
                        } else {
                            CheckState::Unchecked
                        },
                    )
                    .disabled(self.showing_previous())
                    .on_toggle(move |on, _, cx| {
                        let _ = view.update(cx, |this, cx| this.toggle_row(id, on, cx));
                    }),
                ),
            )
            .children(cells)
            .into_any_element()
    }

    pub(super) fn render_table(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let columns = visible_columns(width);
        let header = self.render_head(&columns, cx).into_any_element();
        let paths = category_paths(&AppState::read(cx).categories);
        let now = chrono::Local::now();
        let rows: Vec<AnyElement> = self
            .rows()
            .iter()
            .enumerate()
            .map(|(ix, row)| {
                let path = row
                    .category_id
                    .and_then(|id| paths.get(&id))
                    .map(String::as_str);
                self.render_row(ix, row, &columns, path, now, cx)
            })
            .collect();
        let empty = !self.fetching && rows.is_empty() && self.page.is_some();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // Fixed rather than stretched: a scrolling container would otherwise size the rows to
        // their longest title.
        let table_width = (width - 2.).max(self.widths.total(&columns));
        div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(crate::ui::theme::card(cx))
            .child(
                div()
                    .id("history-table")
                    .size_full()
                    .overflow_x_scrollbar()
                    .child(
                        v_flex().h_full().w(px(table_width)).child(header).child(
                            div()
                                .id("history-rows")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scrollbar()
                                .children(rows)
                                .when(empty, |this| {
                                    this.child(
                                        div()
                                            .py_8()
                                            .text_center()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(if super::filters::is_filtered(&self.filters) {
                                                "No sessions match this filter."
                                            } else {
                                                "No sessions recorded yet."
                                            }),
                                    )
                                }),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_never_go_below_the_minimum_and_reset_to_the_default() {
        let mut widths = ColumnWidths::default();
        widths.resize(Column::App, 10.);
        assert_eq!(widths.get(Column::App), 96.);
        widths.resize(Column::App, 300.);
        assert_eq!(widths.get(Column::App), 300.);
        widths.reset(Column::App);
        assert_eq!(widths.get(Column::App), 120.);
    }

    #[test]
    fn narrow_tables_drop_site_then_start() {
        let all = visible_columns(1000.);
        assert_eq!(all.len(), 6);
        let medium = visible_columns(700.);
        assert!(!medium.contains(&Column::Site) && medium.contains(&Column::Start));
        let narrow = visible_columns(600.);
        assert_eq!(
            narrow,
            [
                Column::App,
                Column::Category,
                Column::Title,
                Column::Duration
            ]
        );
    }

    #[test]
    fn the_table_is_as_wide_as_its_columns() {
        let widths = ColumnWidths::default();
        assert_eq!(
            widths.total(&[Column::App, Column::Duration]),
            SELECT_WIDTH + 200.
        );
    }
}
