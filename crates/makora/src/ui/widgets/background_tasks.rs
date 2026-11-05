//! The sidebar's "N tasks running" line, with each task's progress in its tooltip.

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::ui::app_state::BackgroundTask;

#[derive(IntoElement)]
pub struct BackgroundTasksIndicator {
    tasks: Vec<BackgroundTask>,
    collapsed: bool,
}

impl BackgroundTasksIndicator {
    pub fn new(tasks: Vec<BackgroundTask>) -> Self {
        Self {
            tasks,
            collapsed: false,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

pub fn summary(count: usize) -> String {
    format!("{count} task{} running", if count == 1 { "" } else { "s" })
}

impl RenderOnce for BackgroundTasksIndicator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.tasks.is_empty() {
            return div().into_any_element();
        }
        let tasks = self.tasks.clone();
        h_flex()
            .id("background-tasks")
            .w_full()
            .gap_2()
            .px(px(10.))
            .py(px(6.))
            .rounded(cx.theme().radius)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .when(self.collapsed, |this| this.justify_center().px_0())
            .child(Spinner::new().small().color(cx.theme().muted_foreground))
            .when(!self.collapsed, |this| {
                this.child(summary(self.tasks.len()))
            })
            .tooltip(move |window, cx| {
                let tasks = tasks.clone();
                Tooltip::element(move |_, cx| {
                    v_flex().gap_1p5().children(tasks.iter().map(|task| {
                        v_flex()
                            .child(
                                div()
                                    .font_medium()
                                    .child(SharedString::from(task.label.clone())),
                            )
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(task.progress.clone())),
                            )
                    }))
                })
                .build(window, cx)
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_counts_tasks() {
        assert_eq!(summary(1), "1 task running");
        assert_eq!(summary(2), "2 tasks running");
    }
}
