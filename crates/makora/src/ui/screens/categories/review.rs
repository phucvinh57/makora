//! The AI's pending labels, one row per source and suggested category. A row expands into its
//! activities; either level can be accepted, rejected or moved to another category, alone or as
//! a selection, with Undo on the toast.

use std::collections::{HashMap, HashSet};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, ElementId, Entity, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, WeakEntity, Window, div, px,
};
use makora_lib::categorization::domain::classification::ActivityClassification;
use makora_lib::domain::types::{AiReviewGroup, AiReviewGroupPage, AiReviewPage, ReviewFilter};
use makora_lib::shared::ids::{ActivityId, CategoryId};
use makora_lib::tracking::domain::source_key::KeyKind;

use crate::runtime::window::show_main_window;
use crate::ui::app_state::{AppState, Core};
use crate::ui::data;
use crate::ui::dialogs::add_category::{AddCategoryData, AddCategoryDialog};
use crate::ui::dialogs::{self};
use crate::ui::error::UiError;
use crate::ui::format::categories::{UNCATEGORIZED, category_paths};
use crate::ui::format::duration::format_seconds;
use crate::ui::route::{HistoryFilters, Route};
use crate::ui::widgets::category_combobox::{
    CategoryChoice, CategoryCombobox, CategoryComboboxEvent,
};
use crate::ui::widgets::pagination::Pagination;
use crate::ui::widgets::toast::Toast;
use crate::ui::widgets::tri_checkbox::{CheckState, TriCheckbox};

const GROUP_PAGE_SIZE: i64 = 20;
const MEMBER_PAGE_SIZE: i64 = 20;
const ALL: &str = "all";

const SELECT_WIDTH: f32 = 56.;
const ACTIVITIES_WIDTH: f32 = 96.;
const TIME_WIDTH: f32 = 80.;
const CATEGORY_WIDTH: f32 = 224.;
const ACTIONS_WIDTH: f32 = 80.;

pub fn group_id(group: &AiReviewGroup) -> String {
    format!(
        "{}:{}:{}",
        group.key_kind.as_str(),
        group.key,
        group.category_id.0
    )
}

pub fn source_name(group: &AiReviewGroup) -> &str {
    if group.key_kind == KeyKind::Domain {
        &group.key
    } else {
        &group.app_name
    }
}

pub fn page_count(total: i64, page_size: i64) -> usize {
    ((total.max(0) + page_size - 1) / page_size).max(1) as usize
}

pub fn count_activities(count: usize) -> String {
    if count == 1 {
        "1 activity".into()
    } else {
        format!("{count} activities")
    }
}

pub fn accepted_message(count: usize) -> String {
    if count == 1 {
        "Accepted the AI suggestion".into()
    } else {
        format!("Accepted {count} AI suggestions")
    }
}

/// The toast after moving `count` activities: rejected when there is no category.
pub fn moved_message(count: usize, category: Option<&str>) -> String {
    let count = count_activities(count);
    match category {
        None => format!("Rejected the AI suggestion for {count}"),
        Some(path) => format!("Set {count} to {path}"),
    }
}

/// Puts the classifications back as they were before a decision, from a toast's Undo.
pub(crate) fn undo(prior: Vec<ActivityClassification>, cx: &mut App) {
    let core = Core::get(cx);
    let job = cx
        .background_executor()
        .spawn(async move { core.classification.undo_assignment(&prior) });
    cx.spawn(async move |cx| {
        let result = job.await.map_err(UiError::from);
        cx.update(|cx| match result {
            Ok(()) => {
                Toast::success("Undone").show_in_main_window(cx);
                AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
            }
            Err(err) => Toast::error(err.to_string()).show_in_main_window(cx),
        });
    })
    .detach();
}

