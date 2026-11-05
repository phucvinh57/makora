//! Every activity block, newest first, a page at a time: searched, filtered by category, label
//! source and time, and recategorized one row or a selection at a time with Undo on the toast.

mod filters;
mod rows;
mod table;
mod time_range;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::Local;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IndexPath, Sizable, StyledExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window, div, px,
};
use makora_lib::categorization::domain::classification::ActivityClassification;
use makora_lib::domain::types::{SessionRow, SessionsPage};
use makora_lib::reporting::query;
use makora_lib::shared::ids::{CategoryId, SessionId};

use self::filters::{
    ALL_CATEGORIES, clamped_page, normalized, page_count, parse_source, session_query,
    source_value, time_range, with_category, with_query, with_range, with_source,
};
use self::rows::{
    bulk_toast, category_value, check_site_url, parse_category_value, row_category_toast,
};
use self::table::{ColumnWidths, ResizeOrigin};
use self::time_range::{TimeRangeFilter, TimeRangeFilterEvent};
use crate::ui::app_state::{AppState, Revisions};
use crate::ui::data;
use crate::ui::dialogs::add_category::{AddCategoryData, AddCategoryDialog};
use crate::ui::dialogs::{self};
use crate::ui::error::UiError;
use crate::ui::format::categories::{UNCATEGORIZED, UNCATEGORIZED_LABEL, category_paths};
use crate::ui::prefs::Prefs;
use crate::ui::route::HistoryFilters;
use crate::ui::screens::Screen;
use crate::ui::screens::categories::review::undo;
use crate::ui::shell::{SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_WIDTH};
use crate::ui::widgets::category_combobox::{
    CategoryChoice, CategoryCombobox, CategoryComboboxEvent,
};
use crate::ui::widgets::choice::Choice;
use crate::ui::widgets::feedback::ErrorBanner;
use crate::ui::widgets::pagination::Pagination;
use crate::ui::widgets::toast::Toast;

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);
/// The shell's padding around a screen, which this one fills to keep its header and pagination
/// in view while the rows scroll.
const CONTENT_PADDING: f32 = 52.;
const CONTENT_SIDE_PADDING: f32 = 48.;

/// The revisions the rows are built from.
fn watched(revisions: &Revisions) -> (u64, u64, u64) {
    (revisions.sessions, revisions.labels, revisions.categories)
}

fn source_choices() -> Vec<Choice> {
    vec![
        Choice::new(filters::ALL_SOURCES, "All sources"),
        Choice::new("manual", "Manual"),
        Choice::new("rule", "Rule"),
    ]
}

fn uncategorized_choice() -> CategoryChoice {
    CategoryChoice::new(UNCATEGORIZED, UNCATEGORIZED_LABEL)
}

pub(super) fn open_site(url: &str, window: &mut Window, cx: &mut App) {
    match check_site_url(url) {
        Ok(()) => cx.open_url(url),
        Err(err) => Toast::error("Could not open site")
            .description(err)
            .show(window, cx),
    }
}

fn show_with_undo(
    toast: Toast,
    prior: Vec<ActivityClassification>,
    window: &mut Window,
    cx: &mut App,
) {
    toast
        .action("Undo", move |_, cx| undo(prior.clone(), cx))
        .show(window, cx);
    AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
}

