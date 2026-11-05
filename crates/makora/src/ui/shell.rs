//! The window's frame: the collapsible sidebar and the screen beside it.

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, ClickEvent, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, Size, StatefulInteractiveElement,
    Styled, Subscription, Window, div, px, size,
};

use crate::runtime::window::TITLE;
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

const SIDEBAR_WIDTH: f32 = 208.;
const SIDEBAR_COLLAPSED_WIDTH: f32 = 56.;
const CONTENT_PADDING: f32 = 24.;

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

    /// Shows a screen as the user left it.
    pub fn select(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        let reentered = self.reentering(section);
        self.show(section, reentered, window, cx);
    }

    /// Shows a screen at a tab or with filters.
    pub fn navigate(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        let section = route.section();
        let reentered = self.reentering(section);
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
        self.show(section, reentered, window, cx);
    }

    /// Whether `section` was built on an earlier visit and is about to come back into view.
    fn reentering(&self, section: Section) -> bool {
        section != self.section && self.is_open(section)
    }

    fn show(
        &mut self,
        section: Section,
        reentered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.section;
        self.screen(section, window, cx);
        self.section = section;
        if section != previous {
            self.set_visible(previous, false, window, cx);
            if reentered {
                self.set_visible(section, true, window, cx);
            }
        }
        cx.notify();
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

    fn is_open(&self, section: Section) -> bool {
        match section {
            Section::Statistics => self.statistics.is_some(),
            Section::Categories => self.categories.is_some(),
            Section::History => self.history.is_some(),
            Section::Insights => self.insights.is_some(),
            Section::Settings => self.settings.is_some(),
        }
    }

    fn set_visible(
        &self,
        section: Section,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match section {
            Section::Statistics => set_visible(&self.statistics, visible, window, cx),
            Section::Categories => set_visible(&self.categories, visible, window, cx),
            Section::History => set_visible(&self.history, visible, window, cx),
            Section::Insights => set_visible(&self.insights, visible, window, cx),
            Section::Settings => set_visible(&self.settings, visible, window, cx),
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

fn set_visible<S: Screen + 'static>(
    slot: &Option<Entity<S>>,
    visible: bool,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    if let Some(screen) = slot {
        screen.update(cx, |screen, cx| {
            if visible {
                screen.shown(window, cx);
            } else {
                screen.hidden(window, cx);
            }
        });
    }
}

/// The room a screen has inside the shell's padding, for screens that fill the window rather
/// than scroll in it. The height leaves a few pixels for borders.
pub fn content_size(window: &Window, cx: &App) -> Size<Pixels> {
    let viewport = window.viewport_size();
    let sidebar = if Prefs::get(cx).sidebar_collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        SIDEBAR_WIDTH
    };
    size(
        viewport.width - px(sidebar + 2. * CONTENT_PADDING),
        viewport.height - px(2. * CONTENT_PADDING + 4.),
    )
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let screen = self.screen(self.section, window, cx);
        let theme = cx.theme();
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(theme.font_size)
            .child(
                TitleBar::new().child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(theme.muted_foreground)
                        .child(TITLE),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
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
                            .p(px(CONTENT_PADDING))
                            .overflow_y_scrollbar()
                            .child(screen),
                    ),
            )
    }
}
