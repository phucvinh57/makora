use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone)]
pub struct UncategorizedActivity {
    pub id: ActivityId,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
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

impl CategorySource {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "rule" => Some(Self::Rule),
            "ai" => Some(Self::Ai),
            _ => None,
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