fn stop_click(element: gpui_kit::Div) -> gpui_kit::Div {
    element.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

fn cell(width: f32) -> gpui_kit::Div {
    div().flex_shrink_0().w(px(width)).px_2()
}

fn decision_buttons(
    id: impl Into<SharedString>,
    ids: Vec<ActivityId>,
    noun: String,
    busy: bool,
    queue: WeakEntity<ReviewQueue>,
) -> impl IntoElement {
    let id = id.into();
    let accept_ids = ids.clone();
    let accept_queue = queue.clone();
    h_flex()
        .justify_end()
        .gap_1()
        .child(
            Button::new(ElementId::Name(format!("accept-{id}").into()))
                .ghost()
                .small()
                .icon(IconName::Check)
                .tooltip(format!("Accept {noun}"))
                .disabled(busy)
                .on_click(move |_, window, cx| {
                    let ids = accept_ids.clone();
                    let _ = accept_queue.update(cx, |queue, cx| queue.accept(ids, window, cx));
                }),
        )
        .child(
            Button::new(ElementId::Name(format!("reject-{id}").into()))
                .ghost()
                .small()
                .icon(IconName::X)
                .tooltip(format!("Reject {noun}: leave uncategorized"))
                .disabled(busy)
                .on_click(move |_, window, cx| {
                    let ids = ids.clone();
                    let _ = queue.update(cx, |queue, cx| queue.set_category(ids, None, window, cx));
                }),
        )
}

/// A creatable category picker that moves `ids()` when a different category is picked.
fn category_picker(
    category: CategoryId,
    queue: WeakEntity<ReviewQueue>,
    ids: impl Fn(&App) -> Vec<ActivityId> + 'static,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<CategoryCombobox>, Subscription) {
    let picker = cx.new(|cx| {
        CategoryCombobox::new(Some(category.0.to_string().into()), Vec::new(), window, cx)
            .creatable()
            .small()
    });
    let subscription = window.subscribe(&picker, cx, move |_, event, window, cx| {
        let ids = ids(cx);
        match event {
            CategoryComboboxEvent::Change(value) => {
                let Some(next) = value.parse().ok().map(CategoryId) else {
                    return;
                };
                if next != category {
                    let _ = queue.update(cx, |queue, cx| {
                        queue.set_category(ids, Some(next), window, cx)
                    });
                }
            }
            CategoryComboboxEvent::Create => {
                let queue = queue.clone();
                dialogs::open::<AddCategoryDialog>(
                    AddCategoryData::new(None).on_created(move |created, window, cx| {
                        let ids = ids.clone();
                        let id = created.id;
                        let _ = queue.update(cx, |queue, cx| {
                            queue.set_category(ids, Some(id), window, cx)
                        });
                    }),
                    window,
                    cx,
                );
            }
        }
    });
    (picker, subscription)
}

pub struct ReviewQueue {
    page: usize,
    category: Option<CategoryId>,
    expanded: HashSet<String>,
    selected: HashSet<String>,
    data: Option<AiReviewGroupPage>,
    fetching: bool,
    error: Option<String>,
    busy: bool,
    labels: Option<u64>,
    filter: Entity<CategoryCombobox>,
    pickers: HashMap<String, (Entity<CategoryCombobox>, Subscription)>,
    members: HashMap<String, Entity<GroupMembers>>,
    load: Task<()>,
    decision: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ReviewQueue {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            CategoryCombobox::new(
                Some(ALL.into()),
                vec![CategoryChoice::new(ALL, "All suggestions")],
                window,
                cx,
            )
            .small()
        });
        let subscriptions = vec![
            cx.observe_in(&AppState::entity(cx), window, |this, _, window, cx| {
                let state = AppState::read(cx);
                let labels = state.revisions.labels;
                let filter_gone = this.category.is_some_and(|id| {
                    state.categories_loaded
                        && state.categories_error.is_none()
                        && !state.categories.iter().any(|category| category.id == id)
                });
                if filter_gone {
                    this.category = None;
                    this.page = 0;
                    this.filter.update(cx, |filter, cx| {
                        filter.set_value(Some(ALL.into()), window, cx)
                    });
                    this.reload(window, cx);
                } else if this.labels != Some(labels) {
                    this.reload(window, cx);
                }
            }),
            cx.subscribe_in(&filter, window, |this, _, event, window, cx| {
                if let CategoryComboboxEvent::Change(value) = event {
                    this.category = value.parse().ok().map(CategoryId);
                    this.page = 0;
                    this.selected.clear();
                    this.expanded.clear();
                    this.members.clear();
                    this.reload(window, cx);
                }
            }),
        ];
        let mut queue = Self {
            page: 0,
            category: None,
            expanded: HashSet::new(),
            selected: HashSet::new(),
            data: None,
            fetching: false,
            error: None,
            busy: false,
            labels: None,
            filter,
            pickers: HashMap::new(),
            members: HashMap::new(),
            load: Task::ready(()),
            decision: Task::ready(()),
            _subscriptions: subscriptions,
        };
        queue.reload(window, cx);
        queue
    }

    fn groups(&self) -> &[AiReviewGroup] {
        self.data.as_ref().map_or(&[], |data| &data.groups)
    }

    fn total(&self) -> i64 {
        self.data.as_ref().map_or(0, |data| data.total)
    }

    fn group(&self, id: &str) -> Option<&AiReviewGroup> {
        self.groups().iter().find(|group| group_id(group) == id)
    }

    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.labels = Some(AppState::read(cx).revisions.labels);
        self.fetching = true;
        let page = self.page as i64;
        let filter = ReviewFilter {
            category: self.category,
            activities: None,
        };
        self.load = data::fetch_in(
            window,
            cx,
            move |core| {
                core.labeling
                    .get_review_groups(page, GROUP_PAGE_SIZE, &filter)
                    .map_err(UiError::from)
            },
            |this, result, window, cx| {
                this.fetching = false;
                match result {
                    Ok(data) => {
                        this.error = None;
                        this.data = Some(data);
                        this.sync_rows(window, cx);
                        // A decided group leaves the queue, so a page that emptied is given up.
                        let last = page_count(this.total(), GROUP_PAGE_SIZE) - 1;
                        if this.page > last {
                            this.page = last;
                            this.reload(window, cx);
                        }
                    }
                    Err(err) => this.error = Some(err.to_string()),
                }
            },
        );
        cx.notify();
    }

    /// Keeps a picker per visible group and the expanded groups' members in step with the data.
    fn sync_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let groups: Vec<AiReviewGroup> = self.groups().to_vec();
        let ids: HashSet<String> = groups.iter().map(group_id).collect();
        self.pickers.retain(|id, _| ids.contains(id));
        self.members.retain(|id, _| ids.contains(id));
        self.expanded.retain(|id| ids.contains(id));
        self.selected.retain(|id| ids.contains(id));
        let queue = cx.entity().downgrade();
        for group in &groups {
            let id = group_id(group);
            match self.pickers.get(&id) {
                Some((picker, _)) => picker.update(cx, |picker, cx| {
                    picker.set_value(Some(group.category_id.0.to_string().into()), window, cx)
                }),
                None => {
                    let this = queue.clone();
                    let key = id.clone();
                    let picker = category_picker(
                        group.category_id,
                        queue.clone(),
                        move |cx| {
                            this.upgrade()
                                .and_then(|queue| {
                                    queue.read(cx).group(&key).map(|g| g.activity_ids.clone())
                                })
                                .unwrap_or_default()
                        },
                        window,
                        cx,
                    );
                    self.pickers.insert(id.clone(), picker);
                }
            }
            if let Some(members) = self.members.get(&id) {
                let activity_ids = group.activity_ids.clone();
                members.update(cx, |members, cx| members.set_ids(activity_ids, window, cx));
            }
        }
    }

    fn toggle_expanded(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.expanded.remove(id) {
            self.members.remove(id);
        } else if let Some(group) = self.group(id).cloned() {
            self.expanded.insert(id.to_string());
            let queue = cx.entity().downgrade();
            let members = cx.new(|cx| GroupMembers::new(queue, group.activity_ids, window, cx));
            self.members.insert(id.to_string(), members);
        }
        cx.notify();
    }

    fn toggle_selected(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        if on {
            self.selected.insert(id.to_string());
        } else {
            self.selected.remove(id);
        }
        cx.notify();
    }

    fn selected_ids(&self) -> (usize, Vec<ActivityId>) {
        let groups: Vec<&AiReviewGroup> = self
            .groups()
            .iter()
            .filter(|group| self.selected.contains(&group_id(group)))
            .collect();
        let ids = groups
            .iter()
            .flat_map(|group| group.activity_ids.iter().copied())
            .collect();
        (groups.len(), ids)
    }

    fn accept(&mut self, ids: Vec<ActivityId>, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let count = ids.len();
        self.decision = data::fetch_in(
            window,
            cx,
            move |core| {
                core.classification
                    .accept_ai_labels(&ids)
                    .map_err(UiError::from)
            },
            move |this, result, window, cx| {
                this.decided(result, accepted_message(count), window, cx);
            },
        );
        cx.notify();
    }

    fn set_category(
        &mut self,
        ids: Vec<ActivityId>,
        category: Option<CategoryId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            self.reset_pickers(window, cx);
            return;
        }
        self.busy = true;
        let count = ids.len();
        self.decision = data::fetch_in(
            window,
            cx,
            move |core| {
                core.classification
                    .assign_activities(&ids, category)
                    .map_err(UiError::from)
            },
            move |this, result, window, cx| {
                let path = category.map(|id| {
                    category_paths(&AppState::read(cx).categories)
                        .remove(&id)
                        .unwrap_or_else(|| "category".into())
                });
                let message = moved_message(count, path.as_deref());
                this.decided(result, message, window, cx);
            },
        );
        cx.notify();
    }

    /// Puts every picker back on the category its row has now, undoing a pick that did not apply.
    fn reset_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_rows(window, cx);
        for members in self.members.values() {
            members.update(cx, |members, cx| members.sync_pickers(window, cx));
        }
    }

    fn decided(
        &mut self,
        result: Result<Vec<ActivityClassification>, UiError>,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        match result {
            Ok(prior) => {
                self.selected.clear();
                Toast::success(message)
                    .action("Undo", move |_, cx| undo(prior.clone(), cx))
                    .show(window, cx);
                AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
            }
            Err(err) => {
                Toast::error(err.to_string()).show(window, cx);
                self.reset_pickers(window, cx);
            }
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (groups, _) = self.selected_ids();
        let all = self.groups().iter().map(group_id).collect::<Vec<_>>();
        let state = CheckState::from_counts(groups, all.len());
        let this = cx.entity().downgrade();
        let head = |width: Option<f32>, right: bool, label: &'static str| {
            match width {
                Some(width) => cell(width),
                None => div().flex_1().min_w_0().px_2(),
            }
            .when(right, |this| this.text_right())
            .child(label)
        };
        h_flex()
            .h_10()
            .border_b_1()
            .border_color(theme.border)
            .text_sm()
            .font_medium()
            .child(
                cell(SELECT_WIDTH).child(
                    TriCheckbox::new("select-all", state)
                        .disabled(all.is_empty())
                        .on_toggle(move |checked, _, cx| {
                            let all = all.clone();
                            let _ = this.update(cx, |queue, cx| {
                                queue.selected = if checked {
                                    all.into_iter().collect()
                                } else {
                                    HashSet::new()
                                };
                                cx.notify();
                            });
                        }),
                ),
            )
            .child(head(None, false, "App or site"))
            .child(head(Some(ACTIVITIES_WIDTH), true, "Activities"))
            .child(head(Some(TIME_WIDTH), true, "Time"))
            .child(head(Some(CATEGORY_WIDTH), false, "Suggested category"))
            .child(cell(ACTIONS_WIDTH))
    }

    fn render_group(&self, ix: usize, group: &AiReviewGroup, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let id = group_id(group);
        let open = self.expanded.contains(&id);
        let checked = self.selected.contains(&id);
        let name = source_name(group).to_string();
        let queue = cx.entity().downgrade();
        let muted = theme.muted_foreground;
        let hover = theme.muted.opacity(0.5);
        let toggle_id = id.clone();
        let chevron_id = id.clone();
        let select_id = id.clone();
        let this = cx.entity().downgrade();
        let picker = self.pickers.get(&id).map(|(picker, _)| picker.clone());

        v_flex()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .id(("group", ix))
                    .py_1p5()
                    .text_sm()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_expanded(&toggle_id, window, cx)
                    }))
                    .child(
                        stop_click(cell(SELECT_WIDTH)).child(
                            h_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .id(("chevron", ix))
                                        .size(px(28.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .text_color(muted)
                                        .hover(|style| style.bg(hover))
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(if open {
                                                "Hide activities"
                                            } else {
                                                "Show activities"
                                            })
                                            .build(window, cx)
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.toggle_expanded(&chevron_id, window, cx)
                                        }))
                                        .child(
                                            Icon::new(if open {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            })
                                            .small(),
                                        ),
                                )
                                .child(
                                    TriCheckbox::new(
                                        ("select", ix),
                                        if checked {
                                            CheckState::Checked
                                        } else {
                                            CheckState::Unchecked
                                        },
                                    )
                                    .on_toggle(
                                        move |on, _, cx| {
                                            let _ = this.update(cx, |queue, cx| {
                                                queue.toggle_selected(&select_id, on, cx)
                                            });
                                        },
                                    ),
                                ),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .truncate()
                            .font_medium()
                            .child(name.clone()),
                    )
                    .child(
                        cell(ACTIVITIES_WIDTH)
                            .text_right()
                            .text_color(muted)
                            .child(group.activity_count.to_string()),
                    )
                    .child(
                        cell(TIME_WIDTH)
                            .text_right()
                            .text_color(muted)
                            .child(format_seconds(group.total_seconds)),
                    )
                    .child(stop_click(cell(CATEGORY_WIDTH)).children(picker))
                    .child(stop_click(cell(ACTIONS_WIDTH)).child(decision_buttons(
                        format!("group-{ix}"),
                        group.activity_ids.clone(),
                        format!("all {} in {name}", group.activity_count),
                        self.busy,
                        queue,
                    ))),
            )
            .when_some(self.members.get(&id).filter(|_| open), |this, members| {
                this.child(div().bg(theme.muted.opacity(0.3)).child(members.clone()))
            })
            .into_any_element()
    }
}

