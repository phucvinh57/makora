use diesel::prelude::*;

use super::schema::{activities, ai_labels, ai_runs, categories, category_rules, sessions, sources};
use super::text_enums::PatternFieldColumn;
use crate::categorization::domain::rules::{PatternField, RuleKind};
use crate::categorization::domain::usage::{RunStatus, RunTrigger};
use crate::tracking::domain::source_key::KeyKind;

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = categories)]
pub struct CategoryRow {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent_id: Option<i64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = categories)]
pub struct NewCategoryRow<'a> {
    pub name: &'a str,
    pub color: &'a str,
    pub parent_id: Option<i64>,
    pub description: Option<&'a str>,
}

/// A full replace of every editable column, including clearing `parent_id`/`description` back
/// to `NULL` — so unlike a typical changeset, `None` here means "set it to null", not "leave it".
#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = categories, treat_none_as_null = true)]
pub struct CategoryChanges<'a> {
    pub name: &'a str,
    pub color: &'a str,
    pub parent_id: Option<i64>,
    pub description: Option<&'a str>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = sources)]
pub struct SourceRow {
    pub id: i64,
    pub kind: KeyKind,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = sources)]
pub struct NewSourceRow<'a> {
    pub kind: &'a str,
    pub key: &'a str,
    pub name: &'a str,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = activities)]
pub struct ActivityRow {
    pub id: i64,
    pub source_id: i64,
    pub path: String,
    pub title: String,
    pub category_id: Option<i64>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = activities)]
pub struct NewActivityRow<'a> {
    pub source_id: i64,
    pub path: &'a str,
    pub title: &'a str,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = sessions)]
pub struct SessionRow {
    pub id: i64,
    pub activity_id: i64,
    pub start_time: String,
    pub end_time: Option<String>,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSessionRow<'a> {
    pub activity_id: i64,
    pub start_time: &'a str,
    pub last_seen: &'a str,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = ai_labels, primary_key(activity_id))]
pub struct AiLabelRow {
    pub activity_id: i64,
    pub category_id: i64,
    pub rejected: bool,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = category_rules)]
pub struct CategoryRuleRow {
    pub id: i64,
    pub category_id: i64,
    pub kind: RuleKind,
    #[diesel(deserialize_as = PatternFieldColumn)]
    pub field: Option<PatternField>,
    pub value: String,
}

#[derive(Debug, Clone, Insertable, AsChangeset)]
#[diesel(table_name = category_rules)]
pub struct CategoryRuleValues<'a> {
    pub category_id: i64,
    pub kind: &'a str,
    pub field: &'a str,
    pub value: &'a str,
}

#[derive(Debug, Clone, Queryable)]
pub struct ActivityIdentityRow {
    pub id: i64,
    pub source_kind: KeyKind,
    pub source_key: String,
    pub source_name: String,
    pub path: String,
    pub title: String,
}

/// An activity with a category from the user (a pin or a rule) and its vector, to be compared
/// against the one being labeled.
#[derive(Debug, Clone, QueryableByName)]
pub struct CategorizedEmbeddingRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub activity_id: i64,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub source_kind: KeyKind,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub source_key: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub source_name: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub path: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub title: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub category_id: i64,
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub vector: Vec<u8>,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = ai_runs)]
pub struct AiRunRow {
    pub id: i64,
    pub triggered_by: RunTrigger,
    pub provider: Option<String>,
    pub model: String,
    pub status: RunStatus,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub items: i64,
    pub labeled: i64,
    pub requests: i64,
    pub failed_requests: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub llm_ms: i64,
    pub busy_ms: i64,
    pub prefill_ms: i64,
    pub decode_ms: i64,
    pub embed_count: i64,
    pub embed_ms: i64,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ai_runs)]
pub struct NewAiRunRow<'a> {
    pub triggered_by: &'a str,
    pub provider: Option<&'a str>,
    pub model: &'a str,
    pub status: &'a str,
    pub error: Option<&'a str>,
    pub started_at: &'a str,
    pub finished_at: &'a str,
    pub items: i64,
    pub labeled: i64,
    pub requests: i64,
    pub failed_requests: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub llm_ms: i64,
    pub busy_ms: i64,
    pub prefill_ms: i64,
    pub decode_ms: i64,
    pub embed_count: i64,
    pub embed_ms: i64,
}

#[derive(Debug, Clone, QueryableByName)]
pub struct AiModelTotalsRow {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub provider: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub model: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub runs: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub items: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub labeled: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub requests: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub failed_requests: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub input_tokens: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub cached_input_tokens: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub output_tokens: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub llm_ms: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub busy_ms: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub prefill_ms: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub decode_ms: i64,
}