pub struct HistoryScreen {
    filters: HistoryFilters,
    search: Entity<InputState>,
    category_filter: Entity<CategoryCombobox>,
    source_select: Entity<SelectState<Vec<Choice>>>,
    time_range: Entity<TimeRangeFilter>,
    bulk_picker: Entity<CategoryCombobox>,
    /// The last page loaded and the filters it was loaded for; it stays on screen while the
    /// next filters load.
    page: Option<(HistoryFilters, SessionsPage)>,
    fetching: bool,
    error: Option<String>,
    selected: HashSet<SessionId>,
    bulk_category: Option<CategoryId>,
    applying: bool,
    pickers: HashMap<SessionId, (Entity<CategoryCombobox>, Subscription)>,
    widths: ColumnWidths,
    resize_origin: Option<ResizeOrigin>,
    resizing: Option<table::Column>,
    seen: (u64, u64, u64),
    load: Task<()>,
    commit_search: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl Screen for HistoryScreen {
    type Location = HistoryFilters;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filters = HistoryFilters::default();
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search app, title, or site…"));
        let category_filter = cx.new(|cx| {
            CategoryCombobox::new(
                Some(ALL_CATEGORIES.into()),
                vec![
                    CategoryChoice::new(ALL_CATEGORIES, "All categories"),
                    uncategorized_choice(),
                ],
                window,
                cx,
            )
            .small()
        });
        let source_select =
            cx.new(|cx| SelectState::new(source_choices(), Some(IndexPath::default()), window, cx));
        let time_range = cx.new(|cx| TimeRangeFilter::new(time_range(&filters), window, cx));
        let bulk_picker = cx.new(|cx| {
            CategoryCombobox::new(None, Vec::new(), window, cx)
                .placeholder("Set category…")
                .creatable()
                .small()
        });
        let app_state = AppState::entity(cx);
        let subscriptions = vec![
            cx.observe_in(&app_state, window, |this, state, window, cx| {
                let revisions = watched(&state.read(cx).revisions);
                if revisions != this.seen {
                    this.reload(window, cx);
                }
                cx.notify();
            }),
            cx.subscribe_in(
                &search,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::Change = event {
                        this.schedule_search(window, cx);
                    }
                },
            ),
            cx.subscribe_in(&category_filter, window, |this, _, event, window, cx| {
                if let CategoryComboboxEvent::Change(value) = event {
                    let next = with_category(&this.filters, value);
                    this.set_filters(next, window, cx);
                }
            }),
            cx.subscribe_in(
                &source_select,
                window,
                |this, _, event: &SelectEvent<Vec<Choice>>, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        let next = with_source(&this.filters, parse_source(value));
                        this.set_filters(next, window, cx);
                    }
                },
            ),
            cx.subscribe_in(&time_range, window, |this, _, event, window, cx| {
                let TimeRangeFilterEvent::Change(range) = event;
                let next = with_range(&this.filters, range.clone());
                this.set_filters(next, window, cx);
            }),
            cx.subscribe_in(
                &bulk_picker,
                window,
                |this, picker, event, window, cx| match event {
                    CategoryComboboxEvent::Change(value) => {
                        this.bulk_category = value.parse().ok().map(CategoryId);
                        cx.notify();
                    }
                    CategoryComboboxEvent::Create => {
                        let view = cx.entity().downgrade();
                        let picker = picker.downgrade();
                        dialogs::open::<AddCategoryDialog>(
                            AddCategoryData::new(None).on_created(move |created, window, cx| {
                                let id = created.id;
                                let _ = picker.update(cx, |picker, cx| {
                                    picker.set_value(Some(id.0.to_string().into()), window, cx)
                                });
                                let _ = view.update(cx, |this, cx| {
                                    this.bulk_category = Some(id);
                                    cx.notify();
                                });
                            }),
                            window,
                            cx,
                        );
                    }
                },
            ),
        ];
        let mut screen = Self {
            filters,
            search,
            category_filter,
            source_select,
            time_range,
            bulk_picker,
            page: None,
            fetching: false,
            error: None,
            selected: HashSet::new(),
            bulk_category: None,
            applying: false,
            pickers: HashMap::new(),
            widths: ColumnWidths::default(),
            resize_origin: None,
            resizing: None,
            seen: watched(&app_state.read(cx).revisions),
            load: Task::ready(()),
            commit_search: Task::ready(()),
            _subscriptions: subscriptions,
        };
        screen.reload(window, cx);
        screen
    }

    fn navigate(&mut self, filters: HistoryFilters, window: &mut Window, cx: &mut Context<Self>) {
        let filters = normalized(filters);
        if filters == self.filters {
            self.reload(window, cx);
        } else {
            self.set_filters(filters, window, cx);
        }
    }
}