impl Render for ReviewQueue {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (selected_groups, selected_ids) = self.selected_ids();
        let groups: Vec<AiReviewGroup> = self.groups().to_vec();
        let total = self.total();
        let pages = page_count(total, GROUP_PAGE_SIZE);
        let header = self.render_header(cx).into_any_element();
        let rows: Vec<AnyElement> = groups
            .iter()
            .enumerate()
            .map(|(ix, group)| self.render_group(ix, group, cx))
            .collect();
        let empty = !self.fetching && groups.is_empty() && self.data.is_some();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let busy = self.busy;
        let accept_ids = selected_ids.clone();
        let reject_ids = selected_ids.clone();

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_x_4()
                    .gap_y_2()
                    .child(div().text_sm().font_semibold().child("Review AI labels"))
                    .child(div().flex_1())
                    .child(div().w(px(208.)).child(self.filter.clone()))
                    .child(
                        Button::new("view-uncategorized")
                            .outline()
                            .small()
                            .label("View uncategorized activities")
                            .on_click(|_, _, cx| {
                                let filters = HistoryFilters {
                                    category: Some(UNCATEGORIZED.into()),
                                    ..HistoryFilters::default()
                                };
                                cx.defer(move |cx| {
                                    show_main_window(Some(Route::History(filters)), cx)
                                });
                            }),
                    ),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(crate::ui::widgets::feedback::ErrorBanner::new(error))
            })
            .when(selected_groups > 0, |this| {
                this.child(
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
                        .child(format!(
                            "{selected_groups} selected ({} activities)",
                            selected_ids.len()
                        ))
                        .child(
                            Button::new("accept-selected")
                                .primary()
                                .small()
                                .label("Accept selected")
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.accept(accept_ids.clone(), window, cx)
                                })),
                        )
                        .child(
                            Button::new("reject-selected")
                                .outline()
                                .small()
                                .label("Reject selected")
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_category(reject_ids.clone(), None, window, cx)
                                })),
                        )
                        .child(
                            Button::new("clear-selection")
                                .ghost()
                                .small()
                                .label("Clear selection")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.selected.clear();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                v_flex()
                    .overflow_hidden()
                    .rounded(theme.radius_lg)
                    .border_1()
                    .border_color(theme.border)
                    .bg(crate::ui::theme::card(cx))
                    .child(header)
                    .children(rows)
                    .when(empty, |this| {
                        this.child(
                            div()
                                .py_8()
                                .text_center()
                                .text_sm()
                                .text_color(muted)
                                .child(if self.category.is_none() {
                                    "Nothing to review. Run “Categorize” to get AI suggestions."
                                } else {
                                    "No pending suggestions in this category."
                                }),
                        )
                    }),
            )
            .when(total > GROUP_PAGE_SIZE, |this| {
                this.child(
                    Pagination::new("groups-pages", self.page, pages, format!("{total} groups"))
                        .on_page({
                            let queue = cx.entity().downgrade();
                            move |page, window, cx| {
                                let _ = queue.update(cx, |queue, cx| {
                                    queue.page = page;
                                    queue.selected.clear();
                                    queue.reload(window, cx);
                                });
                            }
                        }),
                )
            })
    }
}

