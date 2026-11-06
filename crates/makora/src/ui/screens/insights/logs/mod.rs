mod list;
pub mod model;
mod network;

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use chrono::{Local, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IndexPath, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, IntoElement, ListAlignment, ListOffset, ListState,
    ParentElement, Render, SharedString, Styled, Subscription, Task, Window, div, px,
};
use makora_lib::logs::LogArea;
use makora_lib::logs::reader::{HostTraffic, LogPage, LogQuery};

use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::widgets::feedback::ErrorBanner;
use crate::ui::widgets::segmented::segmented;

use model::{
    AREAS, GroupBy, KeyedEntry, ListItem, MinLevel, Period, area_label, group_rows, list_items,
    with_keys,
};

/// The log grows while its page is open, so the page polls it.
const POLL: Duration = Duration::from_secs(5);
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

pub fn area_icon(area: LogArea) -> IconName {
    match area {
        LogArea::Network => IconName::Globe,
        LogArea::Ai => IconName::Sparkles,
        LogArea::Browser => IconName::AppWindow,
        LogArea::Tracking => IconName::MousePointerClick,
        LogArea::Settings => IconName::Settings2,
        LogArea::Storage => IconName::Database,
        LogArea::App => IconName::Box,
    }
}

pub struct LogsTab {
    period: Period,
    min_level: MinLevel,
    areas: Vec<LogArea>,
    group_by: GroupBy,
    search: String,
    search_input: Entity<InputState>,
    level_select: Entity<SelectState<Vec<SharedString>>>,
    log_dir: SharedString,

    entries: Vec<KeyedEntry>,
    groups: Vec<model::Group>,
    items: Vec<ListItem>,
    /// What each list item shows, to keep the scroll position across a refresh.
    item_keys: Vec<String>,
    area_counts: BTreeMap<LogArea, u32>,
    truncated: bool,
    loaded: bool,
    /// The entries shown are for filters that have since changed.
    stale: bool,
    logs_error: Option<String>,
    logs_loading: bool,
    hosts: Option<Vec<HostTraffic>>,
    network_error: Option<String>,
    network_loading: bool,
    expanded: HashSet<String>,
    list: ListState,

