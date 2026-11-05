//! The history and statistics as the front end asks for them: filters and times as text, read
//! here before they reach the [`HistorySvc`].

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::domain::types::{ActivityStats, CategoryFilter, CategorySourceFilter, SessionFilter, SessionsPage};
use crate::domain::HistorySvc;
use crate::shared::ids::CategoryId;
use crate::shared::time::TimeRange;

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

#[derive(Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SessionListQuery {
    pub page: i64,
    pub page_size: i64,
    #[ts(optional)]
    pub search: Option<String>,
    /// A category id, or `"uncategorized"`.
    #[ts(optional)]
    pub category: Option<String>,
    #[ts(optional, type = "\"manual\" | \"rule\" | \"ai\"")]
    pub source: Option<String>,
    #[ts(optional)]
    pub start: Option<String>,
    #[ts(optional)]
    pub end: Option<String>,
}

pub fn list_sessions(history: &dyn HistorySvc, query: SessionListQuery) -> Result<SessionsPage, QueryError> {
    let filter = SessionFilter {
        search: query.search,
        category: parse_category_filter(query.category.as_deref())?,
        source: parse_category_source_filter(query.source.as_deref())?,
        start: query.start.as_deref().map(parse_rfc3339).transpose()?,
        end: query.end.as_deref().map(parse_rfc3339).transpose()?,
    };
    Ok(history.get_sessions(query.page, query.page_size, &filter)?)
}

pub fn activity_stats(history: &dyn HistorySvc, start: &str, end: &str) -> Result<ActivityStats, QueryError> {
    let range = TimeRange {
        start: parse_rfc3339(start)?,
        end: parse_rfc3339(end)?,
    };
    Ok(history.get_activity_stats(&range)?)
}

fn parse_category_source_filter(source: Option<&str>) -> Result<CategorySourceFilter, QueryError> {
    match source {
        None => Ok(CategorySourceFilter::All),
        Some("manual") => Ok(CategorySourceFilter::Manual),
        Some("rule") => Ok(CategorySourceFilter::Rule),
        Some("ai") => Ok(CategorySourceFilter::Ai),
        Some(value) => Err(QueryError::Invalid(format!("invalid category source filter: {value}"))),
    }
}

fn parse_category_filter(category: Option<&str>) -> Result<CategoryFilter, QueryError> {
    match category {
        None => Ok(CategoryFilter::All),
        Some("uncategorized") => Ok(CategoryFilter::Uncategorized),
        Some(id) => id
            .parse::<i64>()
            .map(|id| CategoryFilter::Category(CategoryId::from(id)))
            .map_err(|_| QueryError::Invalid(format!("invalid category filter: {id}"))),
    }
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, QueryError> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| QueryError::Invalid(format!("invalid timestamp: {value}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_filter_reads_every_category_source() {
        assert!(matches!(parse_category_source_filter(None), Ok(CategorySourceFilter::All)));
        assert!(matches!(parse_category_source_filter(Some("manual")), Ok(CategorySourceFilter::Manual)));
        assert!(matches!(parse_category_source_filter(Some("rule")), Ok(CategorySourceFilter::Rule)));
        assert!(matches!(parse_category_source_filter(Some("ai")), Ok(CategorySourceFilter::Ai)));
        assert!(parse_category_source_filter(Some("other")).is_err());
    }
}