/// An expanded group's activities, a page at a time.
pub struct GroupMembers {
    queue: WeakEntity<ReviewQueue>,
    ids: Vec<ActivityId>,
    page: usize,
    data: Option<AiReviewPage>,
    labels: Option<u64>,
    pickers: HashMap<ActivityId, (Entity<CategoryCombobox>, Subscription)>,
    load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl GroupMembers {
    fn new(
        queue: WeakEntity<ReviewQueue>,
        ids: Vec<ActivityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscriptions =
            vec![
                cx.observe_in(&AppState::entity(cx), window, |this, _, window, cx| {
                    if this.labels != Some(AppState::read(cx).revisions.labels) {
                        this.reload(window, cx);
                    }
                }),
            ];
        let mut members = Self {
            queue,
            ids,
            page: 0,
            data: None,
            labels: None,
            pickers: HashMap::new(),
            load: Task::ready(()),
            _subscriptions: subscriptions,
        };
        members.reload(window, cx);
        members
    }

    fn set_ids(&mut self, ids: Vec<ActivityId>, window: &mut Window, cx: &mut Context<Self>) {
        if ids != self.ids {
            self.ids = ids;
            self.reload(window, cx);
        }
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.labels = Some(AppState::read(cx).revisions.labels);
        let page = self.page as i64;
        let filter = ReviewFilter {
            category: None,
            activities: Some(self.ids.clone()),
        };
        self.load = data::fetch_in(
            window,
            cx,
            move |core| {
                core.labeling
                    .get_review_page(page, MEMBER_PAGE_SIZE, &filter)
                    .map_err(UiError::from)
            },
            |this, result, window, cx| match result {
                Ok(data) => {
                    this.data = Some(data);
                    this.sync_pickers(window, cx);
                    // Deciding the last row of a page empties it, so settle on the last one left.
                    let last = page_count(this.total(), MEMBER_PAGE_SIZE) - 1;
                    if this.page > last {
                        this.page = last;
                        this.reload(window, cx);
                    }
                }
                Err(err) => tracing::warn!(error = %err, "failed to load a review group"),
            },
        );
    }

    fn total(&self) -> i64 {
        self.data.as_ref().map_or(0, |data| data.total)
    }

    fn sync_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self
            .data
            .as_ref()
            .map(|data| data.rows.clone())
            .unwrap_or_default();
        let ids: HashSet<ActivityId> = rows.iter().map(|row| row.activity_id).collect();
        self.pickers.retain(|id, _| ids.contains(id));
        for row in rows {
            let value = Some(SharedString::from(row.category_id.0.to_string()));
            match self.pickers.get(&row.activity_id) {
                Some((picker, _)) => {
                    picker.update(cx, |picker, cx| picker.set_value(value, window, cx))
                }
                None => {
                    let activity = row.activity_id;
                    let picker = category_picker(
                        row.category_id,
                        self.queue.clone(),
                        move |_| vec![activity],
                        window,
                        cx,
                    );
                    self.pickers.insert(activity, picker);
                }
            }
        }
    }
}