    polling: bool,
    poll: Task<()>,
    logs_load: Task<()>,
    network_load: Task<()>,
    search_debounce: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl LogsTab {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search messages and fields…"));
        let labels: Vec<SharedString> = MinLevel::ALL
            .iter()
            .map(|level| SharedString::from(level.label()))
            .collect();
        let level_select =
            cx.new(|cx| SelectState::new(labels, Some(IndexPath::new(0)), window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&search_input, window, |tab, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().to_string();
                    tab.debounce_search(text, cx);
                }
            }),
            cx.subscribe(
                &level_select,
                |tab, _, event: &SelectEvent<Vec<SharedString>>, cx| {
                    let SelectEvent::Confirm(Some(label)) = event else {
                        return;
                    };
                    if let Some(level) = MinLevel::ALL.iter().find(|l| l.label() == label.as_ref())
                    {
                        tab.min_level = *level;
                        tab.refilter(cx);
                    }
                },
            ),
        ];
        Self {
            period: Period::default(),
            min_level: MinLevel::default(),
            areas: Vec::new(),
            group_by: GroupBy::default(),
            search: String::new(),
            search_input,
            level_select,
            log_dir: crate::ui::app_state::Core::get(cx)
                .logs
                .dir()
                .display()
                .to_string()
                .into(),
            entries: Vec::new(),
            groups: Vec::new(),
            items: Vec::new(),
            item_keys: Vec::new(),
            area_counts: BTreeMap::new(),
            truncated: false,
            loaded: false,
            stale: false,
            logs_error: None,
            logs_loading: false,
            hosts: None,
            network_error: None,
            network_loading: false,
            expanded: HashSet::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            polling: false,
            poll: Task::ready(()),
            logs_load: Task::ready(()),
            network_load: Task::ready(()),
            search_debounce: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    /// Reloads now and every few seconds after while `on`, which is while the tab is on screen.
    pub fn set_polling(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.polling {
            return;
        }
        self.polling = on;
        if !on {
            self.poll = Task::ready(());
            return;
        }
        self.reload(cx);
        self.poll = cx.spawn(async move |tab, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if tab.update(cx, |tab, cx| tab.reload(cx)).is_err() {
                    break;
                }
            }
        });
    }

    fn query(&self) -> LogQuery {
        LogQuery {
            days: self.period.days(),
            min_level: self.min_level.level(),
            areas: (!self.areas.is_empty()).then(|| self.areas.clone()),
            search: (!self.search.is_empty()).then(|| self.search.clone()),
            limit: None,
        }
    }

    fn filtered(&self) -> bool {
        !self.search.is_empty() || !self.areas.is_empty() || self.min_level != MinLevel::Info
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.reload_logs(cx);
        self.reload_network(cx);
    }

    fn reload_logs(&mut self, cx: &mut Context<Self>) {
        let query = self.query();
        self.logs_loading = true;
        self.logs_load = data::fetch(
            cx,
            move |core| core.logs.query(&query, Utc::now()).map_err(UiError::from),
            |tab, result, _| {
                tab.logs_loading = false;
                match result {
                    Ok(page) => {
                        tab.logs_error = None;
                        tab.set_page(page);
                    }
                    Err(err) => tab.logs_error = Some(err.message),
                }
            },
        );
    }

    fn reload_network(&mut self, cx: &mut Context<Self>) {
        let days = self.period.days();
        self.network_loading = true;
        self.network_load = data::fetch(
            cx,
            move |core| {
                core.logs
                    .network_summary(days, Utc::now())
                    .map_err(UiError::from)
            },
            |tab, result, _| {
                tab.network_loading = false;
                match result {
                    Ok(hosts) => {
                        tab.network_error = None;
                        tab.hosts = Some(hosts);
                    }
                    Err(err) => tab.network_error = Some(err.message),
                }
            },
        );
    }

    /// The filters changed: the shown entries dim until the new ones land.
    fn refilter(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        self.reload_logs(cx);
        cx.notify();
    }

    fn set_period(&mut self, period: Period, cx: &mut Context<Self>) {
        if self.period != period {
            self.period = period;
            self.reload_network(cx);
            self.refilter(cx);
        }
    }

    fn set_areas(&mut self, areas: Vec<LogArea>, cx: &mut Context<Self>) {
        self.areas = areas;
        self.refilter(cx);
    }

    fn toggle_area(&mut self, area: LogArea, cx: &mut Context<Self>) {
        let mut areas = self.areas.clone();
        model::toggle_area(&mut areas, area);
        self.set_areas(areas, cx);
    }

    fn set_group_by(&mut self, group_by: GroupBy, cx: &mut Context<Self>) {
        if self.group_by != group_by {
            self.group_by = group_by;
            self.rebuild(false);
            cx.notify();
        }
    }

    fn debounce_search(&mut self, text: String, cx: &mut Context<Self>) {
        self.search_debounce = cx.spawn(async move |tab, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = tab.update(cx, |tab, cx| tab.commit_search(&text, cx));
        });
    }

    fn commit_search(&mut self, text: &str, cx: &mut Context<Self>) {
        let search = text.trim();
        if search != self.search {
            self.search = search.to_owned();
            self.refilter(cx);
        }
    }

    /// Shows the requests to `host`: searches for it in the network area.
    fn show_host(&mut self, host: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search_debounce = Task::ready(());
        self.search_input
            .update(cx, |input, cx| input.set_value(host.to_owned(), window, cx));
        self.search = host.trim().to_owned();
        self.set_areas(vec![LogArea::Network], cx);
    }

    fn set_page(&mut self, page: LogPage) {
        let keep_scroll = !self.stale && self.loaded;
        self.entries = with_keys(page.entries);
        self.area_counts = page.area_counts;
        self.truncated = page.truncated;
        self.loaded = true;
        self.stale = false;
        self.rebuild(keep_scroll);
    }

    fn item_key(&self, item: ListItem) -> String {
        match item {
            ListItem::Header(group) => format!("group|{}", self.groups[group].label),
            ListItem::Row { row, .. } => self.entries[row].key.clone(),
        }
    }

    /// Regroups the entries; a refresh keeps the row at the top of the list in place.
    fn rebuild(&mut self, keep_scroll: bool) {
        let top = self.list.logical_scroll_top();
        self.groups = group_rows(&self.entries, self.group_by, Local::now().date_naive());
        self.items = list_items(&self.groups);
        let new_keys: Vec<String> = self.items.iter().map(|item| self.item_key(*item)).collect();
        let old_keys = std::mem::replace(&mut self.item_keys, new_keys);
        let new_keys = &self.item_keys;
        if keep_scroll && old_keys == *new_keys {
            return;
        }
        self.list.reset(self.items.len());
        let anchor = keep_scroll
            .then(|| old_keys.get(top.item_ix))
            .flatten()
            .and_then(|key| new_keys.iter().position(|other| other == key));
        if let Some(item_ix) = anchor {
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: top.offset_in_item,
            });
        }
    }

    fn toggle_row(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(key) {
            self.expanded.insert(key.to_owned());
        }
        if let Some(ix) = self.item_keys.iter().position(|item| item == key) {
            self.list.remeasure_items(ix..ix + 1);
        }
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let loading = self.logs_loading || self.network_loading;
        let log_dir = self.log_dir.clone();
        h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .mr_auto()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("What Makora has been doing on this device"),
            )
            .child(segmented(
                "logs-period",
                Period::ALL
                    .iter()
                    .map(|period| SharedString::from(period.label()))
                    .collect(),
                Period::ALL
                    .iter()
                    .position(|p| *p == self.period)
                    .unwrap_or(1),
                cx.listener(|tab: &mut Self, ix: &usize, _, cx| {
                    tab.set_period(Period::ALL[*ix], cx)
                }),
            ))
            .child(
                Button::new("open-log-folder")
                    .outline()
                    .small()
                    .icon(Icon::new(IconName::FolderOpen))
                    .label("Open log folder")
                    .tooltip(log_dir.clone())
                    .on_click(move |_, _, cx| {
                        cx.open_with_system(std::path::Path::new(log_dir.as_ref()))
                    }),
            )
            .child(
                Button::new("refresh-logs")
                    .outline()
                    .small()
                    .icon(Icon::new(IconName::RefreshCw))
                    .loading(loading)
                    .disabled(loading)
                    .tooltip("Refresh logs")
                    .on_click(cx.listener(|tab, _, _, cx| {
                        tab.reload(cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let total: u32 = AREAS
            .iter()
            .map(|area| self.area_counts.get(area).copied().unwrap_or(0))
            .sum();
        let mut chips: Vec<AnyElement> = vec![
            area_chip(
                "area-all",
                None,
                "All",
                total,
                self.areas.is_empty(),
                cx.listener(|tab: &mut Self, _, _, cx| tab.set_areas(Vec::new(), cx)),
                cx,
            )
            .into_any_element(),
        ];
        chips.extend(AREAS.iter().map(|area| {
            let area = *area;
            area_chip(
                SharedString::from(format!("area-{}", area_label(area))),
                Some(area_icon(area)),
                area_label(area),
                self.area_counts.get(&area).copied().unwrap_or(0),
                self.areas.contains(&area),
                cx.listener(move |tab: &mut Self, _, _, cx| tab.toggle_area(area, cx)),
                cx,
            )
            .into_any_element()
        }));

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().w(px(288.)).flex_none().child(
                            Input::new(&self.search_input)
                                .small()
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .xsmall()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .aria_label("Search logs"),
                        ),
                    )
                    .child(
                        Select::new(&self.level_select)
                            .small()
                            .w(px(176.))
                            .accessibility_label("Minimum level"),
                    )
                    .child(
                        h_flex().ml_auto().child(
                            TabBar::new("log-group-by")
                                .segmented()
                                .small()
                                .selected_index(match self.group_by {
                                    GroupBy::Day => 0,
                                    GroupBy::Area => 1,
                                })
                                .children([Tab::new().label("By day"), Tab::new().label("By area")])
                                .on_click(cx.listener(|tab, ix: &usize, _, cx| {
                                    tab.set_group_by(
                                        if *ix == 0 {
                                            GroupBy::Day
                                        } else {
                                            GroupBy::Area
                                        },
                                        cx,
                                    )
                                })),
                        ),
                    ),
            )
            .child(h_flex().flex_wrap().items_center().gap_1().children(chips))
            .into_any_element()
    }
}

fn area_chip(
    id: impl Into<gpui_kit::ElementId>,
    icon: Option<IconName>,
    label: &'static str,
    count: u32,
    active: bool,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
    cx: &gpui_kit::App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    Button::new(id)
        .xsmall()
        .map(|this| {
            if active {
                this.secondary()
            } else {
                this.ghost()
            }
        })
        .when(count == 0 && !active, |this| this.text_color(muted))
        .when_some(icon, |this, icon| this.icon(Icon::new(icon)))
        .label(label)
        .child(
            div()
                .text_color(muted)
                .child(crate::ui::format::number::to_locale_string(f64::from(
                    count,
                ))),
        )
        .on_click(on_click)
}

impl Render for LogsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let network = self.render_network(cx);
        let list = self.render_list(window, cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .gap_6()
            .child(self.render_header(cx))
            .child(network)
            .child(
                v_flex()
                    .flex_1()
                    .min_h(px(240.))
                    .gap_3()
                    .child(self.render_toolbar(cx))
                    .when_some(self.logs_error.clone(), |this, error| {
                        this.child(ErrorBanner::new(format!("Failed to load logs: {error}")))
                    })
                    .child(list),
            )
    }
}
