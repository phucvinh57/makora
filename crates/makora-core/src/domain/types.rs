use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::categorization::domain::retrieval::SourceRef;
use crate::shared::ids::{ActivityId, CategoryId, SessionId};
use crate::tracking::domain::source_key::KeyKind;

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub color: String,
    pub parent_id: Option<CategoryId>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewCategory {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
    pub parent_id: Option<CategoryId>,
}

/// An activity's identity and text, as a model or the privacy preview sees it.
#[derive(Debug, Clone)]
pub struct ActivitySample {
    pub id: ActivityId,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
    pub source: SourceRef,
}

/// What the model made of one activity: a category, or `None` when it found none it stands by.
#[derive(Debug, Clone, Copy)]
pub struct AiVerdict {
    pub activity_id: ActivityId,
    pub category_id: Option<CategoryId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CategorySource {
    Manual,
    Rule,
    Ai,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown category source {0:?}")]
pub struct UnknownCategorySource(pub String);

impl TryFrom<&str> for CategorySource {
    type Error = UnknownCategorySource;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "manual" => Ok(Self::Manual),
            "rule" => Ok(Self::Rule),
            "ai" => Ok(Self::Ai),
            _ => Err(UnknownCategorySource(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewRow {
    pub activity_id: ActivityId,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
    pub category_id: CategoryId,
    pub category_name: String,
    pub category_color: String,
    pub total_seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewPage {
    pub rows: Vec<AiReviewRow>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewGroup {
    pub key_kind: KeyKind,
    pub key: String,
    pub app_name: String,
    pub category_id: CategoryId,
    pub category_name: String,
    pub category_color: String,
    pub activity_count: i64,
    pub total_seconds: f64,
    pub activity_ids: Vec<ActivityId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewGroupPage {
    pub groups: Vec<AiReviewGroup>,
    pub total: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReviewFilter {
    pub category: Option<CategoryId>,
    pub activities: Option<Vec<ActivityId>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FeedbackStats {
    pub corrected: i64,
    pub confirmed: i64,
    pub pinned: i64,
    pub rejected: i64,
    pub pending_review: i64,
    pub pending_seconds: f64,
    pub reviewed_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionsPage {
    pub rows: Vec<SessionRow>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: SessionId,
    pub activity_id: ActivityId,
    pub app_name: String,
    pub source_kind: KeyKind,
    pub source_key: String,
    pub title: String,
    pub url: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub category_id: Option<CategoryId>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub category_is_pinned: bool,
    pub category_source: Option<CategorySource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityStats {
    pub total_seconds: f64,
    pub session_count: i64,
    pub categories: Vec<CategoryStat>,
    pub apps: Vec<AppStat>,
    pub daily: Vec<DailyStat>,
    /// Seconds per weekday (Monday first) and local hour.
    pub hour_table: [[f64; 24]; 7],
    pub days: Vec<DayProfile>,
    pub habits: Habits,
    pub observations: Vec<Observation>,
    /// Empty for ranges longer than a week.
    pub timeline: Vec<TimelineDay>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryStat {
    pub category_id: Option<CategoryId>,
    pub name: Option<String>,
    pub color: Option<String>,
    pub total_seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStat {
    pub label: String,
    pub kind: KeyKind,
    pub category_color: Option<String>,
    pub total_seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyStat {
    pub day: String,
    pub total_seconds: f64,
}

/// One working day (04:00 to 04:00) with any tracked time. Times of day are wall-clock minutes
/// from the day's midnight, so 01:30 the next morning is 1530.
#[derive(Debug, Clone, Serialize)]
pub struct DayProfile {
    pub day: String,
    /// 0 is Monday.
    pub weekday: u8,
    pub first_active: u32,
    pub last_active: u32,
    pub active_seconds: f64,
    /// The longest run of sessions with gaps under five minutes, gaps included.
    pub longest_focus_seconds: f64,
    pub sessions: u32,
    #[serde(skip)]
    pub hours: [f64; 24],
}

/// Local hours `start..end`, e.g. 9..12 for 09:00 to 12:00.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HourRange {
    pub start: u8,
    pub end: u8,
}

/// "9–12, 14–18".
pub fn format_hour_ranges(ranges: &[HourRange]) -> String {
    ranges
        .iter()
        .map(|r| format!("{}–{}", r.start, r.end))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Habits and observations need this many active days to mean anything.
pub const MIN_DAYS_FOR_HABITS: usize = 3;

/// Minutes from midnight as "9:05"; past midnight wraps, so 1530 is "1:30".
pub fn format_clock(minute: u32) -> String {
    format!("{}:{:02}", minute / 60 % 24, minute % 60)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Habits {
    pub active_days: usize,
    /// Hours with at least 15 minutes on a median active day.
    pub active_hours: Vec<HourRange>,
    pub typical_start: Option<u32>,
    pub typical_end: Option<u32>,
    /// Lower and upper quartile of the first active minute.
    pub start_quartiles: Option<(u32, u32)>,
    /// The three busiest hours, as ranges.
    pub peak_hours: Vec<HourRange>,
    /// Share of time between 22:00 and 04:00.
    pub late_share: f64,
    /// Share of time on working days that fall on a Saturday or Sunday.
    pub weekend_share: f64,
    pub longest_focus: Option<LongestFocus>,
    pub median_focus_seconds: f64,
    /// Sessions per active hour.
    pub fragmentation: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongestFocus {
    pub day: String,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Observation {
    pub text: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineDay {
    pub day: String,
    pub blocks: Vec<TimelineBlock>,
}

/// Consecutive time in one category; seconds are wall-clock seconds from local midnight.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimelineBlock {
    pub start: f64,
    pub end: f64,
    pub category_id: Option<CategoryId>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
}

/// A page of a list as `LIMIT`/`OFFSET`. A negative page reads as the first and a page size
/// below one as one, so any request names a real page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paging {
    pub limit: i64,
    pub offset: i64,
}

impl Paging {
    pub fn new(page: i64, page_size: i64) -> Self {
        let limit = page_size.max(1);
        Self {
            limit,
            offset: page.max(0).saturating_mul(limit),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum CategoryFilter {
    #[default]
    All,
    Uncategorized,
    Category(CategoryId),
}

#[derive(Debug, Clone, Default)]
pub enum CategorySourceFilter {
    #[default]
    All,
    Manual,
    Rule,
    Ai,
}

#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub search: Option<String>,
    pub category: CategoryFilter,
    pub source: CategorySourceFilter,
    /// Sessions that ended after this (a still-open one always qualifies).
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paging_turns_a_page_into_limit_and_offset() {
        assert_eq!(Paging::new(2, 25), Paging { limit: 25, offset: 50 });
    }

    #[test]
    fn paging_clamps_what_names_no_page() {
        assert_eq!(Paging::new(-3, 0), Paging { limit: 1, offset: 0 });
        assert_eq!(Paging::new(i64::MAX, 50).offset, i64::MAX);
    }
}