impl Render for GroupMembers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let border = theme.border;
        let busy = self
            .queue
            .upgrade()
            .is_some_and(|queue| queue.read(cx).busy);
        let rows = self
            .data
            .as_ref()
            .map(|data| data.rows.clone())
            .unwrap_or_default();
        let total = self.total();
        let pages = page_count(total, MEMBER_PAGE_SIZE);

        v_flex()
            .gap_2()
            .py_2()
            .pl_8()
            .text_sm()
            .children(rows.into_iter().enumerate().map(|(ix, row)| {
                let full = match &row.url {
                    Some(url) => format!("{}\n{url}", row.title),
                    None => row.title.clone(),
                };
                let picker = self
                    .pickers
                    .get(&row.activity_id)
                    .map(|(picker, _)| picker.clone());
                h_flex()
                    .py_1()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .id(("member-title", ix))
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .gap_2()
                            .overflow_hidden()
                            .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .max_w_full()
                                    .truncate()
                                    .child(row.title.clone()),
                            )
                            .when_some(row.url.clone(), |this, url| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(mono.clone())
                                        .text_xs()
                                        .text_color(muted)
                                        .child(url),
                                )
                            }),
                    )
                    .child(
                        cell(64.)
                            .text_right()
                            .text_color(muted)
                            .child(format_seconds(row.total_seconds)),
                    )
                    .child(cell(CATEGORY_WIDTH).children(picker))
                    .child(cell(ACTIONS_WIDTH).child(decision_buttons(
                        format!("member-{}", row.activity_id.0),
                        vec![row.activity_id],
                        "label".into(),
                        busy,
                        self.queue.clone(),
                    )))
            }))
            .when(total > MEMBER_PAGE_SIZE, |this| {
                let members = cx.entity().downgrade();
                this.child(
                    Pagination::new(
                        "member-pages",
                        self.page,
                        pages,
                        format!("{total} activities"),
                    )
                    .on_page(move |page, window, cx| {
                        let _ = members.update(cx, |members, cx| {
                            members.page = page;
                            members.reload(window, cx);
                        });
                    }),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(kind: KeyKind) -> AiReviewGroup {
        AiReviewGroup {
            key_kind: kind,
            key: "github.com".into(),
            app_name: "Firefox".into(),
            category_id: CategoryId(7),
            category_name: "Work".into(),
            category_color: "#fff".into(),
            activity_count: 2,
            total_seconds: 10.,
            activity_ids: vec![ActivityId(1), ActivityId(2)],
        }
    }

    #[test]
    fn a_group_is_named_by_its_site_or_app() {
        assert_eq!(source_name(&group(KeyKind::Domain)), "github.com");
        assert_eq!(source_name(&group(KeyKind::Exe)), "Firefox");
        assert_eq!(group_id(&group(KeyKind::Domain)), "domain:github.com:7");
    }

    #[test]
    fn pages_round_up_and_never_reach_zero() {
        assert_eq!(page_count(0, 20), 1);
        assert_eq!(page_count(20, 20), 1);
        assert_eq!(page_count(21, 20), 2);
    }

    #[test]
    fn toasts_count_what_was_decided() {
        assert_eq!(accepted_message(1), "Accepted the AI suggestion");
        assert_eq!(accepted_message(3), "Accepted 3 AI suggestions");
        assert_eq!(
            moved_message(1, None),
            "Rejected the AI suggestion for 1 activity"
        );
        assert_eq!(
            moved_message(4, Some("Work / Code")),
            "Set 4 activities to Work / Code"
        );
    }
}
