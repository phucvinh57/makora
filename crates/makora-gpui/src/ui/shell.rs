//! The window's frame: the collapsible sidebar and the screen beside it.

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AnyView, AppContext, ClickEvent, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
    div, px,
};

use crate::ui::app_state::AppState;
use crate::ui::prefs::Prefs;
use crate::ui::route::{Route, Section};
use crate::ui::screens::Screen;
use crate::ui::screens::categories::CategoriesScreen;
use crate::ui::screens::history::HistoryScreen;
use crate::ui::screens::insights::InsightsScreen;
use crate::ui::screens::settings::SettingsScreen;
use crate::ui::screens::statistics::StatisticsScreen;
use crate::ui::theme;
use crate::ui::widgets::background_tasks::BackgroundTasksIndicator;
use crate::ui::widgets::toast::Toast;

pub const SIDEBAR_WIDTH: f32 = 208.;
pub const SIDEBAR_COLLAPSED_WIDTH: f32 = 56.;

pub struct Shell {
    section: Section,
    collapsed: bool,
    statistics: Option<Entity<StatisticsScreen>>,
    categories: Option<Entity<CategoriesScreen>>,
    history: Option<Entity<HistoryScreen>>,
    insights: Option<Entity<InsightsScreen>>,
    settings: Option<Entity<SettingsScreen>>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(route: Option<Route>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe(&AppState::entity(cx), |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| theme::follow_system(window, cx)),
        ];
        let mut shell = Self {
            section: Section::Statistics,
            collapsed: Prefs::get(cx).sidebar_collapsed,
            statistics: None,
            categories: None,
            history: None,
            insights: None,
            settings: None,
            _subscriptions: subscriptions,
        };
        match route {
            Some(route) => shell.navigate(route, window, cx),
            None => shell.select(Section::Statistics, window, cx),
        }
        theme::follow_system(window, cx);
        cx.defer_in(window, |_, window, cx| {
            if let Some(issue) = AppState::update(cx, |state, _| state.take_settings_issue()) {
                Toast::error(issue).persistent().show(window, cx);
            }
        });
        shell
    }

    pub fn section(&self) -> Section {
        self.section
    }

    /// Shows a screen as the user left it.
    pub fn select(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.screen(section, window, cx);
        self.section = section;
        cx.notify();
    }

    /// Shows a screen at a tab or with filters.
    pub fn navigate(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        let section = route.section();
        match route {
            Route::Statistics | Route::Settings => {}
            Route::Categories(tab) => {
                let screen = open(&mut self.categories, window, cx);
                screen.update(cx, |screen, cx| screen.navigate(tab, window, cx));
            }
            Route::History(filters) => {
                let screen = open(&mut self.history, window, cx);
                screen.update(cx, |screen, cx| screen.navigate(filters, window, cx));
            }
            Route::Insights(tab) => {
                let screen = open(&mut self.insights, window, cx);
                screen.update(cx, |screen, cx| screen.navigate(tab, window, cx));
            }
        }
        self.select(section, window, cx);
    }

    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.collapsed = collapsed;
        Prefs::update(cx, |prefs| prefs.sidebar_collapsed = collapsed);
        cx.notify();
    }

    fn screen(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) -> AnyView {
        match section {
            Section::Statistics => open(&mut self.statistics, window, cx).into(),
            Section::Categories => open(&mut self.categories, window, cx).into(),
            Section::History => open(&mut self.history, window, cx).into(),
            Section::Insights => open(&mut self.insights, window, cx).into(),
            Section::Settings => open(&mut self.settings, window, cx).into(),
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.collapsed;
        let tasks = AppState::read(cx).background_tasks();
        let toggle_label = if collapsed {
            "Expand sidebar"
        } else {
            "Collapse sidebar"
        };
        let nav: Vec<_> = Section::ALL
            .into_iter()
            .map(|section| self.render_nav_item(section, cx))
            .collect();
        let theme = cx.theme();

        v_flex()
            .flex_shrink_0()
            .h_full()
            .w(px(if collapsed {
                SIDEBAR_COLLAPSED_WIDTH
            } else {
                SIDEBAR_WIDTH
            }))
            .border_r_1()
            .border_color(theme.sidebar_border)
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .child(
                h_flex()
                    .py_4()
                    .map(|this| {
                        if collapsed {
                            this.justify_center().px_2()
                        } else {
                            this.justify_between().px_4()
                        }
                    })
                    .when(!collapsed, |this| {
                        this.child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .justify_center()
                                        .size(px(28.))
                                        .rounded(theme.radius)
                                        .bg(theme.primary)
                                        .text_color(theme.primary_foreground)
                                        .child(Icon::new(IconName::Hourglass).small()),
                                )
                                .child(div().text_base().font_semibold().child("Makora")),
                        )
                    })
                    .child(
                        div()
                            .id("sidebar-toggle")
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .size(px(28.))
                            .rounded(theme.radius)
                            .text_color(theme.muted_foreground)
                            .hover(|style| {
                                style
                                    .bg(theme.sidebar_accent.opacity(0.5))
                                    .text_color(theme.sidebar_accent_foreground)
                            })
                            .cursor_pointer()
                            .tooltip(move |window, cx| Tooltip::new(toggle_label).build(window, cx))
                            .on_click(cx.listener(move |shell, _: &ClickEvent, _, cx| {
                                shell.set_collapsed(!collapsed, cx)
                            }))
                            .child(
                                Icon::new(if collapsed {
                                    IconName::PanelLeftOpen
                                } else {
                                    IconName::PanelLeftClose
                                })
                                .small(),
                            ),
                    ),
            )
            .child(v_flex().flex_1().gap_1().px_2().children(nav))
            .child(
                div()
                    .px_2()
                    .pb_2()
                    .child(BackgroundTasksIndicator::new(tasks).collapsed(collapsed)),
            )
    }

    fn render_nav_item(&self, section: Section, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let active = self.section == section;
        let label = SharedString::from(section.label());
        let icon = match section {
            Section::Statistics => IconName::ChartColumn,
            Section::Categories => IconName::Tags,
            Section::History => IconName::RotateCcwClock,
            Section::Insights => IconName::Gauge,
            Section::Settings => IconName::Settings,
        };

        h_flex()
            .id(SharedString::from(format!("nav-{}", section.label())))
            .gap_2()
            .px(px(10.))
            .py(px(6.))
            .rounded(theme.radius)
            .text_sm()
            .font_medium()
            .cursor_pointer()
            .map(|this| {
                if active {
                    this.bg(theme.sidebar_primary.opacity(0.1))
                        .text_color(theme.sidebar_primary)
                } else {
                    let (hover_bg, hover_fg) = (
                        theme.sidebar_accent.opacity(0.5),
                        theme.sidebar_accent_foreground,
                    );
                    this.text_color(theme.muted_foreground)
                        .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
                }
            })
            .when(self.collapsed, |this| {
                let label = label.clone();
                this.justify_center()
                    .px_0()
                    .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
            })
            .on_click(cx.listener(move |shell, _: &ClickEvent, window, cx| {
                shell.select(section, window, cx)
            }))
            .child(Icon::new(icon).small().flex_shrink_0())
            .when(!self.collapsed, |this| this.child(label))
            .into_any_element()
    }
}

fn open<S: Screen + 'static>(
    slot: &mut Option<Entity<S>>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Entity<S> {
    slot.get_or_insert_with(|| cx.new(|cx| S::new(window, cx)))
        .clone()
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let screen = self.screen(self.section, window, cx);
        let theme = cx.theme();
        h_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(theme.font_size)
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "content-{}",
                        self.section.label()
                    )))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_6()
                    .overflow_y_scrollbar()
                    .child(screen),
            )
    }
}
