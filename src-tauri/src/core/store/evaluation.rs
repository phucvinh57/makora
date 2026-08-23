use std::collections::BTreeMap;

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Outcome {
    Agree,
    Corrected,
    Rejected,
    NoPrediction,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agree => "agree",
            Self::Corrected => "corrected",
            Self::Rejected => "rejected",
            Self::NoPrediction => "no_prediction",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingVerdict {
    pub surface: &'static str,
    pub bulk: bool,
    pub outcome: Outcome,
    pub key_kind: Option<String>,
    pub key: Option<String>,
    pub url_key: Option<String>,
    pub window_title: Option<String>,
    pub session_id: Option<i64>,
    pub activity_id: Option<i64>,
    pub predicted_category_id: Option<i64>,
    pub predicted_category_name: Option<String>,
    pub chosen_category_id: Option<i64>,
    pub chosen_category_name: Option<String>,
    pub confidence: Option<f32>,
    pub model_version: Option<i64>,
    pub seconds: f64,
}

pub(crate) fn category_name(conn: &Connection, id: Option<i64>) -> Result<Option<String>> {
    id.map_or(Ok(None), |id| {
        conn.query_row(
            "SELECT name FROM categories WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    })
}

pub(crate) fn record_verdict(conn: &Connection, verdict: &PendingVerdict) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_verdicts (
             recorded_at, surface, bulk, outcome, key_kind, key, url_key, window_title,
             session_id, activity_id, predicted_category_id, predicted_category_name,
             chosen_category_id, chosen_category_name, confidence, model_version, seconds
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            Utc::now().to_rfc3339(),
            verdict.surface,
            verdict.bulk as i64,
            verdict.outcome.as_str(),
            verdict.key_kind,
            verdict.key,
            verdict.url_key,
            verdict.window_title,
            verdict.session_id,
            verdict.activity_id,
            verdict.predicted_category_id,
            verdict.predicted_category_name,
            verdict.chosen_category_id,
            verdict.chosen_category_name,
            verdict.confidence,
            verdict.model_version,
            verdict.seconds.max(0.0),
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VerdictTotals {
    pub total_count: i64,
    pub total_seconds: f64,
    pub agree_count: i64,
    pub agree_seconds: f64,
    pub corrected_count: i64,
    pub corrected_seconds: f64,
    pub rejected_count: i64,
    pub rejected_seconds: f64,
    pub no_prediction_count: i64,
    pub no_prediction_seconds: f64,
}

impl VerdictTotals {
    fn add(&mut self, outcome: &str, seconds: f64) {
        let seconds = seconds.max(0.0);
        self.total_count += 1;
        self.total_seconds += seconds;
        match outcome {
            "agree" => {
                self.agree_count += 1;
                self.agree_seconds += seconds;
            }
            "corrected" => {
                self.corrected_count += 1;
                self.corrected_seconds += seconds;
            }
            "rejected" => {
                self.rejected_count += 1;
                self.rejected_seconds += seconds;
            }
            "no_prediction" => {
                self.no_prediction_count += 1;
                self.no_prediction_seconds += seconds;
            }
            _ => {}
        }
    }

    fn judged_count(&self) -> i64 {
        self.agree_count + self.corrected_count + self.rejected_count
    }

    fn judged_seconds(&self) -> f64 {
        self.agree_seconds + self.corrected_seconds + self.rejected_seconds
    }

    fn precision(&self) -> Option<f32> {
        let judged = self.judged_count();
        (judged > 0).then(|| self.agree_count as f32 / judged as f32)
    }

    fn time_weighted_precision(&self) -> Option<f32> {
        let judged = self.judged_seconds();
        (judged > 0.0).then(|| (self.agree_seconds / judged) as f32)
    }

    fn coverage(&self) -> Option<f32> {
        let total = self.judged_count() + self.no_prediction_count;
        (total > 0).then(|| self.judged_count() as f32 / total as f32)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfidenceBucket {
    pub lower: f32,
    pub upper: f32,
    pub count: i64,
    pub seconds: f64,
    pub precision: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceStat {
    pub surface: String,
    pub totals: VerdictTotals,
    pub precision: Option<f32>,
    pub time_weighted_precision: Option<f32>,
    pub coverage: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyPrecision {
    pub day: String,
    pub reviewed_count: i64,
    pub agree_count: i64,
    pub precision: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfusionPair {
    pub predicted_category_name: String,
    pub chosen_category_name: String,
    pub count: i64,
    pub seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunEvaluation {
    pub recorded_at: String,
    pub model_version: i64,
    pub update_kind: String,
    pub confidence_threshold: f32,
    pub holdout_accuracy: Option<f32>,
    pub holdout_sample_count: i64,
    pub manual_holdout_accuracy: Option<f32>,
    pub manual_holdout_sample_count: i64,
    pub predicted_session_count: i64,
    pub labeled_key_count: i64,
    pub category_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiAccuracyReport {
    pub totals: VerdictTotals,
    pub precision: Option<f32>,
    pub time_weighted_precision: Option<f32>,
    pub coverage: Option<f32>,
    pub by_confidence: Vec<ConfidenceBucket>,
    pub by_surface: Vec<SurfaceStat>,
    pub daily: Vec<DailyPrecision>,
    pub confusions: Vec<ConfusionPair>,
    pub runs: Vec<RunEvaluation>,
    pub excluded_bulk_count: i64,
    pub current_confidence_threshold: f32,
}

#[derive(Debug, Default)]
struct BucketAccumulator {
    totals: VerdictTotals,
    count: i64,
}

#[derive(Debug, Default)]
struct DailyAccumulator {
    reviewed_count: i64,
    agree_count: i64,
}

pub fn accuracy_report(conn: &Connection, start: &str, end: &str) -> Result<AiAccuracyReport> {
    accuracy_report_impl(conn, start, end, false)
}

pub fn accuracy_report_with_bulk(
    conn: &Connection,
    start: &str,
    end: &str,
    include_bulk: bool,
) -> Result<AiAccuracyReport> {
    accuracy_report_impl(conn, start, end, include_bulk)
}

fn accuracy_report_impl(
    conn: &Connection,
    start: &str,
    end: &str,
    include_bulk: bool,
) -> Result<AiAccuracyReport> {
    let excluded_bulk_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_verdicts
         WHERE recorded_at >= ?1 AND recorded_at < ?2 AND bulk = 1",
        params![start, end],
        |row| row.get(0),
    )?;

    let mut totals = VerdictTotals::default();
    let mut by_surface: BTreeMap<String, VerdictTotals> = BTreeMap::new();
    let mut daily: BTreeMap<String, DailyAccumulator> = BTreeMap::new();
    let mut confusions: BTreeMap<(String, String), (i64, f64)> = BTreeMap::new();
    let bounds = [0.50_f32, 0.60, 0.70, 0.80, 0.90, 0.95, 1.00];
    let mut buckets = bounds
        .windows(2)
        .map(|_| BucketAccumulator {
            totals: VerdictTotals::default(),
            count: 0,
        })
        .collect::<Vec<_>>();

    let mut stmt = conn.prepare(
        "SELECT date(recorded_at, 'localtime'), surface, outcome, bulk, predicted_category_name,
                chosen_category_name, confidence, seconds
         FROM ai_verdicts
         WHERE recorded_at >= ?1 AND recorded_at < ?2
           AND (?3 = 1 OR bulk = 0)
         ORDER BY recorded_at, id",
    )?;
    let rows = stmt.query_map(params![start, end, include_bulk as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<f32>>(6)?,
            row.get::<_, f64>(7)?,
        ))
    })?;

    for row in rows {
        let (recorded_at, surface, outcome, predicted_name, chosen_name, confidence, seconds) =
            row?;
        totals.add(&outcome, seconds);
        by_surface
            .entry(surface)
            .or_default()
            .add(&outcome, seconds);

        let daily_entry = daily.entry(recorded_at).or_default();
        if outcome == "agree" {
            daily_entry.agree_count += 1;
        }
        if matches!(outcome.as_str(), "agree" | "corrected" | "rejected") {
            daily_entry.reviewed_count += 1;
        }

        if let Some(confidence) = confidence {
            if let Some(index) = bounds.windows(2).position(|window| {
                confidence >= window[0]
                    && (confidence < window[1] || (window[1] == 1.0 && confidence <= window[1]))
            }) {
                buckets[index].count += 1;
                buckets[index].totals.add(&outcome, seconds);
            }
        }

        if outcome == "corrected" {
            if let (Some(predicted), Some(chosen)) = (predicted_name, chosen_name) {
                let entry = confusions.entry((predicted, chosen)).or_default();
                entry.0 += 1;
                entry.1 += seconds.max(0.0);
            }
        }
    }

    let mut run_stmt = conn.prepare(
        "SELECT recorded_at, model_version, update_kind, confidence_threshold,
                holdout_accuracy, holdout_sample_count, manual_holdout_accuracy,
                manual_holdout_sample_count, predicted_session_count,
                labeled_key_count, category_count
         FROM ai_run_evaluations
         WHERE recorded_at >= ?1 AND recorded_at < ?2
         ORDER BY recorded_at, id",
    )?;
    let runs = run_stmt
        .query_map(params![start, end], |row| {
            Ok(RunEvaluation {
                recorded_at: row.get(0)?,
                model_version: row.get(1)?,
                update_kind: row.get(2)?,
                confidence_threshold: row.get(3)?,
                holdout_accuracy: row.get(4)?,
                holdout_sample_count: row.get(5)?,
                manual_holdout_accuracy: row.get(6)?,
                manual_holdout_sample_count: row.get(7)?,
                predicted_session_count: row.get(8)?,
                labeled_key_count: row.get(9)?,
                category_count: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let current_confidence_threshold = conn
        .query_row(
            "SELECT value FROM settings
             WHERE key = 'ai_categorization_confidence_threshold'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.75);

    let by_confidence = buckets
        .into_iter()
        .enumerate()
        .map(|(index, bucket)| ConfidenceBucket {
            lower: bounds[index],
            upper: bounds[index + 1],
            count: bucket.count,
            seconds: bucket.totals.total_seconds,
            precision: bucket.totals.precision(),
        })
        .collect();
    let by_surface = by_surface
        .into_iter()
        .map(|(surface, totals)| SurfaceStat {
            surface,
            precision: totals.precision(),
            time_weighted_precision: totals.time_weighted_precision(),
            coverage: totals.coverage(),
            totals,
        })
        .collect();
    let daily = daily
        .into_iter()
        .map(|(day, values)| DailyPrecision {
            day,
            reviewed_count: values.reviewed_count,
            agree_count: values.agree_count,
            precision: (values.reviewed_count > 0)
                .then(|| values.agree_count as f32 / values.reviewed_count as f32),
        })
        .collect();
    let mut confusions: Vec<ConfusionPair> = confusions
        .into_iter()
        .map(
            |((predicted_category_name, chosen_category_name), (count, seconds))| ConfusionPair {
                predicted_category_name,
                chosen_category_name,
                count,
                seconds,
            },
        )
        .collect();
    confusions.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| right.seconds.total_cmp(&left.seconds))
            .then_with(|| {
                left.predicted_category_name
                    .cmp(&right.predicted_category_name)
            })
    });

    Ok(AiAccuracyReport {
        precision: totals.precision(),
        time_weighted_precision: totals.time_weighted_precision(),
        coverage: totals.coverage(),
        totals,
        by_confidence,
        by_surface,
        daily,
        confusions,
        runs,
        excluded_bulk_count,
        current_confidence_threshold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    #[test]
    fn report_calculates_precision_coverage_and_bulk_exclusion() {
        let path = std::env::temp_dir().join(format!(
            "makora-evaluation-{}-{}.sqlite",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let conn = open_connection(&path).unwrap();
        let conn = conn.lock().unwrap();
        conn.execute_batch(
            "INSERT INTO ai_verdicts
                (recorded_at, surface, bulk, outcome, predicted_category_name,
                 chosen_category_name, confidence, seconds)
             VALUES
                ('2025-01-01T01:00:00Z', 'session', 0, 'agree', 'Work', 'Work', 0.76, 10),
                ('2025-01-01T02:00:00Z', 'session', 0, 'corrected', 'Work', 'Study', 0.76, 30),
                ('2025-01-01T03:00:00Z', 'suggestion', 0, 'no_prediction', NULL, 'Work', NULL, 5),
                ('2025-01-01T04:00:00Z', 'suggestion', 1, 'agree', 'Study', 'Study', 0.96, 40);",
        )
        .unwrap();

        let report =
            accuracy_report(&conn, "2025-01-01T00:00:00Z", "2025-01-02T00:00:00Z").unwrap();
        assert_eq!(report.totals.total_count, 3);
        assert_eq!(report.totals.agree_count, 1);
        assert_eq!(report.totals.corrected_count, 1);
        assert_eq!(report.totals.no_prediction_count, 1);
        assert_eq!(report.excluded_bulk_count, 1);
        assert_eq!(report.precision, Some(0.5));
        assert_eq!(report.coverage, Some(2.0 / 3.0));

        let report =
            accuracy_report_with_bulk(&conn, "2025-01-01T00:00:00Z", "2025-01-02T00:00:00Z", true)
                .unwrap();
        assert_eq!(report.totals.total_count, 4);
        assert_eq!(report.totals.agree_count, 2);
        assert_eq!(report.precision, Some(2.0 / 3.0));

        drop(conn);
        let _ = std::fs::remove_file(path);
    }
}
