use gpui_kit::{App, ElementId, IntoElement, SharedString, Window};

use crate::ui::widgets::segmented::segmented;

/// How far back the network log is read; it keeps about a month.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogPeriod {
    Day,
    #[default]
    Week,
    Month,
}

impl LogPeriod {
    const ALL: [LogPeriod; 3] = [LogPeriod::Day, LogPeriod::Week, LogPeriod::Month];

    fn label(self) -> &'static str {
        match self {
            Self::Day => "24h",
            Self::Week => "7d",
            Self::Month => "30d",
        }
    }

    pub fn days(self) -> u32 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::Month => 30,
        }
    }

    pub fn phrase(self) -> &'static str {
        match self {
            Self::Day => "in the last 24 hours",
            Self::Week => "in the last 7 days",
            Self::Month => "in the last 30 days",
        }
    }

    pub fn picker(
        self,
        id: impl Into<ElementId>,
        on_pick: impl Fn(&LogPeriod, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        segmented(
            id,
            Self::ALL
                .iter()
                .map(|period| SharedString::from(period.label()))
                .collect(),
            Self::ALL.iter().position(|p| *p == self).unwrap_or(1),
            move |ix, window, cx| on_pick(&Self::ALL[*ix], window, cx),
        )
    }
}
