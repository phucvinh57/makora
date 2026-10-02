//! The shapes the rest of the app (Tauri commands, the frontend, the AI code) sees. These are
//! what [`crate::domain`]'s services take and return; how they map onto rows is
//! [`crate::db`]'s job.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::confidence::Confidence;
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

/// An activity the LLM is asked to label, as it needs to see it.
#[derive(Debug, Clone)]
pub struct UncategorizedActivity {
    pub id: ActivityId,
    pub key_kind: KeyKind,
    pub key: String,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
}

/// An activity the user has judged, either by pinning it or by rejecting an AI label. These
/// are the examples the LLM learns from.
#[derive(Debug, Clone)]
pub struct LabeledActivity {
    pub key_kind: KeyKind,
    pub key: String,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
    pub category_id: Option<CategoryId>,
    pub ai_category_id: Option<CategoryId>,
    pub ai_rejected_category_id: Option<CategoryId>,
}

/// What the model made of one activity: a category, or `None` when it found none it stands by.
#[derive(Debug, Clone, Copy)]
pub struct AiVerdict {
    pub activity_id: ActivityId,
    pub category_id: Option<CategoryId>,
    pub confidence: Confidence,
}

/// Where a session's resolved category came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CategorySource {
    Manual,
    Rule,
    Ai,
}

impl CategorySource {
    /// Reads the `manual`/`rule`/`ai` marker the history query derives.
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "rule" => Some(Self::Rule),
            "ai" => Some(Self::Ai),
            _ => None,
        }
    }
}

/// An AI label waiting for the user to accept, change or reject it.
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

/// AI labels of one source (an executable or a domain) that suggest the same category, so one
/// decision can cover all of them.
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
    /// The least sure the model was about any member; `None` when unknown.
    pub min_confidence: Option<Confidence>,
    pub activity_ids: Vec<ActivityId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewGroupPage {
    pub groups: Vec<AiReviewGroup>,
    pub total: i64,
}

/// Which pending AI labels the review queue shows.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReviewFilter {
    /// Only labels in this category or under it.
    pub category: Option<CategoryId>,
    /// Only these activities, e.g. the members of one group.
    pub activities: Option<Vec<ActivityId>>,
    /// Hide labels the model was confident about.
    pub uncertain_only: bool,
    /// Hide activities (or groups) tracked for less than this, in seconds.
    pub min_seconds: f64,
}

/// How much the user has taught the model so far.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FeedbackStats {
    pub corrected: i64,
    pub confirmed: i64,
    pub pinned: i64,
    pub rejected: i64,
    pub pending_review: i64,
    /// Tracked time of the activities still waiting for review.
    pub pending_seconds: f64,
    /// Tracked time of the AI labels the user has already judged.
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
    /// Sessions that started before this.
    pub end: Option<DateTime<Utc>>,
}
