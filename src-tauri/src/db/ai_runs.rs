use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{Nullable, Text};
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use super::models::{AiModelTotalsRow, AiRunRow, NewAiRunRow};
use super::schema::ai_runs;

pub use super::traits::AiRunsRepo;

impl AiRunsRepo for SqliteConnection {
    fn insert(&mut self, new: &NewAiRunRow<'_>) -> QueryResult<()> {
        diesel::insert_into(ai_runs::table).values(new).execute(self)?;
        Ok(())
    }

    fn get_recent(&mut self, limit: i64) -> QueryResult<Vec<AiRunRow>> {
        ai_runs::table
            .order((ai_runs::started_at.desc(), ai_runs::id.desc()))
            .limit(limit)
            .select(AiRunRow::as_select())
            .load(self)
    }

    fn get_totals_by_model(&mut self, since: Option<&str>) -> QueryResult<Vec<AiModelTotalsRow>> {
        sql_query(
            "SELECT provider, model, COUNT(*) AS runs, SUM(items) AS items, SUM(labeled) AS labeled,
                    SUM(requests) AS requests, SUM(failed_requests) AS failed_requests,
                    SUM(input_tokens) AS input_tokens, SUM(cached_input_tokens) AS cached_input_tokens,
                    SUM(output_tokens) AS output_tokens, SUM(llm_ms) AS llm_ms, SUM(busy_ms) AS busy_ms,
                    SUM(prefill_ms) AS prefill_ms, SUM(decode_ms) AS decode_ms
             FROM ai_runs
             WHERE ? IS NULL OR started_at >= ?
             GROUP BY provider, model
             ORDER BY MAX(started_at) DESC",
        )
        .bind::<Nullable<Text>, _>(since)
        .bind::<Nullable<Text>, _>(since)
        .load(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::categorization::domain::usage::{RunStatus, RunTrigger};
    use crate::db::open_in_memory;

    use super::*;

    fn run<'a>(provider: Option<&'a str>, model: &'a str, started_at: &'a str, output_tokens: i64) -> NewAiRunRow<'a> {
        NewAiRunRow {
            triggered_by: "scheduled",
            provider,
            model,
            status: "completed",
            error: None,
            started_at,
            finished_at: started_at,
            items: 10,
            labeled: 8,
            requests: 10,
            failed_requests: 1,
            input_tokens: 1000,
            cached_input_tokens: 600,
            output_tokens,
            llm_ms: 2000,
            busy_ms: 5000,
            prefill_ms: 0,
            decode_ms: 0,
            embed_count: 3,
            embed_ms: 40,
        }
    }

    #[test]
    fn a_run_round_trips() {
        let mut conn = open_in_memory();
        let mut failed = run(None, "qwen", "2026-03-01T10:00:00+00:00", 30);
        failed.triggered_by = "manual";
        failed.status = "failed";
        failed.error = Some("the model is missing");
        conn.insert(&failed).unwrap();
        conn.insert(&run(Some("openai"), "gpt", "2026-03-02T10:00:00+00:00", 20)).unwrap();

        let rows = conn.get_recent(10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].provider.as_deref(), Some("openai"));
        let local = &rows[1];
        assert_eq!(local.triggered_by, RunTrigger::Manual);
        assert_eq!(local.status, RunStatus::Failed);
        assert_eq!(local.error.as_deref(), Some("the model is missing"));
        assert_eq!(local.provider, None);
        assert_eq!((local.output_tokens, local.embed_count, local.embed_ms), (30, 3, 40));
        assert_eq!(conn.get_recent(1).unwrap().len(), 1);
    }

    #[test]
    fn totals_are_summed_per_model_and_period() {
        let mut conn = open_in_memory();
        conn.insert(&run(None, "qwen", "2026-03-01T10:00:00+00:00", 30)).unwrap();
        conn.insert(&run(None, "qwen", "2026-03-03T10:00:00+00:00", 30)).unwrap();
        conn.insert(&run(Some("openai"), "gpt", "2026-03-02T10:00:00+00:00", 20)).unwrap();

        let all = conn.get_totals_by_model(None).unwrap();
        let summary: Vec<(Option<&str>, &str, i64, i64, i64)> = all
            .iter()
            .map(|row| (row.provider.as_deref(), row.model.as_str(), row.runs, row.items, row.output_tokens))
            .collect();
        assert_eq!(summary, [(None, "qwen", 2, 20, 60), (Some("openai"), "gpt", 1, 10, 20)]);

        let recent = conn.get_totals_by_model(Some("2026-03-02T00:00:00+00:00")).unwrap();
        let summary: Vec<(&str, i64)> = recent.iter().map(|row| (row.model.as_str(), row.runs)).collect();
        assert_eq!(summary, [("qwen", 1), ("gpt", 1)]);
    }
}