impl HistoryScreen {
    fn rows(&self) -> &[SessionRow] {
        self.page.as_ref().map_or(&[], |(_, page)| &page.rows)
    }

    fn row(&self, id: SessionId) -> Option<&SessionRow> {
        self.rows().iter().find(|row| row.id == id)
    }

    fn total(&self) -> i64 {
        self.page.as_ref().map_or(0, |(_, page)| page.total)
    }

    /// Whether the rows on screen belong to other filters, while these load.
    fn showing_previous(&self) -> bool {
        self.page
            .as_ref()
            .is_some_and(|(filters, _)| *filters != self.filters)
    }

    /// The ticked rows still on screen, which a reload can change.
    fn selected_row_ids(&self) -> Vec<SessionId> {
        self.rows()
            .iter()
            .filter(|row| self.selected.contains(&row.id))
            .map(|row| row.id)
            .collect()
    }

    /// Applies new filters: the selection starts over and the controls follow.
    fn set_filters(
        &mut self,
        filters: HistoryFilters,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filters = normalized(filters);
        if filters == self.filters {
            return;
        }
        let query_changed = filters.q != self.filters.q;
        self.filters = filters;
        self.clear_selection(window, cx);
        self.sync_controls(query_changed, window, cx);
        self.reload(window, cx);
    }

    /// Mirrors the filters in the header; the search box only follows a query set elsewhere, so
    /// text still being typed survives other filter changes.
    fn sync_controls(&mut self, query_changed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.filters.q.clone();
        let typed = self.search.read(cx).value();
        if query_changed && typed.trim() != q {
            self.commit_search = Task::ready(());
            self.search
                .update(cx, |input, cx| input.set_value(q, window, cx));
        }
        let category = self
            .filters
            .category
            .clone()
            .unwrap_or_else(|| ALL_CATEGORIES.into());
        self.category_filter.update(cx, |picker, cx| {
            picker.set_value(Some(category.into()), window, cx)
        });
        let source = source_value(self.filters.source);
        self.source_select.update(cx, |select, cx| {
            select.set_selected_value(&source, window, cx)
        });
        let range = time_range(&self.filters);
        self.time_range
            .update(cx, |filter, cx| filter.set_range(range, cx));
    }

