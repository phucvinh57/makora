//! The record of every AI run that had something to label: tokens, timings and how it ended, kept
//! so the settings page can show what categorization costs and how fast each model is.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::categorization::domain::usage::{AiRunRecord, RunStatus, RunTrigger};
use crate::categorization::ports::RunUsage;
use crate::db::ai_runs::AiRunsRepo;
use crate::db::models::{AiModelTotalsRow, AiRunRow, NewAiRunRow};
use crate::db::Db;
use crate::shared::time;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AiRunView {
    pub id: i64,
    pub trigger: RunTrigger,
    /// `None` for the local model.
    pub provider: Option<String>,
    pub model: String,
    pub status: RunStatus,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub items: u64,
    pub labeled: u64,
    pub usage: RunUsage,
}

/// Sums over a set of runs. Rates are left to the reader: output speed is `output_tokens` over
/// `decode_ms` for the local model and over `busy_ms` for a remote one.
#[derive(Debug, Clone, Default, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct UsageTotals {
    pub runs: u64,
    pub items: u64,
    pub labeled: u64,
    pub requests: u64,
    pub failed_requests: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub llm_ms: u64,
    pub busy_ms: u64,
    pub prefill_ms: u64,
    pub decode_ms: u64,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ModelUsage {
    /// `None` for the local model.
    pub provider: Option<String>,
    pub model: String,
    pub totals: UsageTotals,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct AiUsageSummary {
    pub totals: UsageTotals,
    /// The most recently used first.
    pub models: Vec<ModelUsage>,
}

pub trait AiUsageSvc: Send + Sync {
    fn record(&self, run: &AiRunRecord) -> Result<()>;
    /// Up to `limit` runs, the most recent first.
    fn get_recent_runs(&self, limit: usize) -> Result<Vec<AiRunView>>;
    /// The runs started at or after `since`, every run when `None`.
    fn get_summary(&self, since: Option<DateTime<Utc>>) -> Result<AiUsageSummary>;
}

pub struct SqliteAiUsage {
    db: Db,
}

impl SqliteAiUsage {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl AiUsageSvc for SqliteAiUsage {
    fn record(&self, run: &AiRunRecord) -> Result<()> {
        let (started_at, finished_at) = (time::to_db(run.started_at), time::to_db(run.finished_at));
        let usage = &run.usage;
        let row = NewAiRunRow {
            triggered_by: run.trigger.as_str(),
            provider: run.provider.as_deref(),
            model: &run.model,
            status: run.status.as_str(),
            error: run.error.as_deref(),
            started_at: &started_at,
            finished_at: &finished_at,
            items: signed(run.items),
            labeled: signed(run.labeled),
            requests: signed(usage.requests),
            failed_requests: signed(usage.failed_requests),
            input_tokens: signed(usage.input_tokens),
            cached_input_tokens: signed(usage.cached_input_tokens),
            output_tokens: signed(usage.output_tokens),
            llm_ms: signed(usage.llm_ms),
            busy_ms: signed(usage.busy_ms),
            prefill_ms: signed(usage.prefill_ms),
            decode_ms: signed(usage.decode_ms),
            embed_count: signed(usage.embed_count),
            embed_ms: signed(usage.embed_ms),
        };
        AiRunsRepo::insert(&mut *self.db.write(), &row)?;
        Ok(())
    }

    fn get_recent_runs(&self, limit: usize) -> Result<Vec<AiRunView>> {
        let mut conn = self.db.read()?;
        let rows = AiRunsRepo::get_recent(&mut *conn, i64::try_from(limit).unwrap_or(i64::MAX))?;
        Ok(rows.into_iter().map(run_view).collect())
    }

    fn get_summary(&self, since: Option<DateTime<Utc>>) -> Result<AiUsageSummary> {
        let since = since.map(time::to_db);
        let mut conn = self.db.read()?;
        let rows = AiRunsRepo::get_totals_by_model(&mut *conn, since.as_deref())?;
        let models: Vec<ModelUsage> = rows
            .into_iter()
            .map(|row| ModelUsage {
                provider: row.provider.clone(),
                model: row.model.clone(),
                totals: model_totals(&row),
            })
            .collect();
        let totals = models.iter().fold(UsageTotals::default(), |sum, model| add(sum, &model.totals));
        Ok(AiUsageSummary { totals, models })
    }
}

fn run_view(row: AiRunRow) -> AiRunView {
    AiRunView {
        id: row.id,
        trigger: row.triggered_by,
        provider: row.provider,
        model: row.model,
        status: row.status,
        error: row.error,
        started_at: row.started_at,
        finished_at: row.finished_at,
        items: unsigned(row.items),
        labeled: unsigned(row.labeled),
        usage: RunUsage {
            requests: unsigned(row.requests),
            failed_requests: unsigned(row.failed_requests),
            input_tokens: unsigned(row.input_tokens),
            cached_input_tokens: unsigned(row.cached_input_tokens),
            output_tokens: unsigned(row.output_tokens),
            llm_ms: unsigned(row.llm_ms),
            busy_ms: unsigned(row.busy_ms),
            prefill_ms: unsigned(row.prefill_ms),
            decode_ms: unsigned(row.decode_ms),
            embed_count: unsigned(row.embed_count),
            embed_ms: unsigned(row.embed_ms),
        },
    }
}

fn model_totals(row: &AiModelTotalsRow) -> UsageTotals {
    UsageTotals {
        runs: unsigned(row.runs),
        items: unsigned(row.items),
        labeled: unsigned(row.labeled),
        requests: unsigned(row.requests),
        failed_requests: unsigned(row.failed_requests),
        input_tokens: unsigned(row.input_tokens),
        cached_input_tokens: unsigned(row.cached_input_tokens),
        output_tokens: unsigned(row.output_tokens),
        llm_ms: unsigned(row.llm_ms),
        busy_ms: unsigned(row.busy_ms),
        prefill_ms: unsigned(row.prefill_ms),
        decode_ms: unsigned(row.decode_ms),
    }
}

fn add(sum: UsageTotals, more: &UsageTotals) -> UsageTotals {
    UsageTotals {
        runs: sum.runs + more.runs,
        items: sum.items + more.items,
        labeled: sum.labeled + more.labeled,
        requests: sum.requests + more.requests,
        failed_requests: sum.failed_requests + more.failed_requests,
        input_tokens: sum.input_tokens + more.input_tokens,
        cached_input_tokens: sum.cached_input_tokens + more.cached_input_tokens,
        output_tokens: sum.output_tokens + more.output_tokens,
        llm_ms: sum.llm_ms + more.llm_ms,
        busy_ms: sum.busy_ms + more.busy_ms,
        prefill_ms: sum.prefill_ms + more.prefill_ms,
        decode_ms: sum.decode_ms + more.decode_ms,
    }
}

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}
