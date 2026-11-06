//! Where the window can be sent: a screen, and for some screens a tab or filters. The web UI kept
//! these in the URL.

use serde::{Deserialize, Serialize};

use crate::ui::format::time_range::TimeRangeKind;

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Statistics,
    Categories(CategoriesTab),
    History(HistoryFilters),
    Monitoring(MonitoringTab),
    Settings,
}

impl Route {
    pub fn section(&self) -> Section {
        match self {
            Self::Statistics => Section::Statistics,
            Self::Categories(_) => Section::Categories,
            Self::History(_) => Section::History,
            Self::Monitoring(_) => Section::Monitoring,
            Self::Settings => Section::Settings,
        }
    }
}

/// A sidebar entry: a screen without a tab or filters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Section {
    #[default]
    Statistics,
    Categories,
    History,
    Monitoring,
    Settings,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Statistics,
        Section::Categories,
        Section::History,
        Section::Monitoring,
        Section::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Statistics => "Statistics",
            Self::Categories => "Categories",
            Self::History => "History",
            Self::Monitoring => "Monitoring",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoriesTab {
    #[default]
    Taxonomy,
    Rules,
    Review,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitoringTab {
    #[default]
    Network,
    Masked,
    Usage,
}

/// The category sources the history list can be narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistorySource {
    Manual,
    Rule,
}

/// The history screen's filters, the web UI's `kind`/`from`/`to`/`q`/`category`/`source`/`page`
/// URL parameters. `from` and `to` are RFC 3339 and only used by a custom range; `category` is a
/// category id or `"uncategorized"`; `page` is one-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryFilters {
    pub kind: TimeRangeKind,
    pub from: Option<String>,
    pub to: Option<String>,
    pub q: String,
    pub category: Option<String>,
    pub source: Option<HistorySource>,
    pub page: u32,
}

impl Default for HistoryFilters {
    fn default() -> Self {
        Self {
            kind: TimeRangeKind::All,
            from: None,
            to: None,
            q: String::new(),
            category: None,
            source: None,
            page: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_belongs_to_its_section() {
        assert_eq!(
            Route::Categories(CategoriesTab::Review).section(),
            Section::Categories
        );
        assert_eq!(
            Route::History(HistoryFilters::default()).section(),
            Section::History
        );
        assert_eq!(
            Route::Monitoring(MonitoringTab::Masked).section(),
            Section::Monitoring
        );
    }

    #[test]
    fn history_starts_on_the_first_page_of_all_time() {
        let filters = HistoryFilters::default();
        assert_eq!((filters.kind, filters.page), (TimeRangeKind::All, 1));
        assert!(filters.category.is_none() && filters.source.is_none() && filters.q.is_empty());
    }
}