    fn schedule_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.search.read(cx).value().to_string();
        self.commit_search = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if text.trim() != this.filters.q {
                    let next = with_query(&this.filters, &text);
                    this.set_filters(next, window, cx);
                }
            });
        });
    }

    fn go_to_page(&mut self, page: u32, window: &mut Window, cx: &mut Context<Self>) {
        let next = HistoryFilters {
            page,
            ..self.filters.clone()
        };
        self.set_filters(next, window, cx);
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.seen = watched(&AppState::read(cx).revisions);
        self.fetching = true;
        let filters = self.filters.clone();
        let request = session_query(&filters, Local::now());
        self.load = data::fetch_in(
            window,
            cx,
            move |core| query::list_sessions(&*core.history, request).map_err(UiError::from),
            move |this, result, window, cx| {
                this.fetching = false;
                match result {
                    Ok(page) => {
                        this.error = None;
                        let past_end = clamped_page(filters.page, page.total);
                        this.page = Some((filters, page));
                        this.sync_pickers(window, cx);
                        // A link or a shrinking list can land past the last page; settle there.
                        if let Some(last) = past_end {
                            this.go_to_page(last, window, cx);
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to load the history");
                        this.error = Some(err.to_string());
                    }
                }
            },
        );
        cx.notify();
    }

    /// Keeps a category picker per row on screen, each showing its row's category.
    fn sync_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows: Vec<SessionRow> = self.rows().to_vec();
        let ids: HashSet<SessionId> = rows.iter().map(|row| row.id).collect();
        self.pickers.retain(|id, _| ids.contains(id));
        for row in &rows {
            let value = SharedString::from(category_value(row));
            match self.pickers.get(&row.id) {
                Some((picker, _)) => picker.update(cx, |picker, cx| {
                    if picker.value() != Some(&value) {
                        picker.set_value(Some(value), window, cx)
                    }
                }),
                None => {
                    let picker = self.row_picker(row.id, value, window, cx);
                    self.pickers.insert(row.id, picker);
                }
            }
        }
    }

    fn row_picker(
        &self,
        id: SessionId,
        value: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<CategoryCombobox>, Subscription) {
        let picker = cx.new(|cx| {
            CategoryCombobox::new(Some(value), vec![uncategorized_choice()], window, cx)
                .creatable()
                .small()
        });
        let subscription =
            cx.subscribe_in(
                &picker,
                window,
                move |this, _, event, window, cx| match event {
                    CategoryComboboxEvent::Change(value) => {
                        let Some(row) = this.row(id) else { return };
                        let Ok(category) = parse_category_value(value) else {
                            return;
                        };
                        if category != row.category_id {
                            this.set_row_category(id, category, window, cx);
                        }
                    }
                    CategoryComboboxEvent::Create => {
                        let view = cx.entity().downgrade();
                        dialogs::open::<AddCategoryDialog>(
                            AddCategoryData::new(None).on_created(move |created, window, cx| {
                                let category = created.id;
                                let _ = view.update(cx, |this, cx| {
                                    this.set_row_category(id, Some(category), window, cx)
                                });
                            }),
                            window,
                            cx,
                        );
                    }
                },
            );
        (picker, subscription)
    }

    fn set_row_category(
        &mut self,
        id: SessionId,
        category: Option<CategoryId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.row(id).cloned() else {
            return;
        };
        data::fetch_in(
            window,
            cx,
            move |core| {
                core.classification
                    .assign_sessions(&[id], category)
                    .map_err(UiError::from)
            },
            move |this, result, window, cx| match result {
                Ok(prior) => {
                    let path = category.map(|id| {
                        category_paths(&AppState::read(cx).categories)
                            .remove(&id)
                            .unwrap_or_else(|| "category".into())
                    });
                    let toast = row_category_toast(&row, path.as_deref());
                    let mut shown = Toast::new(toast.kind, toast.message);
                    if let Some(description) = toast.description {
                        shown = shown.description(description);
                    }
                    show_with_undo(shown, prior, window, cx);
                }
                Err(err) => {
                    Toast::error(err.to_string()).show(window, cx);
                    this.sync_pickers(window, cx);
                }
            },
        )
        .detach();
    }

    fn accept_ai_label(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(activity) = self.row(id).map(|row| row.activity_id) else {
            return;
        };
        data::fetch_in(
            window,
            cx,
            move |core| {
                core.classification
                    .accept_ai_labels(&[activity])
                    .map_err(UiError::from)
            },
            |_, result, window, cx| match result {
                Ok(prior) => show_with_undo(
                    Toast::success("Accepted the AI suggestion"),
                    prior,
                    window,
                    cx,
                ),
                Err(err) => Toast::error(err.to_string()).show(window, cx),
            },
        )
        .detach();
    }

    fn apply_bulk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(category) = self.bulk_category else {
            return;
        };
        let ids = self.selected_row_ids();
        if ids.is_empty() || self.applying {
            return;
        }
        self.applying = true;
        data::fetch_in(
            window,
            cx,
            move |core| {
                let prior = core
                    .classification
                    .assign_sessions(&ids, Some(category))
                    .map_err(UiError::from);
                (ids.len(), prior)
            },
            |this, (count, result), window, cx| {
                this.applying = false;
                match result {
                    Ok(prior) => {
                        this.clear_selection(window, cx);
                        show_with_undo(Toast::success(bulk_toast(count)), prior, window, cx);
                    }
                    Err(err) => Toast::error(err.to_string()).show(window, cx),
                }
            },
        )
        .detach();
        cx.notify();
    }

    fn clear_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected.clear();
        self.bulk_category = None;
        self.bulk_picker
            .update(cx, |picker, cx| picker.set_value(None, window, cx));
        cx.notify();
    }

    fn toggle_row(&mut self, id: SessionId, on: bool, cx: &mut Context<Self>) {
        if on {
            self.selected.insert(id);
        } else {
            self.selected.remove(&id);
        }
        cx.notify();
    }

    fn toggle_all(&mut self, on: bool, cx: &mut Context<Self>) {
        self.selected = if on {
            self.rows().iter().map(|row| row.id).collect()
        } else {
            HashSet::new()
        };
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                v_flex()
                    .mr_auto()
                    .child(div().text_lg().font_semibold().child("History"))
                    .child(div().text_sm().text_color(muted).child("App-focus history")),
            )
            .child(
                div().flex_1().min_w(px(160.)).max_w(px(256.)).child(
                    Input::new(&self.search)
                        .small()
                        .aria_label("Search history"),
                ),
            )
            .child(
                div()
                    .id("category-filter")
                    .w(px(160.))
                    .tooltip(|window, cx| {
                        Tooltip::new("Picking a category also shows its subcategories")
                            .build(window, cx)
                    })
                    .child(self.category_filter.clone()),
            )
            .child(
                div().w(px(128.)).child(
                    Select::new(&self.source_select)
                        .small()
                        .accessibility_label("Category source"),
                ),
            )
            .child(self.time_range.clone())
            .child(
                Button::new("refresh-history")
                    .outline()
                    .small()
                    .icon(IconName::RefreshCw)
                    .loading(self.fetching)
                    .loading_icon(Icon::new(IconName::RefreshCw))
                    .tooltip("Refresh history")
                    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
            )
    }

    fn render_bulk_bar(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .flex_wrap()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.5))
            .px_3()
            .py_2()
            .text_sm()
            .child(format!("{count} selected"))
            .child(div().w(px(160.)).child(self.bulk_picker.clone()))
            .child(
                Button::new("apply-bulk")
                    .primary()
                    .small()
                    .label("Apply")
                    .disabled(self.bulk_category.is_none() || self.applying)
                    .on_click(cx.listener(|this, _, window, cx| this.apply_bulk(window, cx))),
            )
            .child(
                Button::new("clear-selection")
                    .outline()
                    .small()
                    .label("Clear selection")
                    .on_click(cx.listener(|this, _, window, cx| this.clear_selection(window, cx))),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child("Applies to the selected activities only"),
            )
    }
}

impl Render for HistoryScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let sidebar = if Prefs::get(cx).sidebar_collapsed {
            SIDEBAR_COLLAPSED_WIDTH
        } else {
            SIDEBAR_WIDTH
        };
        let width = f32::from(viewport.width) - sidebar - CONTENT_SIDE_PADDING;
        let error = match (&self.error, &AppState::read(cx).categories_error) {
            (Some(err), _) => Some(format!("Failed to load sessions: {err}")),
            (None, Some(err)) => Some(format!("Failed to load categories: {err}")),
            (None, None) => None,
        }
        .map(ErrorBanner::new);
        let selected = self.selected_row_ids().len();
        let total = self.total();
        let page = self.filters.page;
        let view = cx.entity().downgrade();
        v_flex()
            .h(viewport.height - px(CONTENT_PADDING))
            .child(div().pb_4().child(self.render_header(cx)))
            .when_some(error, |this, error| this.child(div().pb_3().child(error)))
            .when(selected > 0, |this| {
                this.child(div().pb_3().child(self.render_bulk_bar(selected, cx)))
            })
            .child(self.render_table(width, cx))
            .child(
                div().pt_3().child(
                    Pagination::new(
                        "history-pages",
                        page as usize - 1,
                        page_count(total) as usize,
                        format!("{total} sessions"),
                    )
                    .on_page(move |page, window, cx| {
                        let _ = view
                            .update(cx, |this, cx| this.go_to_page(page as u32 + 1, window, cx));
                    }),
                ),
            )
    }
}
