//! Local categorization based on semantic embeddings of stable keys and activity titles.
//!
//! Coherent keys still produce review-first rule suggestions. Multi-purpose keys are instead
//! classified at the activity/title level, with repeated stable URL paths graduating to the
//! same reviewable rule surface. All materialized labels are replaced transactionally.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use tracing::{error, info};

use super::category::{resolve_category, CompiledRule, KeyKind, SessionCategoryInput};
use super::cluster::{cluster, label_cluster, ClusterInput, ClusterParams};
use super::embed::{Embedder, Model2Vec};
use super::store::categories::{reset_categorization_data, CategorizationResetOutcome};
use super::store::sqlite::SharedConnection;

type CompletionCallback = Arc<dyn Fn() + Send + Sync>;
type Key = (String, String);

const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.75;
const MIN_LEAD: f32 = 0.15;
const SOFTMAX_TEMPERATURE: f32 = 0.05;
const MIN_DISTINCT_TITLES: usize = 8;
const MIN_DISPERSION_SECONDS: f64 = 30.0 * 60.0;
const MAX_TOP_SHARE: f64 = 0.60;
const MIN_CATEGORY_SHARE: f64 = 0.15;
const MIN_URL_SESSIONS: usize = 5;
const MIN_URL_DAYS: usize = 2;
const MIN_URL_CONSISTENCY: f64 = 0.80;
const SCHEDULE_SETTING_KEY: &str = "ai_categorization_schedule";
const LAST_COMPLETED_SETTING_KEY: &str = "ai_categorization_last_completed_at";
const LAST_EXECUTION_DURATION_SETTING_KEY: &str = "ai_categorization_last_execution_duration_ms";
const CONFIDENCE_THRESHOLD_SETTING_KEY: &str = "ai_categorization_confidence_threshold";
const INPUT_REVISION_SETTING_KEY: &str = "ai_categorization_input_revision";
const MODEL_VERSION_SETTING_KEY: &str = "ai_categorization_model_version";
const MODEL_ID_SETTING_KEY: &str = "ai_categorization_model_id";
const CATEGORY_SIGNATURE_SETTING_KEY: &str = "ai_categorization_category_signature";
const HOLDOUT_ACCURACY_SETTING_KEY: &str = "ai_categorization_holdout_accuracy";
const HOLDOUT_SAMPLE_COUNT_SETTING_KEY: &str = "ai_categorization_holdout_sample_count";
const CLUSTER_GRANULARITY_SETTING_KEY: &str = "ai_categorization_cluster_granularity";
const MAX_KEY_CLUSTER_INPUT: usize = 1500;
const MAX_TITLE_CLUSTER_INPUT: usize = 600;
const MAX_CLUSTERED_PARENTS: usize = 12;
const MAX_CLUSTERS_PER_SCOPE: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AiCategorizationSchedule {
    Manual,
    Hourly,
    Daily,
    Weekly,
}

impl AiCategorizationSchedule {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "manual" => Ok(Self::Manual),
            "hourly" => Ok(Self::Hourly),
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            _ => bail!("invalid AI categorization schedule: {value}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    fn duration(self, intervals: &ScheduleIntervals) -> Option<Duration> {
        match self {
            Self::Manual => None,
            Self::Hourly => Some(intervals.hourly),
            Self::Daily => Some(intervals.daily),
            Self::Weekly => Some(intervals.weekly),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClusteringGranularity {
    Broad,
    Balanced,
    Fine,
}

impl ClusteringGranularity {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "broad" => Ok(Self::Broad),
            "balanced" => Ok(Self::Balanced),
            "fine" => Ok(Self::Fine),
            _ => bail!("invalid AI clustering granularity: {value}"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Broad => "broad",
            Self::Balanced => "balanced",
            Self::Fine => "fine",
        }
    }

    fn params(self, max_inputs: usize) -> ClusterParams {
        let (sigma_multiplier, min_similarity, max_members) = match self {
            Self::Broad => (1.0, 0.20, 400),
            Self::Balanced => (1.5, 0.30, 200),
            Self::Fine => (2.0, 0.40, 100),
        };
        ClusterParams {
            sigma_multiplier,
            min_similarity,
            max_similarity: 1.0,
            min_members: 3,
            max_members,
            max_clusters: MAX_CLUSTERS_PER_SCOPE,
            max_inputs,
        }
    }
}

#[derive(Clone, Copy)]
struct ScheduleIntervals {
    hourly: Duration,
    daily: Duration,
    weekly: Duration,
}

struct ScheduleDeadline {
    at: Instant,
    scheduled_at: String,
}

impl Default for ScheduleIntervals {
    fn default() -> Self {
        Self {
            hourly: Duration::from_secs(60 * 60),
            daily: Duration::from_secs(24 * 60 * 60),
            weekly: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClassifierStatus {
    pub state: String,
    pub generation: u64,
    pub update_kind: Option<String>,
    pub training_example_count: usize,
    pub trained_category_count: usize,
    pub last_completed_at: Option<String>,
    pub last_execution_duration_ms: Option<u64>,
    pub last_error: Option<String>,
    pub schedule: String,
    pub next_scheduled_at: Option<String>,
    pub confidence_threshold: f32,
    pub holdout_accuracy: Option<f32>,
    pub holdout_sample_count: usize,
    pub cluster_granularity: String,
}

impl ClassifierStatus {
    #[allow(clippy::too_many_arguments)]
    fn initial(
        schedule: AiCategorizationSchedule,
        last_completed_at: Option<String>,
        last_execution_duration_ms: Option<u64>,
        confidence_threshold: f32,
        holdout_accuracy: Option<f32>,
        holdout_sample_count: usize,
        cluster_granularity: ClusteringGranularity,
        intervals: &ScheduleIntervals,
    ) -> Self {
        Self {
            state: "idle".into(),
            generation: 0,
            update_kind: None,
            training_example_count: 0,
            trained_category_count: 0,
            last_completed_at,
            last_execution_duration_ms,
            last_error: None,
            schedule: schedule.as_str().into(),
            next_scheduled_at: schedule_deadline(schedule, intervals)
                .map(|deadline| deadline.scheduled_at),
            confidence_threshold,
            holdout_accuracy,
            holdout_sample_count,
            cluster_granularity: cluster_granularity.as_str().into(),
        }
    }
}

enum ServiceMessage {
    Run(u64),
    SetSchedule(AiCategorizationSchedule),
    SetConfidenceThreshold(f32),
    SetClusteringGranularity(ClusteringGranularity),
    Reset(mpsc::Sender<std::result::Result<CategorizationResetOutcome, String>>),
}

#[derive(Clone)]
pub struct AiCategorizationService {
    tx: mpsc::Sender<ServiceMessage>,
    conn: SharedConnection,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<ClassifierStatus>>,
    intervals: ScheduleIntervals,
    commit_gate: Arc<Mutex<()>>,
    reset_pending: Arc<std::sync::atomic::AtomicBool>,
}

impl AiCategorizationService {
    pub(crate) fn new_with_completion(
        conn: SharedConnection,
        on_completed: CompletionCallback,
    ) -> Result<Self> {
        Self::new_with_intervals(conn, ScheduleIntervals::default(), on_completed)
    }

    fn new_with_intervals(
        conn: SharedConnection,
        intervals: ScheduleIntervals,
        on_completed: CompletionCallback,
    ) -> Result<Self> {
        let embedder: Arc<dyn Embedder> = Arc::new(Model2Vec::new()?);
        let schedule = load_schedule(&conn)?;
        let confidence_threshold = load_confidence_threshold(&conn)?;
        let cluster_granularity = load_cluster_granularity(&conn)?;
        let last_completed_at = read_setting(&conn, LAST_COMPLETED_SETTING_KEY)?;
        let last_execution_duration_ms = read_setting(&conn, LAST_EXECUTION_DURATION_SETTING_KEY)?
            .and_then(|value| value.parse::<u64>().ok());
        let holdout_accuracy = read_setting(&conn, HOLDOUT_ACCURACY_SETTING_KEY)?
            .and_then(|value| value.parse::<f32>().ok());
        let holdout_sample_count = read_setting(&conn, HOLDOUT_SAMPLE_COUNT_SETTING_KEY)?
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let commit_gate = Arc::new(Mutex::new(()));
        let reset_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let status = Arc::new(Mutex::new(ClassifierStatus::initial(
            schedule,
            last_completed_at,
            last_execution_duration_ms,
            confidence_threshold,
            holdout_accuracy,
            holdout_sample_count,
            cluster_granularity,
            &intervals,
        )));
        let service = Self {
            tx,
            conn: conn.clone(),
            generation: generation.clone(),
            status: status.clone(),
            intervals,
            commit_gate: commit_gate.clone(),
            reset_pending: reset_pending.clone(),
        };
        std::thread::Builder::new()
            .name("makora-classifier".into())
            .spawn(move || {
                run_service(
                    ServiceRuntime {
                        conn,
                        embedder,
                        schedule,
                        confidence_threshold,
                        cluster_granularity,
                        intervals,
                        generation,
                        status,
                        commit_gate,
                        on_completed,
                    },
                    rx,
                )
            })
            .context("spawning classifier worker")?;
        Ok(service)
    }

    pub fn run_now(&self) -> Result<()> {
        let target = {
            let _gate = self.commit_gate.lock().unwrap();
            if self.reset_pending.load(Ordering::Acquire) {
                bail!("AI categorization reset is in progress");
            }
            let target = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            {
                let mut status = self.status.lock().unwrap();
                status.state = "queued".into();
                status.generation = target;
                status.last_error = None;
                status.next_scheduled_at = schedule_from_status(&status)
                    .and_then(|schedule| schedule_deadline(schedule, &self.intervals))
                    .map(|deadline| deadline.scheduled_at);
            }
            target
        };
        self.tx
            .send(ServiceMessage::Run(target))
            .context("AI categorization worker is unavailable")
    }

    pub fn reset_categorization(&self) -> Result<CategorizationResetOutcome> {
        if self
            .reset_pending
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            bail!("AI categorization reset is already in progress");
        }

        let target = {
            let _gate = self.commit_gate.lock().unwrap();
            let target = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            let mut status = self.status.lock().unwrap();
            status.state = "queued".into();
            status.generation = target;
            status.last_error = None;
            status.next_scheduled_at = None;
            target
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        if let Err(error) = self.tx.send(ServiceMessage::Reset(reply_tx)) {
            self.reset_pending.store(false, Ordering::Release);
            bail!("AI categorization worker is unavailable: {error}");
        }
        let result = reply_rx.recv().map_err(|error| {
            anyhow::anyhow!("AI categorization reset worker did not respond: {error}")
        });
        self.reset_pending.store(false, Ordering::Release);
        result?.map_err(anyhow::Error::msg).inspect(|_| {
            let mut status = self.status.lock().unwrap();
            status.generation = target.max(status.generation);
        })
    }

    pub fn status(&self) -> ClassifierStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn set_schedule(&self, schedule: AiCategorizationSchedule) -> Result<()> {
        let _gate = self.commit_gate.lock().unwrap();
        if self.reset_pending.load(Ordering::Acquire) {
            bail!("AI categorization reset is in progress");
        }
        write_setting(&self.conn, SCHEDULE_SETTING_KEY, schedule.as_str())?;
        let next_scheduled_at =
            schedule_deadline(schedule, &self.intervals).map(|deadline| deadline.scheduled_at);
        {
            let mut status = self.status.lock().unwrap();
            status.schedule = schedule.as_str().into();
            status.next_scheduled_at = next_scheduled_at;
        }
        drop(_gate);
        self.tx
            .send(ServiceMessage::SetSchedule(schedule))
            .context("AI categorization worker is unavailable")
    }

    pub fn set_confidence_threshold(&self, threshold: f32) -> Result<()> {
        validate_confidence_threshold(threshold)?;
        let _gate = self.commit_gate.lock().unwrap();
        if self.reset_pending.load(Ordering::Acquire) {
            bail!("AI categorization reset is in progress");
        }
        write_setting(
            &self.conn,
            CONFIDENCE_THRESHOLD_SETTING_KEY,
            &threshold.to_string(),
        )?;
        self.status.lock().unwrap().confidence_threshold = threshold;
        drop(_gate);
        self.tx
            .send(ServiceMessage::SetConfidenceThreshold(threshold))
            .context("AI categorization worker is unavailable")
    }

    pub fn set_clustering_granularity(&self, granularity: ClusteringGranularity) -> Result<()> {
        let _gate = self.commit_gate.lock().unwrap();
        if self.reset_pending.load(Ordering::Acquire) {
            bail!("AI categorization reset is in progress");
        }
        write_setting(
            &self.conn,
            CLUSTER_GRANULARITY_SETTING_KEY,
            granularity.as_str(),
        )?;
        self.status.lock().unwrap().cluster_granularity = granularity.as_str().into();
        drop(_gate);
        self.tx
            .send(ServiceMessage::SetClusteringGranularity(granularity))
            .context("AI categorization worker is unavailable")
    }
}

struct ServiceRuntime {
    conn: SharedConnection,
    embedder: Arc<dyn Embedder>,
    schedule: AiCategorizationSchedule,
    confidence_threshold: f32,
    cluster_granularity: ClusteringGranularity,
    intervals: ScheduleIntervals,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<ClassifierStatus>>,
    commit_gate: Arc<Mutex<()>>,
    on_completed: CompletionCallback,
}

fn run_service(runtime: ServiceRuntime, rx: mpsc::Receiver<ServiceMessage>) {
    let ServiceRuntime {
        conn,
        embedder,
        mut schedule,
        mut confidence_threshold,
        mut cluster_granularity,
        intervals,
        generation,
        status,
        commit_gate,
        on_completed,
    } = runtime;
    let mut deadline = schedule_deadline(schedule, &intervals);
    loop {
        let message = match deadline.as_ref() {
            Some(deadline) => {
                match rx.recv_timeout(deadline.at.saturating_duration_since(Instant::now())) {
                    Ok(message) => Some(message),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let _gate = commit_gate.lock().unwrap();
                        let target = generation.fetch_add(1, Ordering::SeqCst) + 1;
                        let mut s = status.lock().unwrap();
                        s.state = "queued".into();
                        s.generation = target;
                        s.last_error = None;
                        Some(ServiceMessage::Run(target))
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            },
        };

        match message.expect("message is present unless the service stopped") {
            ServiceMessage::SetSchedule(next) => {
                schedule = next;
                deadline = schedule_deadline(schedule, &intervals);
                update_schedule_status(&status, schedule, deadline.as_ref());
            }
            ServiceMessage::SetConfidenceThreshold(next) => {
                confidence_threshold = next;
            }
            ServiceMessage::SetClusteringGranularity(next) => {
                cluster_granularity = next;
            }
            ServiceMessage::Reset(reply) => {
                let result = reset_categorization_data(&conn);
                match result {
                    Ok(outcome) => {
                        schedule = AiCategorizationSchedule::Manual;
                        deadline = None;
                        reset_status(
                            &status,
                            &generation,
                            confidence_threshold,
                            cluster_granularity,
                        );
                        let _ = reply.send(Ok(outcome));
                    }
                    Err(error) => {
                        let message = error.to_string();
                        let mut status = status.lock().unwrap();
                        status.state = "error".into();
                        status.last_error = Some(message.clone());
                        let _ = reply.send(Err(message));
                    }
                }
            }
            ServiceMessage::Run(mut target) => {
                let mut reset_seen = false;
                while let Ok(message) = rx.try_recv() {
                    match message {
                        ServiceMessage::SetSchedule(next) => schedule = next,
                        ServiceMessage::SetConfidenceThreshold(next) => {
                            confidence_threshold = next;
                        }
                        ServiceMessage::SetClusteringGranularity(next) => {
                            cluster_granularity = next;
                        }
                        ServiceMessage::Run(next) => target = next,
                        ServiceMessage::Reset(reply) => {
                            let result = reset_categorization_data(&conn);
                            match result {
                                Ok(outcome) => {
                                    schedule = AiCategorizationSchedule::Manual;
                                    deadline = None;
                                    reset_status(
                                        &status,
                                        &generation,
                                        confidence_threshold,
                                        cluster_granularity,
                                    );
                                    let _ = reply.send(Ok(outcome));
                                }
                                Err(error) => {
                                    let message = error.to_string();
                                    let mut status = status.lock().unwrap();
                                    status.state = "error".into();
                                    status.last_error = Some(message.clone());
                                    let _ = reply.send(Err(message));
                                }
                            }
                            reset_seen = true;
                            break;
                        }
                    }
                }
                if reset_seen {
                    continue;
                }
                deadline = schedule_deadline(schedule, &intervals);
                update_schedule_status(&status, schedule, deadline.as_ref());
                execute_job(
                    &conn,
                    embedder.as_ref(),
                    target,
                    &generation,
                    &status,
                    &commit_gate,
                    &on_completed,
                    confidence_threshold,
                    cluster_granularity,
                );
                deadline = schedule_deadline(schedule, &intervals);
                update_schedule_status(&status, schedule, deadline.as_ref());
            }
        }
    }
}

fn reset_status(
    status: &Mutex<ClassifierStatus>,
    generation: &AtomicU64,
    confidence_threshold: f32,
    cluster_granularity: ClusteringGranularity,
) {
    let mut status = status.lock().unwrap();
    status.state = "idle".into();
    status.generation = generation.load(Ordering::SeqCst);
    status.update_kind = None;
    status.training_example_count = 0;
    status.trained_category_count = 0;
    status.last_completed_at = None;
    status.last_execution_duration_ms = None;
    status.last_error = None;
    status.schedule = AiCategorizationSchedule::Manual.as_str().into();
    status.next_scheduled_at = None;
    status.confidence_threshold = confidence_threshold;
    status.holdout_accuracy = None;
    status.holdout_sample_count = 0;
    status.cluster_granularity = cluster_granularity.as_str().into();
}

#[allow(clippy::too_many_arguments)]
fn execute_job(
    conn: &SharedConnection,
    embedder: &dyn Embedder,
    target: u64,
    generation: &AtomicU64,
    status: &Mutex<ClassifierStatus>,
    commit_gate: &Mutex<()>,
    on_completed: &CompletionCallback,
    confidence_threshold: f32,
    cluster_granularity: ClusteringGranularity,
) {
    if generation.load(Ordering::SeqCst) != target {
        return;
    }
    info!(generation = target, "AI categorization job started");
    {
        let mut s = status.lock().unwrap();
        s.state = "running".into();
        s.generation = target;
        s.last_error = None;
    }
    let started_at = Instant::now();
    let result = update(
        conn,
        embedder,
        target,
        generation,
        commit_gate,
        confidence_threshold,
        cluster_granularity,
    );
    let current_generation = generation.load(Ordering::SeqCst);
    let execution_duration_ms = started_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut s = status.lock().unwrap();
    s.generation = current_generation;
    match result {
        Ok(UpdateResult::Completed {
            examples,
            categories,
            holdout_accuracy,
            holdout_sample_count,
            kind,
            ..
        }) => complete_job(
            conn,
            &mut s,
            if examples > 0 && categories > 0 {
                "ready"
            } else {
                "insufficient_data"
            },
            examples,
            categories,
            holdout_accuracy,
            holdout_sample_count,
            kind,
            execution_duration_ms,
            on_completed,
        ),
        Ok(UpdateResult::Stale) => {
            info!(
                generation = target,
                current = current_generation,
                "AI categorization job superseded"
            );
            s.state = if current_generation == target {
                "idle"
            } else {
                "queued"
            }
            .into();
        }
        Err(error) => {
            error!(
                generation = target,
                error = format!("{error:#}"),
                "AI categorization job failed"
            );
            s.state = "error".into();
            s.last_error = Some(error.to_string());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn complete_job(
    conn: &SharedConnection,
    status: &mut ClassifierStatus,
    state: &str,
    examples: usize,
    categories: usize,
    holdout_accuracy: Option<f32>,
    holdout_sample_count: usize,
    kind: UpdateKind,
    execution_duration_ms: u64,
    on_completed: &CompletionCallback,
) {
    let completed_at = Utc::now().to_rfc3339();
    info!(
        state,
        examples,
        categories,
        execution_duration_ms,
        kind = kind.as_str(),
        "AI categorization job finished"
    );
    let result = (|| {
        write_setting(conn, LAST_COMPLETED_SETTING_KEY, &completed_at)?;
        write_setting(
            conn,
            LAST_EXECUTION_DURATION_SETTING_KEY,
            &execution_duration_ms.to_string(),
        )?;
        write_setting(
            conn,
            HOLDOUT_SAMPLE_COUNT_SETTING_KEY,
            &holdout_sample_count.to_string(),
        )?;
        if let Some(accuracy) = holdout_accuracy {
            write_setting(conn, HOLDOUT_ACCURACY_SETTING_KEY, &accuracy.to_string())?;
        }
        Ok::<_, anyhow::Error>(())
    })();
    match result {
        Ok(()) => {
            status.state = state.into();
            status.update_kind = Some(kind.as_str().into());
            status.training_example_count = examples;
            status.trained_category_count = categories;
            status.last_completed_at = Some(completed_at);
            status.last_execution_duration_ms = Some(execution_duration_ms);
            status.holdout_accuracy = holdout_accuracy;
            status.holdout_sample_count = holdout_sample_count;
            on_completed();
        }
        Err(error) => {
            error!(
                error = format!("{error:#}"),
                "recording the AI categorization completion time failed"
            );
            status.state = "error".into();
            status.last_error = Some(error.to_string());
        }
    }
}

fn update_schedule_status(
    status: &Mutex<ClassifierStatus>,
    schedule: AiCategorizationSchedule,
    deadline: Option<&ScheduleDeadline>,
) {
    let mut status = status.lock().unwrap();
    status.schedule = schedule.as_str().into();
    status.next_scheduled_at = deadline.map(|deadline| deadline.scheduled_at.clone());
}

fn schedule_deadline(
    schedule: AiCategorizationSchedule,
    intervals: &ScheduleIntervals,
) -> Option<ScheduleDeadline> {
    let now_utc = Utc::now();
    let scheduled_at = match schedule {
        AiCategorizationSchedule::Manual => return None,
        AiCategorizationSchedule::Hourly | AiCategorizationSchedule::Daily => {
            next_local_boundary(schedule, Local::now())?.with_timezone(&Utc)
        }
        AiCategorizationSchedule::Weekly => {
            let delay = schedule.duration(intervals)?;
            now_utc + chrono::Duration::from_std(delay).ok()?
        }
    };
    let delay = (scheduled_at - now_utc).to_std().ok()?;
    Some(ScheduleDeadline {
        at: Instant::now() + delay,
        scheduled_at: scheduled_at.to_rfc3339(),
    })
}

fn next_local_boundary(
    schedule: AiCategorizationSchedule,
    now: DateTime<Local>,
) -> Option<DateTime<Local>> {
    next_boundary_naive(schedule, now.naive_local()).and_then(|value| {
        Local
            .from_local_datetime(&value)
            .single()
            .or_else(|| Local.from_local_datetime(&value).earliest())
            .or_else(|| Local.from_local_datetime(&value).latest())
    })
}

fn next_boundary_naive(
    schedule: AiCategorizationSchedule,
    now: NaiveDateTime,
) -> Option<NaiveDateTime> {
    match schedule {
        AiCategorizationSchedule::Hourly => now
            .date()
            .and_hms_opt(now.hour(), 0, 0)
            .map(|value| value + chrono::Duration::hours(1)),
        AiCategorizationSchedule::Daily => now
            .date()
            .succ_opt()
            .and_then(|date| date.and_hms_opt(0, 0, 0)),
        AiCategorizationSchedule::Manual | AiCategorizationSchedule::Weekly => None,
    }
}

fn schedule_from_status(status: &ClassifierStatus) -> Option<AiCategorizationSchedule> {
    AiCategorizationSchedule::parse(&status.schedule).ok()
}

fn read_setting(conn: &SharedConnection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?)
}

fn write_setting(conn: &SharedConnection, key: &str, value: &str) -> Result<()> {
    let conn = conn.lock().unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    if key == CONFIDENCE_THRESHOLD_SETTING_KEY || key == CLUSTER_GRANULARITY_SETTING_KEY {
        bump_input_revision(&conn)?;
    }
    Ok(())
}

fn bump_input_revision(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, '0')",
        params![INPUT_REVISION_SETTING_KEY],
    )?;
    conn.execute(
        "UPDATE settings SET value = CAST(value AS INTEGER) + 1 WHERE key = ?1",
        params![INPUT_REVISION_SETTING_KEY],
    )?;
    Ok(())
}

fn load_input_revision(conn: &SharedConnection) -> Result<i64> {
    let conn = conn.lock().unwrap();
    load_input_revision_from_conn(&conn)
}

fn load_input_revision_from_conn(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![INPUT_REVISION_SETTING_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .and_then(|value| value.parse().ok())
        .unwrap_or_default())
}

fn load_schedule(conn: &SharedConnection) -> Result<AiCategorizationSchedule> {
    read_setting(conn, SCHEDULE_SETTING_KEY)?
        .map(|value| AiCategorizationSchedule::parse(&value))
        .unwrap_or(Ok(AiCategorizationSchedule::Manual))
}

fn validate_confidence_threshold(threshold: f32) -> Result<()> {
    if !threshold.is_finite() || !(0.50..=0.99).contains(&threshold) {
        bail!("AI categorization confidence threshold must be between 0.50 and 0.99");
    }
    Ok(())
}

fn load_confidence_threshold(conn: &SharedConnection) -> Result<f32> {
    match read_setting(conn, CONFIDENCE_THRESHOLD_SETTING_KEY)? {
        Some(value) => {
            let threshold = value
                .parse::<f32>()
                .context("reading AI categorization confidence threshold")?;
            validate_confidence_threshold(threshold)?;
            Ok(threshold)
        }
        None => Ok(DEFAULT_CONFIDENCE_THRESHOLD),
    }
}

fn load_cluster_granularity(conn: &SharedConnection) -> Result<ClusteringGranularity> {
    match read_setting(conn, CLUSTER_GRANULARITY_SETTING_KEY)? {
        Some(value) => ClusteringGranularity::parse(&value),
        None => Ok(ClusteringGranularity::Balanced),
    }
}

#[derive(Debug, Clone)]
struct CategoryInfo {
    id: i64,
    name: String,
    description: Option<String>,
    is_leaf: bool,
}

#[derive(Debug, Clone)]
struct LabeledKey {
    category_id: i64,
    weight: f32,
    origin: LabelOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabelOrigin {
    Manual,
    Rule,
}

#[derive(Debug, Clone)]
struct Candidate {
    key_kind: String,
    key: String,
    category_id: i64,
    confidence: f32,
    seconds: f64,
}

/// Per-key label votes, counted separately so one manual pin still outweighs every rule row.
#[derive(Default)]
struct LabelVotes {
    manual: HashMap<i64, u64>,
    rules: HashMap<i64, u64>,
}

#[derive(Debug, Clone)]
struct ActivityPrediction {
    category_id: i64,
    confidence: f32,
}

#[derive(Debug, Clone)]
struct Dispersion {
    top_share: f64,
}

#[derive(Default)]
struct DispersionStats {
    titles: HashSet<String>,
    seconds: f64,
    category_seconds: HashMap<i64, f64>,
}

#[derive(Default)]
struct UrlStats {
    session_ids: HashSet<i64>,
    days: HashSet<String>,
    seconds: f64,
    category_seconds: HashMap<i64, f64>,
}

enum UpdateResult {
    Completed {
        examples: usize,
        categories: usize,
        holdout_accuracy: Option<f32>,
        holdout_sample_count: usize,
        kind: UpdateKind,
    },
    Stale,
}

#[derive(Clone, Copy)]
enum UpdateKind {
    Cached,
    Incremental,
    FullRebuild,
}

impl UpdateKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cached => "cached",
            Self::Incremental => "incremental",
            Self::FullRebuild => "full_rebuild",
        }
    }
}

#[derive(Clone)]
struct CachedEmbedding {
    doc_hash: String,
    model_id: String,
    vector: Vec<f32>,
}

#[derive(Debug, Clone)]
struct PendingClusterMember {
    activity_id: Option<i64>,
    key_kind: String,
    key: String,
    url_key: Option<String>,
    window_title: Option<String>,
    seconds: f64,
    session_count: usize,
}

#[derive(Debug, Clone)]
struct PendingCluster {
    scope: &'static str,
    parent: Option<Key>,
    label: String,
    category_id: Option<i64>,
    confidence: Option<f32>,
    lead: Option<f32>,
    cohesion: f32,
    seconds: f64,
    members: Vec<PendingClusterMember>,
}

struct ClusterOutcome {
    key_clusters: Vec<PendingCluster>,
    title_clusters: Vec<PendingCluster>,
    clustered_keys: HashSet<Key>,
}

fn cluster_category(
    centroid: &[f32],
    label_vectors: &HashMap<i64, Vec<f32>>,
) -> (Option<i64>, Option<f32>, Option<f32>) {
    rank_embedding(centroid, label_vectors)
        .map(|(category, confidence, lead)| (Some(category), Some(confidence), Some(lead)))
        .unwrap_or((None, None, None))
}

fn build_clusters(
    documents: &[super::keydoc::KeyDocument],
    activities: &[super::keydoc::ActivityDocument],
    embeddings: &HashMap<Key, Vec<f32>>,
    labels: &HashMap<Key, LabeledKey>,
    label_vectors: &HashMap<i64, Vec<f32>>,
    candidate_multi_purpose: &HashSet<Key>,
    granularity: ClusteringGranularity,
) -> ClusterOutcome {
    let key_documents = documents
        .iter()
        .filter(|document| {
            let key = (document.key_kind.clone(), document.key.clone());
            !candidate_multi_purpose.contains(&key) && !labels.contains_key(&key)
        })
        .filter(|document| {
            embeddings.contains_key(&(document.key_kind.clone(), document.key.clone()))
        })
        .collect::<Vec<_>>();
    let key_ids = key_documents
        .iter()
        .map(|document| format!("{}\u{1f}{}", document.key_kind, document.key))
        .collect::<Vec<_>>();
    let key_inputs = key_documents
        .iter()
        .zip(&key_ids)
        .map(|(document, id)| ClusterInput {
            id,
            vector: embeddings
                .get(&(document.key_kind.clone(), document.key.clone()))
                .expect("filtered key embedding must exist"),
            seconds: document.seconds,
            text: &document.document,
        })
        .collect::<Vec<_>>();
    let mut key_clusters = Vec::new();
    let key_pool_texts = key_inputs
        .iter()
        .map(|input| input.text)
        .collect::<Vec<_>>();
    let key_results = cluster(&key_inputs, granularity.params(MAX_KEY_CLUSTER_INPUT));
    let mut clustered_keys = HashSet::new();
    let key_session_counts =
        activities
            .iter()
            .fold(HashMap::<Key, usize>::new(), |mut counts, activity| {
                *counts
                    .entry((activity.key_kind.clone(), activity.key.clone()))
                    .or_default() += activity.session_ids.len();
                counts
            });
    for result in key_results {
        let mut member_positions = result.members.clone();
        member_positions.sort_by(|left, right| {
            key_inputs[*right]
                .seconds
                .total_cmp(&key_inputs[*left].seconds)
                .then_with(|| key_inputs[*left].id.cmp(key_inputs[*right].id))
        });
        let member_texts = member_positions
            .iter()
            .map(|position| key_inputs[*position].text)
            .collect::<Vec<_>>();
        let members = member_positions
            .iter()
            .map(|position| {
                let document = key_documents[*position];
                let key = (document.key_kind.clone(), document.key.clone());
                clustered_keys.insert(key);
                PendingClusterMember {
                    activity_id: None,
                    key_kind: document.key_kind.clone(),
                    key: document.key.clone(),
                    url_key: None,
                    window_title: None,
                    seconds: document.seconds,
                    session_count: key_session_counts
                        .get(&(document.key_kind.clone(), document.key.clone()))
                        .copied()
                        .unwrap_or_default(),
                }
            })
            .collect::<Vec<_>>();
        let (category_id, confidence, lead) = cluster_category(&result.centroid, label_vectors);
        key_clusters.push(PendingCluster {
            scope: "key",
            parent: None,
            label: label_cluster(&member_texts, &key_pool_texts, None),
            category_id,
            confidence,
            lead,
            cohesion: result.cohesion,
            seconds: result.seconds,
            members,
        });
    }

    let mut parents = candidate_multi_purpose
        .iter()
        .filter_map(|parent| {
            let seconds = activities
                .iter()
                .filter(|activity| &(activity.key_kind.clone(), activity.key.clone()) == parent)
                .map(|activity| activity.seconds)
                .sum::<f64>();
            (seconds > 0.0).then_some((parent.clone(), seconds))
        })
        .collect::<Vec<_>>();
    parents.sort_by(|(left, left_seconds), (right, right_seconds)| {
        right_seconds
            .total_cmp(left_seconds)
            .then_with(|| left.cmp(right))
    });
    let mut title_clusters = Vec::new();
    for (parent, _) in parents.into_iter().take(MAX_CLUSTERED_PARENTS) {
        let pool = activities
            .iter()
            .filter(|activity| {
                (activity.key_kind.clone(), activity.key.clone()) == parent
                    && !activity.window_title.trim().is_empty()
                    && activity.labeled_session_count == 0
            })
            .collect::<Vec<_>>();
        let title_activities = pool
            .iter()
            .filter(|activity| {
                embeddings.contains_key(&("title".to_string(), activity.cache_key()))
            })
            .collect::<Vec<_>>();
        let title_ids = title_activities
            .iter()
            .map(|activity| activity.cache_key())
            .collect::<Vec<_>>();
        let inputs = title_activities
            .iter()
            .zip(&title_ids)
            .map(|(activity, id)| ClusterInput {
                id,
                vector: embeddings
                    .get(&("title".to_string(), id.clone()))
                    .expect("filtered title embedding must exist"),
                seconds: activity.seconds,
                text: &activity.document,
            })
            .collect::<Vec<_>>();
        let pool_texts = inputs.iter().map(|input| input.text).collect::<Vec<_>>();
        for result in cluster(&inputs, granularity.params(MAX_TITLE_CLUSTER_INPUT)) {
            let mut positions = result.members.clone();
            positions.sort_by(|left, right| {
                inputs[*right]
                    .seconds
                    .total_cmp(&inputs[*left].seconds)
                    .then_with(|| inputs[*left].id.cmp(inputs[*right].id))
            });
            let member_texts = positions
                .iter()
                .map(|position| inputs[*position].text)
                .collect::<Vec<_>>();
            let members = positions
                .iter()
                .map(|position| {
                    let activity = title_activities[*position];
                    PendingClusterMember {
                        activity_id: activity.activity_id,
                        key_kind: activity.key_kind.clone(),
                        key: activity.key.clone(),
                        url_key: activity.url_key.clone(),
                        window_title: Some(activity.window_title.clone()),
                        seconds: activity.seconds,
                        session_count: activity.session_ids.len(),
                    }
                })
                .collect::<Vec<_>>();
            let (category_id, confidence, lead) = cluster_category(&result.centroid, label_vectors);
            title_clusters.push(PendingCluster {
                scope: "title",
                parent: Some(parent.clone()),
                label: label_cluster(&member_texts, &pool_texts, Some(&parent.1)),
                category_id,
                confidence,
                lead,
                cohesion: result.cohesion,
                seconds: result.seconds,
                members,
            });
        }
    }
    title_clusters.sort_by(|left, right| {
        right
            .seconds
            .total_cmp(&left.seconds)
            .then_with(|| left.parent.cmp(&right.parent))
    });
    title_clusters.truncate(MAX_CLUSTERS_PER_SCOPE);
    ClusterOutcome {
        key_clusters,
        title_clusters,
        clustered_keys,
    }
}

fn update(
    conn: &SharedConnection,
    embedder: &dyn Embedder,
    target: u64,
    generation: &AtomicU64,
    commit_gate: &Mutex<()>,
    confidence_threshold: f32,
    cluster_granularity: ClusteringGranularity,
) -> Result<UpdateResult> {
    let snapshot_input_revision = load_input_revision(conn)?;
    let (documents, activities) = {
        let conn = conn.lock().unwrap();
        (
            super::keydoc::build(&conn)?,
            super::keydoc::build_activities(&conn)?,
        )
    };
    let categories = load_categories(conn)?;
    let labels = load_labeled_keys(conn)?;
    let cached = load_cached_embeddings(conn)?;
    let mut embeddings = HashMap::new();
    let mut changed = Vec::new();
    let mut changed_keys = HashSet::new();
    for document in &documents {
        let key = (document.key_kind.clone(), document.key.clone());
        match cached.get(&key) {
            Some(cached)
                if cached.doc_hash == document.doc_hash
                    && cached.model_id == embedder.model_id()
                    && cached.vector.len() == embedder.dim() =>
            {
                embeddings.insert(key, cached.vector.clone());
            }
            _ => {
                changed_keys.insert(key);
                changed.push((
                    (document.key_kind.clone(), document.key.clone()),
                    document.doc_hash.clone(),
                    document.document.clone(),
                ));
            }
        }
    }
    for activity in &activities {
        let key = ("title".to_string(), activity.cache_key());
        match cached.get(&key) {
            Some(cached)
                if cached.doc_hash == activity.doc_hash
                    && cached.model_id == embedder.model_id()
                    && cached.vector.len() == embedder.dim() =>
            {
                embeddings.insert(key, cached.vector.clone());
            }
            _ => {
                changed_keys.insert(key.clone());
                changed.push((key, activity.doc_hash.clone(), activity.document.clone()));
            }
        }
    }
    if !changed.is_empty() {
        let texts = changed
            .iter()
            .map(|(_, _, document)| document.clone())
            .collect::<Vec<_>>();
        let vectors = embedder.embed(&texts)?;
        for ((key, _, _), vector) in changed.iter().zip(vectors) {
            embeddings.insert(key.clone(), vector);
        }
    }

    let category_signature = signature_for_categories(&categories);
    let previous_model_id = read_setting(conn, MODEL_ID_SETTING_KEY)?;
    let previous_category_signature = read_setting(conn, CATEGORY_SIGNATURE_SETTING_KEY)?;
    let kind = if previous_model_id.as_deref() == Some(embedder.model_id())
        && previous_category_signature.as_deref() == Some(category_signature.as_str())
        && changed_keys.is_empty()
    {
        UpdateKind::Cached
    } else if previous_model_id.as_deref() == Some(embedder.model_id())
        && previous_category_signature.as_deref() == Some(category_signature.as_str())
    {
        UpdateKind::Incremental
    } else {
        UpdateKind::FullRebuild
    };

    let description_texts = categories
        .iter()
        .map(|category| {
            format!(
                "{} {}",
                category.name,
                category.description.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let description_vectors = embedder.embed(&description_texts)?;
    let description_vectors = categories
        .iter()
        .zip(description_vectors)
        .map(|(category, vector)| (category.id, vector))
        .collect::<HashMap<_, _>>();
    let label_vectors = build_label_vectors(
        &categories,
        &embeddings,
        &labels,
        &description_vectors,
        None,
    );
    let holdout = evaluate_holdout(&categories, &embeddings, &labels, &description_vectors);
    let stored_multi_purpose = load_multi_purpose_keys(conn)?;
    let mut candidate_multi_purpose = stored_multi_purpose.clone();
    let predictions = activities
        .iter()
        .map(|activity| {
            embeddings
                .get(&("title".to_string(), activity.cache_key()))
                .and_then(|embedding| {
                    score_embedding(embedding, &label_vectors).map(|(category_id, confidence)| {
                        ActivityPrediction {
                            category_id,
                            confidence,
                        }
                    })
                })
        })
        .collect::<Vec<_>>();

    let mut dispersion_stats: HashMap<Key, DispersionStats> = HashMap::new();
    for (activity, prediction) in activities.iter().zip(&predictions) {
        let entry = dispersion_stats
            .entry((activity.key_kind.clone(), activity.key.clone()))
            .or_default();
        if !activity.window_title.trim().is_empty() {
            entry.titles.insert(activity.window_title.clone());
        }
        entry.seconds += activity.seconds;
        if let Some(prediction) = prediction {
            *entry
                .category_seconds
                .entry(prediction.category_id)
                .or_default() += activity.seconds;
        }
    }
    let dispersions = dispersion_stats
        .into_iter()
        .filter_map(|(key, stats)| {
            if stats.titles.len() < MIN_DISTINCT_TITLES || stats.seconds < MIN_DISPERSION_SECONDS {
                return None;
            }
            let top_seconds = stats.category_seconds.values().copied().fold(0.0, f64::max);
            let top_share = if stats.seconds > 0.0 {
                top_seconds / stats.seconds
            } else {
                0.0
            };
            let populated_categories = stats
                .category_seconds
                .values()
                .filter(|category_seconds| **category_seconds / stats.seconds >= MIN_CATEGORY_SHARE)
                .count();
            (top_share < MAX_TOP_SHARE && populated_categories >= 2)
                .then_some((key, Dispersion { top_share }))
        })
        .collect::<HashMap<_, _>>();
    candidate_multi_purpose.extend(dispersions.keys().cloned());

    let cluster_outcome = build_clusters(
        &documents,
        &activities,
        &embeddings,
        &labels,
        &label_vectors,
        &candidate_multi_purpose,
        cluster_granularity,
    );

    let candidates = documents
        .iter()
        .filter(|document| {
            !candidate_multi_purpose.contains(&(document.key_kind.clone(), document.key.clone()))
        })
        .filter(|document| {
            !cluster_outcome
                .clustered_keys
                .contains(&(document.key_kind.clone(), document.key.clone()))
        })
        .filter(|document| !labels.contains_key(&(document.key_kind.clone(), document.key.clone())))
        .filter_map(|document| {
            let embedding = embeddings.get(&(document.key_kind.clone(), document.key.clone()))?;
            let (category_id, confidence) = score_embedding(embedding, &label_vectors)?;
            (confidence >= confidence_threshold).then_some(Candidate {
                key_kind: document.key_kind.clone(),
                key: document.key.clone(),
                category_id,
                confidence,
                seconds: document.seconds,
            })
        })
        .collect::<Vec<_>>();

    let mut url_groups: HashMap<(Key, String), UrlStats> = HashMap::new();
    for (activity, prediction) in activities.iter().zip(&predictions) {
        let parent_key = (activity.key_kind.clone(), activity.key.clone());
        if !candidate_multi_purpose.contains(&parent_key) {
            continue;
        }
        let Some(url_key) = activity.url_key.clone() else {
            continue;
        };
        let Some(prediction) = prediction else {
            continue;
        };
        let entry = url_groups.entry((parent_key, url_key)).or_default();
        entry
            .session_ids
            .extend(activity.session_ids.iter().copied());
        entry.days.extend(activity.observation_days.iter().cloned());
        entry.seconds += activity.seconds;
        *entry
            .category_seconds
            .entry(prediction.category_id)
            .or_default() += activity.seconds;
    }
    let url_candidates = url_groups
        .into_iter()
        .filter_map(|((_, url_key), stats)| {
            if stats.session_ids.len() < MIN_URL_SESSIONS
                || stats.days.len() < MIN_URL_DAYS
                || stats.seconds <= 0.0
            {
                return None;
            }
            let (category_id, top_seconds) = stats
                .category_seconds
                .into_iter()
                .max_by(|left, right| left.1.total_cmp(&right.1))?;
            let confidence = top_seconds / stats.seconds;
            (confidence >= MIN_URL_CONSISTENCY).then_some(Candidate {
                key_kind: "url".into(),
                key: url_key,
                category_id,
                confidence: confidence as f32,
                seconds: stats.seconds,
            })
        })
        .collect::<Vec<_>>();
    let current_model_version = read_setting(conn, MODEL_VERSION_SETTING_KEY)?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default();
    let model_version = current_model_version
        + if matches!(kind, UpdateKind::Cached) {
            0
        } else {
            1
        };

    let _gate = commit_gate.lock().unwrap();
    if generation.load(Ordering::SeqCst) != target {
        return Ok(UpdateResult::Stale);
    }
    let mut conn = conn.lock().unwrap();
    let tx = conn.transaction()?;
    if load_input_revision_from_conn(&tx)? != snapshot_input_revision {
        return Ok(UpdateResult::Stale);
    }
    for (key, doc_hash, _) in &changed {
        if changed_keys.contains(key) {
            let vector = embeddings
                .get(key)
                .context("newly embedded document was not returned by the embedder")?;
            tx.execute(
                "INSERT INTO key_embeddings (key_kind, key, doc_hash, model_id, vector)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key_kind, key) DO UPDATE SET
                    doc_hash = excluded.doc_hash,
                    model_id = excluded.model_id,
                    vector = excluded.vector",
                params![
                    key.0,
                    key.1,
                    doc_hash,
                    embedder.model_id(),
                    vector_to_blob(vector),
                ],
            )?;
        }
    }
    for (key, dispersion) in &dispersions {
        tx.execute(
            "INSERT OR IGNORE INTO multi_purpose_keys
                (key_kind, key, source, enabled, top_share, detected_at)
             VALUES (?1, ?2, 'auto', 1, ?3, ?4)",
            params![key.0, key.1, dispersion.top_share, Utc::now().to_rfc3339()],
        )?;
        tx.execute(
            "UPDATE multi_purpose_keys
             SET top_share = ?3, detected_at = ?4
             WHERE key_kind = ?1 AND key = ?2 AND source != 'user'",
            params![key.0, key.1, dispersion.top_share, Utc::now().to_rfc3339()],
        )?;
    }
    let effective_multi_purpose = load_multi_purpose_keys_from_conn(&tx)?;
    super::store::resolve::resolve_all(&tx)?;
    tx.execute(
        "UPDATE activities
         SET ml_category_id = NULL, ml_confidence = NULL, ml_model_version = NULL
         WHERE category_id IS NULL AND rule_category_id IS NULL",
        [],
    )?;
    for (activity, prediction) in activities.iter().zip(&predictions) {
        let key = (activity.key_kind.clone(), activity.key.clone());
        let Some(prediction) = prediction else {
            continue;
        };
        if !effective_multi_purpose.contains(&key) || prediction.confidence < confidence_threshold {
            continue;
        }
        tx.execute(
            "UPDATE activities
             SET ml_category_id = ?1, ml_confidence = ?2, ml_model_version = ?3
             WHERE id = ?4 AND category_id IS NULL AND rule_category_id IS NULL",
            params![
                prediction.category_id,
                prediction.confidence,
                model_version,
                activity.activity_id
            ],
        )?;
    }
    let predicted_session_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sessions s JOIN activities a ON a.id = s.activity_id
         WHERE a.ml_model_version = ?1",
        params![model_version],
        |row| row.get(0),
    )?;
    let current_title_keys = activities
        .iter()
        .map(|activity| activity.cache_key())
        .collect::<HashSet<_>>();
    let stale_title_keys = tx
        .prepare("SELECT key FROM key_embeddings WHERE key_kind = 'title'")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for key in stale_title_keys {
        if !current_title_keys.contains(&key) {
            tx.execute(
                "DELETE FROM key_embeddings WHERE key_kind = 'title' AND key = ?1",
                params![key],
            )?;
        }
    }
    tx.execute(
        "DELETE FROM category_suggestions WHERE status = 'pending'",
        [],
    )?;
    let dismissed = load_dismissed_keys(&tx)?;
    let stored_rules = super::store::resolve::load_rules(&tx)?;
    let mut all_candidates = candidates;
    all_candidates.extend(url_candidates);
    for candidate in all_candidates {
        if dismissed.contains(&(candidate.key_kind.clone(), candidate.key.clone())) {
            continue;
        }
        if candidate.key_kind != "url" && rule_already_covers(&stored_rules, &candidate) {
            continue;
        }
        if candidate.key_kind == "url" {
            let parent_exists: bool = tx.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sessions s
                    JOIN multi_purpose_keys m
                      ON m.key_kind = s.category_key_kind AND m.key = s.category_key
                     AND m.enabled = 1
                    WHERE s.url_key = ?1
                )",
                params![candidate.key],
                |row| row.get(0),
            )?;
            let existing_url_rule: bool = tx.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM category_rules WHERE key_kind = 'url' AND pattern = ?1
                )",
                params![candidate.key],
                |row| row.get(0),
            )?;
            if !parent_exists || existing_url_rule {
                continue;
            }
        }
        tx.execute(
            "INSERT INTO category_suggestions
                (key_kind, key, category_id, confidence, seconds, model_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                candidate.key_kind,
                candidate.key,
                candidate.category_id,
                candidate.confidence,
                candidate.seconds,
                model_version,
                Utc::now().to_rfc3339(),
            ],
        )?;
    }
    persist_clusters(
        &tx,
        &cluster_outcome,
        &effective_multi_purpose,
        model_version,
    )?;
    write_setting_tx(&tx, MODEL_VERSION_SETTING_KEY, &model_version.to_string())?;
    write_setting_tx(&tx, MODEL_ID_SETTING_KEY, embedder.model_id())?;
    write_setting_tx(&tx, CATEGORY_SIGNATURE_SETTING_KEY, &category_signature)?;
    write_setting_tx(
        &tx,
        HOLDOUT_SAMPLE_COUNT_SETTING_KEY,
        &holdout.sample_count.to_string(),
    )?;
    if let Some(accuracy) = holdout.accuracy {
        write_setting_tx(&tx, HOLDOUT_ACCURACY_SETTING_KEY, &accuracy.to_string())?;
    }
    tx.execute(
        "INSERT INTO ai_run_evaluations (
             recorded_at, model_version, update_kind, confidence_threshold,
             holdout_accuracy, holdout_sample_count, manual_holdout_accuracy,
             manual_holdout_sample_count, predicted_session_count,
             labeled_key_count, category_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            Utc::now().to_rfc3339(),
            model_version,
            kind.as_str(),
            confidence_threshold,
            holdout.accuracy,
            holdout.sample_count as i64,
            holdout.manual_accuracy,
            holdout.manual_sample_count as i64,
            predicted_session_count,
            labels.len() as i64,
            label_vectors.len() as i64,
        ],
    )?;
    tx.commit()?;
    Ok(UpdateResult::Completed {
        examples: labels.len(),
        categories: label_vectors.len(),
        holdout_accuracy: holdout.accuracy,
        holdout_sample_count: holdout.sample_count,
        kind,
    })
}

fn write_setting_tx(tx: &rusqlite::Transaction<'_>, key: &str, value: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn persist_clusters(
    tx: &rusqlite::Transaction<'_>,
    outcome: &ClusterOutcome,
    effective_multi_purpose: &HashSet<Key>,
    model_version: i64,
) -> Result<()> {
    tx.execute("DELETE FROM activity_clusters", [])?;
    for cluster in outcome
        .key_clusters
        .iter()
        .chain(outcome.title_clusters.iter())
    {
        if cluster.scope == "title"
            && !cluster
                .parent
                .as_ref()
                .is_some_and(|parent| effective_multi_purpose.contains(parent))
        {
            continue;
        }
        let (parent_kind, parent_key) = cluster
            .parent
            .as_ref()
            .map(|(kind, key)| (Some(kind.as_str()), Some(key.as_str())))
            .unwrap_or((None, None));
        tx.execute(
            "INSERT INTO activity_clusters
                (scope, parent_kind, parent_key, label, category_id, confidence, lead,
                 cohesion, member_count, seconds, model_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                cluster.scope,
                parent_kind,
                parent_key,
                cluster.label,
                cluster.category_id,
                cluster.confidence,
                cluster.lead,
                cluster.cohesion,
                cluster.members.len() as i64,
                cluster.seconds.max(0.0),
                model_version,
                Utc::now().to_rfc3339(),
            ],
        )?;
        let cluster_id = tx.last_insert_rowid();
        for member in &cluster.members {
            tx.execute(
                "INSERT INTO activity_cluster_members
                    (cluster_id, activity_id, key_kind, key, url_key, window_title, seconds, session_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    cluster_id,
                    member.activity_id,
                    member.key_kind,
                    member.key,
                    member.url_key,
                    member.window_title,
                    member.seconds.max(0.0),
                    member.session_count as i64,
                ],
            )?;
        }
    }
    Ok(())
}

fn load_categories(conn: &SharedConnection) -> Result<Vec<CategoryInfo>> {
    let conn = conn.lock().unwrap();
    let mut stmt =
        conn.prepare("SELECT id, name, description, parent_id FROM categories ORDER BY id")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let parent_ids = rows
        .iter()
        .filter_map(|(_, _, _, parent_id)| *parent_id)
        .collect::<HashSet<_>>();
    Ok(rows
        .into_iter()
        .map(|(id, name, description, _)| CategoryInfo {
            id,
            name,
            description,
            is_leaf: !parent_ids.contains(&id),
        })
        .collect())
}

fn load_labeled_keys(conn: &SharedConnection) -> Result<HashMap<Key, LabeledKey>> {
    let conn = conn.lock().unwrap();
    let multi_purpose = load_multi_purpose_keys_from_conn(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT key_kind, key, url_key, title, category_id, rule_category_id
         FROM activities
         WHERE category_id IS NOT NULL OR rule_category_id IS NOT NULL
         UNION ALL
         SELECT category_key_kind, category_key, url_key, window_title,
                category_id, rule_category_id
         FROM sessions
         WHERE activity_id IS NULL
           AND (category_id IS NOT NULL OR rule_category_id IS NOT NULL)",
    )?;
    let mut evidence: HashMap<Key, LabelVotes> = HashMap::new();
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })? {
        let (kind, key, url_key, title, manual, rule) = row?;
        let parent = (kind.clone(), key.clone());
        let label_key = if multi_purpose.contains(&parent) {
            if title.trim().is_empty() {
                continue;
            }
            (
                "title".to_string(),
                super::keydoc::activity_cache_key(&kind, &key, url_key.as_deref(), &title),
            )
        } else {
            // Multi-purpose keys are never offered as standalone suggestions, so excluding
            // their mixed key document from training preserves the invariant that ruled keys
            // stay labeled without poisoning the taxonomy with unrelated titles.
            parent
        };
        let entry = evidence.entry(label_key).or_default();
        if let Some(category) = manual {
            *entry.manual.entry(category).or_default() += 1;
        } else if let Some(category) = rule {
            *entry.rules.entry(category).or_default() += 1;
        }
    }
    Ok(evidence
        .into_iter()
        .filter_map(|(key, votes)| {
            // A key keeps its label even when its pins disagree: one stale pin among many
            // agreeing ones must not erase the key from training *and* make it look unlabeled,
            // which is what re-suggested an already-ruled domain on every run.
            if let Some(category_id) = dominant_category(&votes.manual) {
                Some((
                    key,
                    LabeledKey {
                        category_id,
                        weight: 3.0,
                        origin: LabelOrigin::Manual,
                    },
                ))
            } else {
                dominant_category(&votes.rules).map(|category_id| {
                    (
                        key,
                        LabeledKey {
                            category_id,
                            weight: 1.0,
                            origin: LabelOrigin::Rule,
                        },
                    )
                })
            }
        })
        .collect())
}

/// Picks the category backing the most rows, breaking exact ties by lowest id for determinism.
fn dominant_category(counts: &HashMap<i64, u64>) -> Option<i64> {
    counts
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(category_id, _)| *category_id)
}

fn load_cached_embeddings(conn: &SharedConnection) -> Result<HashMap<Key, CachedEmbedding>> {
    let conn = conn.lock().unwrap();
    let mut stmt =
        conn.prepare("SELECT key_kind, key, doc_hash, model_id, vector FROM key_embeddings")?;
    let rows = stmt
        .query_map([], |row| {
            let bytes: Vec<u8> = row.get(4)?;
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                CachedEmbedding {
                    doc_hash: row.get(2)?,
                    model_id: row.get(3)?,
                    vector: blob_to_vector(&bytes),
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn load_multi_purpose_keys(conn: &SharedConnection) -> Result<HashSet<Key>> {
    let conn = conn.lock().unwrap();
    load_multi_purpose_keys_from_conn(&conn)
}

fn load_multi_purpose_keys_from_conn(conn: &Connection) -> Result<HashSet<Key>> {
    let mut stmt =
        conn.prepare("SELECT key_kind, key FROM multi_purpose_keys WHERE enabled = 1")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(rows)
}

/// Reports whether the stored rules already label an `exe`/`domain` candidate.
///
/// Suggesting a key a rule already resolves is noise the user cannot clear: the label is not
/// going to change. Multi-purpose keys never reach here, so the resolver is asked the same
/// question a plain session would ask, which also covers prefix, substring and glob rules.
fn rule_already_covers(rules: &[CompiledRule], candidate: &Candidate) -> bool {
    let Some(key_kind) = KeyKind::parse(&candidate.key_kind) else {
        return false;
    };
    resolve_category(
        rules,
        &SessionCategoryInput {
            key_kind,
            key: &candidate.key,
            url_key: None,
            multi_purpose: false,
        },
    )
    .is_some()
}

fn load_dismissed_keys(conn: &Connection) -> Result<HashSet<Key>> {
    let mut stmt =
        conn.prepare("SELECT key_kind, key FROM category_suggestions WHERE status = 'dismissed'")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn signature_for_categories(categories: &[CategoryInfo]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for category in categories {
        category.id.hash(&mut hasher);
        category.name.hash(&mut hasher);
        category.description.hash(&mut hasher);
        category.is_leaf.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn blob_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    }
}

fn build_label_vectors(
    categories: &[CategoryInfo],
    embeddings: &HashMap<Key, Vec<f32>>,
    labels: &HashMap<Key, LabeledKey>,
    description_vectors: &HashMap<i64, Vec<f32>>,
    excluded: Option<&Key>,
) -> HashMap<i64, Vec<f32>> {
    let dimension = description_vectors
        .values()
        .next()
        .map(Vec::len)
        .or_else(|| embeddings.values().next().map(Vec::len))
        .unwrap_or_default();
    let aggregates = build_label_aggregates(embeddings, labels, dimension);
    let excluded = excluded.and_then(|key| {
        labels.get(key).and_then(|label| {
            embeddings
                .get(key)
                .map(|vector| (label.category_id, vector.as_slice(), label.weight))
        })
    });
    build_label_vectors_from_aggregates(
        categories,
        &aggregates,
        description_vectors,
        dimension,
        excluded,
    )
}

fn build_label_aggregates(
    embeddings: &HashMap<Key, Vec<f32>>,
    labels: &HashMap<Key, LabeledKey>,
    dimension: usize,
) -> HashMap<i64, (Vec<f32>, usize)> {
    let mut aggregates = HashMap::new();
    for (key, label) in labels {
        let Some(vector) = embeddings.get(key) else {
            continue;
        };
        let entry = aggregates
            .entry(label.category_id)
            .or_insert_with(|| (vec![0.0; dimension], 0usize));
        for (target, value) in entry.0.iter_mut().zip(vector) {
            *target += *value * label.weight;
        }
        entry.1 += 1;
    }
    aggregates
}

fn build_category_vector(
    category: &CategoryInfo,
    aggregates: &HashMap<i64, (Vec<f32>, usize)>,
    description_vectors: &HashMap<i64, Vec<f32>>,
    dimension: usize,
    excluded: Option<(i64, &[f32], f32)>,
) -> Option<Vec<f32>> {
    let (mut sum, mut count) = aggregates
        .get(&category.id)
        .cloned()
        .unwrap_or_else(|| (vec![0.0; dimension], 0));
    if let Some((excluded_category, vector, weight)) = excluded {
        if excluded_category == category.id {
            for (target, value) in sum.iter_mut().zip(vector) {
                *target -= *value * weight;
            }
            count = count.saturating_sub(1);
        }
    }
    let n = count as f32;
    if n > 0.0 {
        normalize(&mut sum);
    }
    let description = description_vectors
        .get(&category.id)
        .cloned()
        .unwrap_or_else(|| vec![0.0; dimension]);
    let mut blended = blend_vectors(&sum, &description, n);
    normalize(&mut blended);
    blended.iter().any(|value| *value != 0.0).then_some(blended)
}

fn build_label_vectors_from_aggregates(
    categories: &[CategoryInfo],
    aggregates: &HashMap<i64, (Vec<f32>, usize)>,
    description_vectors: &HashMap<i64, Vec<f32>>,
    dimension: usize,
    excluded: Option<(i64, &[f32], f32)>,
) -> HashMap<i64, Vec<f32>> {
    categories
        .iter()
        .filter(|category| category.is_leaf)
        .filter_map(|category| {
            build_category_vector(
                category,
                aggregates,
                description_vectors,
                dimension,
                excluded,
            )
            .map(|vector| (category.id, vector))
        })
        .collect()
}

fn blend_vectors(prototype: &[f32], description: &[f32], n: f32) -> Vec<f32> {
    let prototype_weight = n / (n + 3.0);
    let description_weight = 3.0 / (n + 3.0);
    prototype
        .iter()
        .zip(description.iter())
        .map(|(prototype, description)| {
            prototype * prototype_weight + description * description_weight
        })
        .collect()
}

fn rank_embedding(
    embedding: &[f32],
    label_vectors: &HashMap<i64, Vec<f32>>,
) -> Option<(i64, f32, f32)> {
    let mut scores = label_vectors
        .iter()
        .map(|(category, vector)| (*category, cosine(embedding, vector)))
        .collect::<Vec<_>>();
    scores.sort_by_key(|(category, _)| *category);
    if scores.is_empty() {
        return None;
    }
    let probabilities = softmax(&scores.iter().map(|(_, score)| *score).collect::<Vec<_>>());
    let mut ranked = scores
        .into_iter()
        .zip(probabilities)
        .map(|((category, _), probability)| (category, probability))
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let (category, confidence) = ranked[0];
    let next = ranked
        .get(1)
        .map(|(_, probability)| *probability)
        .unwrap_or(0.0);
    Some((category, confidence, confidence - next))
}

fn score_embedding(
    embedding: &[f32],
    label_vectors: &HashMap<i64, Vec<f32>>,
) -> Option<(i64, f32)> {
    rank_embedding(embedding, label_vectors)
        .filter(|(_, _, lead)| *lead >= MIN_LEAD)
        .map(|(category, confidence, _)| (category, confidence))
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn softmax(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return Vec::new();
    }
    let scaled = scores
        .iter()
        .map(|score| *score / SOFTMAX_TEMPERATURE)
        .collect::<Vec<_>>();
    let max = scaled.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let values = scaled
        .iter()
        .map(|score| (*score - max).exp())
        .collect::<Vec<_>>();
    let normalizer = values.iter().sum::<f32>();
    values.into_iter().map(|value| value / normalizer).collect()
}

fn evaluate_holdout(
    categories: &[CategoryInfo],
    embeddings: &HashMap<Key, Vec<f32>>,
    labels: &HashMap<Key, LabeledKey>,
    description_vectors: &HashMap<i64, Vec<f32>>,
) -> HoldoutEvaluation {
    let dimension = description_vectors
        .values()
        .next()
        .map(Vec::len)
        .or_else(|| embeddings.values().next().map(Vec::len))
        .unwrap_or_default();
    let aggregates = build_label_aggregates(embeddings, labels, dimension);
    let base_vectors = build_label_vectors_from_aggregates(
        categories,
        &aggregates,
        description_vectors,
        dimension,
        None,
    );
    let mut correct = 0usize;
    let mut sample_count = 0usize;
    let mut manual_correct = 0usize;
    let mut manual_sample_count = 0usize;
    for (key, label) in labels {
        let Some(embedding) = embeddings.get(key) else {
            continue;
        };
        let excluded = (label.category_id, embedding.as_slice(), label.weight);
        let Some(predicted) = best_category_excluding(
            embedding,
            &base_vectors,
            categories,
            &aggregates,
            description_vectors,
            dimension,
            excluded,
        ) else {
            continue;
        };
        sample_count += 1;
        if predicted == label.category_id {
            correct += 1;
        }
        if label.origin == LabelOrigin::Manual {
            manual_sample_count += 1;
            if predicted == label.category_id {
                manual_correct += 1;
            }
        }
    }
    HoldoutEvaluation {
        accuracy: (sample_count > 0).then(|| correct as f32 / sample_count as f32),
        sample_count,
        manual_accuracy: (manual_sample_count > 0)
            .then(|| manual_correct as f32 / manual_sample_count as f32),
        manual_sample_count,
    }
}

#[derive(Debug, Clone, Copy)]
struct HoldoutEvaluation {
    accuracy: Option<f32>,
    sample_count: usize,
    manual_accuracy: Option<f32>,
    manual_sample_count: usize,
}

fn best_category_excluding(
    embedding: &[f32],
    base_vectors: &HashMap<i64, Vec<f32>>,
    categories: &[CategoryInfo],
    aggregates: &HashMap<i64, (Vec<f32>, usize)>,
    description_vectors: &HashMap<i64, Vec<f32>>,
    dimension: usize,
    excluded: (i64, &[f32], f32),
) -> Option<i64> {
    categories
        .iter()
        .filter(|category| category.is_leaf)
        .filter_map(|category| {
            let score = if category.id == excluded.0 {
                let vector = build_category_vector(
                    category,
                    aggregates,
                    description_vectors,
                    dimension,
                    Some(excluded),
                )?;
                cosine(embedding, &vector)
            } else {
                cosine(embedding, base_vectors.get(&category.id)?)
            };
            Some((category.id, score))
        })
        .max_by(|(left_id, left), (right_id, right)| {
            left.total_cmp(right).then_with(|| right_id.cmp(left_id))
        })
        .map(|(category, _)| category)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    #[test]
    fn hourly_schedule_targets_the_next_start_of_an_hour() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 8, 22)
            .unwrap()
            .and_hms_milli_opt(13, 42, 17, 500)
            .unwrap();

        assert_eq!(
            next_boundary_naive(AiCategorizationSchedule::Hourly, now),
            Some(
                chrono::NaiveDate::from_ymd_opt(2026, 8, 22)
                    .unwrap()
                    .and_hms_opt(14, 0, 0)
                    .unwrap()
            )
        );
    }

    #[test]
    fn daily_schedule_targets_the_next_midnight() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 8, 22)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        assert_eq!(
            next_boundary_naive(AiCategorizationSchedule::Daily, now),
            Some(
                chrono::NaiveDate::from_ymd_opt(2026, 8, 23)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
            )
        );
    }

    #[test]
    fn softmax_temperature_turns_cosine_margin_into_decisive_probabilities() {
        let probabilities = softmax(&[0.50, 0.45, 0.10]);
        assert!(probabilities[0] > 0.70);
        assert!((probabilities.iter().sum::<f32>() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn prototype_description_blend_covers_cold_start_and_many_examples() {
        let prototype = vec![1.0, 0.0];
        let description = vec![0.0, 1.0];
        assert_eq!(blend_vectors(&prototype, &description, 0.0), vec![0.0, 1.0]);
        let one = blend_vectors(&prototype, &description, 1.0);
        assert!(one[1] > one[0]);
        let ten = blend_vectors(&prototype, &description, 10.0);
        assert!(ten[0] > 0.7 && ten[1] < 0.3);
    }

    #[test]
    fn model_embeddings_transfer_github_semantics_to_gitlab() {
        let model = Model2Vec::new().unwrap();
        let vectors = model
            .embed(&[
                "github software repository source code pull request".to_string(),
                "gitlab software repository source code merge request".to_string(),
                "music streaming songs playlist artist".to_string(),
            ])
            .unwrap();
        assert!(cosine(&vectors[0], &vectors[1]) > cosine(&vectors[0], &vectors[2]));
    }

    #[test]
    fn rule_and_manual_labels_are_aggregated_per_key_with_manual_weight() {
        let path = std::env::temp_dir().join(format!(
            "makora-classifier-labels-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = open_connection(&path).unwrap();
        conn.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO categories (id, name, color) VALUES (1, 'Development', '#111111');
             INSERT INTO sessions
                (app_name, exe_path, window_title, category_key, category_key_kind,
                 start_time, end_time, category_id)
             VALUES ('Chrome', 'chrome', 'GitHub', 'github.com', 'domain',
                     '2024-01-01T00:00:00Z', '2024-01-01T00:01:00Z', 1);",
            )
            .unwrap();
        let labels = load_labeled_keys(&conn).unwrap();
        assert_eq!(
            labels[&(String::from("domain"), String::from("github.com"))].weight,
            3.0
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn conflicting_pins_keep_the_dominant_label_instead_of_dropping_the_key() {
        let path = std::env::temp_dir().join(format!(
            "makora-classifier-dominant-pin-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = open_connection(&path).unwrap();
        conn.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO categories (id, name, color)
                 VALUES (1, 'Social media', '#111111'), (2, 'News', '#222222');
             INSERT INTO sessions
                (app_name, exe_path, window_title, category_key, category_key_kind,
                 start_time, end_time, category_id)
             VALUES ('Chrome', 'chrome', 'Feed', 'facebook.com', 'domain',
                     '2024-01-01T00:00:00Z', '2024-01-01T00:01:00Z', 1),
                    ('Chrome', 'chrome', 'Feed', 'facebook.com', 'domain',
                     '2024-01-01T00:02:00Z', '2024-01-01T00:03:00Z', 1),
                    ('Chrome', 'chrome', 'Headline', 'facebook.com', 'domain',
                     '2024-01-01T00:04:00Z', '2024-01-01T00:05:00Z', 2);",
            )
            .unwrap();

        let labels = load_labeled_keys(&conn).unwrap();
        let label = &labels[&(String::from("domain"), String::from("facebook.com"))];
        assert_eq!(label.category_id, 1);
        assert_eq!(label.weight, 3.0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn multi_purpose_pins_train_title_labels_without_labeling_the_parent_key() {
        let path = std::env::temp_dir().join(format!(
            "makora-classifier-title-labels-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = open_connection(&path).unwrap();
        conn.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO categories (id, name, color) VALUES (1, 'Learning', '#111111');
                 INSERT INTO multi_purpose_keys (key_kind, key, source, enabled)
                    VALUES ('domain', 'youtube.com', 'user', 1);
                 INSERT INTO sessions
                    (app_name, exe_path, window_title, category_key, category_key_kind,
                     url_key, start_time, end_time, category_id)
                 VALUES ('Chrome', 'chrome', 'Rust lecture', 'youtube.com', 'domain',
                         'youtube.com/watch', '2024-01-01T00:00:00Z',
                         '2024-01-01T00:10:00Z', 1),
                        ('Chrome', 'chrome', 'Game stream', 'youtube.com', 'domain',
                         'youtube.com/watch', '2024-01-01T01:00:00Z',
                         '2024-01-01T01:10:00Z', NULL);",
            )
            .unwrap();
        let labels = load_labeled_keys(&conn).unwrap();
        assert!(!labels.contains_key(&("domain".into(), "youtube.com".into())));
        assert!(labels.contains_key(&(
            "title".into(),
            super::super::keydoc::activity_cache_key(
                "domain",
                "youtube.com",
                Some("youtube.com/watch"),
                "Rust lecture",
            )
        )));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn candidates_already_covered_by_a_rule_are_not_suggested() {
        use crate::core::category::{compile_rules, MatchKind, RuleSpec};

        let rules = compile_rules([
            RuleSpec {
                id: 1,
                key_kind: KeyKind::Domain,
                pattern: "facebook.com".into(),
                match_kind: MatchKind::Exact,
                category_id: 12,
            },
            RuleSpec {
                id: 2,
                key_kind: KeyKind::Domain,
                pattern: "*whatsapp*".into(),
                match_kind: MatchKind::Glob,
                category_id: 11,
            },
        ]);
        let candidate = |key: &str| Candidate {
            key_kind: "domain".into(),
            key: key.into(),
            category_id: 21,
            confidence: 0.8,
            seconds: 10.0,
        };

        assert!(rule_already_covers(&rules, &candidate("facebook.com")));
        assert!(rule_already_covers(&rules, &candidate("web.whatsapp.com")));
        assert!(!rule_already_covers(&rules, &candidate("figma.com")));
    }

    struct FixtureEmbedder;

    impl Embedder for FixtureEmbedder {
        fn model_id(&self) -> &str {
            "fixture"
        }

        fn dim(&self) -> usize {
            3
        }

        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts
                .iter()
                .map(|text| {
                    let text = text.to_ascii_lowercase();
                    if text.contains("lecture") || text.contains("course") {
                        vec![1.0, 0.0, 0.0]
                    } else if text.contains("game") || text.contains("stream") {
                        vec![0.0, 1.0, 0.0]
                    } else if text.contains("news") || text.contains("headline") {
                        vec![0.0, 0.0, 1.0]
                    } else {
                        vec![0.0, 0.0, 0.0]
                    }
                })
                .collect())
        }
    }

    #[test]
    fn title_pass_detects_mixed_site_labels_and_graduates_consistent_urls() {
        let path = std::env::temp_dir().join(format!(
            "makora-classifier-title-pass-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = open_connection(&path).unwrap();
        let db = conn.lock().unwrap();
        db.execute_batch(
            "INSERT INTO categories (id, name, color, description) VALUES
                (1, 'Courses', '#111111', 'lecture course lessons'),
                (2, 'Games', '#222222', 'game stream gameplay'),
                (3, 'News', '#333333', 'news headlines journalism');
             INSERT INTO category_rules (key_kind, pattern, match_kind, category_id) VALUES
                ('domain', 'courses.example', 'exact', 1),
                ('domain', 'games.example', 'exact', 2),
                ('domain', 'news.example', 'exact', 3);
             INSERT INTO multi_purpose_keys (key_kind, key, source, enabled)
                VALUES ('domain', 'youtube.com', 'seed', 1);",
        )
        .unwrap();
        let mut session_id = 10i64;
        let mut insert = |key: &str, title: &str, url_key: &str, day: &str, seconds: i64| {
            let start = format!("{day}T00:00:00Z");
            let end = format!("{day}T00:{:02}:00Z", seconds / 60);
            db.execute(
                "INSERT INTO sessions
                    (id, app_name, exe_path, window_title, url, category_key, category_key_kind,
                     url_key, start_time, end_time)
                 VALUES (?1, 'Chrome', 'chrome', ?2, ?3, ?4, 'domain', ?5, ?6, ?7)",
                params![
                    session_id,
                    title,
                    format!("https://{url_key}"),
                    key,
                    url_key,
                    start,
                    end
                ],
            )
            .unwrap();
            session_id += 1;
        };
        insert(
            "courses.example",
            "lecture course",
            "courses.example",
            "2024-01-01",
            60,
        );
        insert(
            "games.example",
            "game stream",
            "games.example",
            "2024-01-01",
            60,
        );
        insert(
            "news.example",
            "news headline",
            "news.example",
            "2024-01-01",
            60,
        );
        for index in 0..3 {
            insert(
                "youtube.com",
                &format!("lecture module {index}"),
                "youtube.com/watch",
                "2024-01-01",
                300,
            );
            insert(
                "youtube.com",
                &format!("game stream {index}"),
                "youtube.com/watch",
                "2024-01-02",
                350,
            );
            insert(
                "youtube.com",
                &format!("news headline {index}"),
                "youtube.com/watch",
                "2024-01-03",
                100,
            );
        }
        for index in 0..5 {
            insert(
                "youtube.com",
                &format!("lecture channel lesson {index}"),
                "youtube.com/@channel",
                if index % 2 == 0 {
                    "2024-01-04"
                } else {
                    "2024-01-05"
                },
                120,
            );
        }
        drop(db);
        crate::core::store::resolve::resolve_all(&conn.lock().unwrap()).unwrap();

        let generation = AtomicU64::new(0);
        let commit_gate = Mutex::new(());
        let result = update(
            &conn,
            &FixtureEmbedder,
            0,
            &generation,
            &commit_gate,
            0.75,
            ClusteringGranularity::Balanced,
        )
        .unwrap();
        assert!(matches!(result, UpdateResult::Completed { .. }));

        let db = conn.lock().unwrap();
        let domain_suggestion: Option<i64> = db
            .query_row(
                "SELECT id FROM category_suggestions
                 WHERE key_kind = 'domain' AND key = 'youtube.com'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(domain_suggestion, None);
        let url_suggestion: Option<(String, i64)> = db
            .query_row(
                "SELECT key, category_id FROM category_suggestions WHERE key_kind = 'url'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap();
        assert_eq!(url_suggestion, Some(("youtube.com/@channel".into(), 1)));
        let distinct_ml: i64 = db
            .query_row(
                "SELECT COUNT(DISTINCT a.ml_category_id) FROM activities a
                 WHERE a.key = 'youtube.com' AND a.ml_category_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(distinct_ml >= 3);
        let persisted_clusters: i64 = db
            .query_row("SELECT COUNT(*) FROM activity_clusters", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(
            persisted_clusters > 0,
            "the title pass should materialize reviewable clusters"
        );
        let top_share: f64 = db
            .query_row(
                "SELECT top_share FROM multi_purpose_keys WHERE key = 'youtube.com'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(top_share < 0.60);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn running_pipeline_suggests_an_unlabeled_related_domain_without_writing_ml_columns() {
        let path = std::env::temp_dir().join(format!(
            "makora-classifier-pipeline-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let conn = open_connection(&path).unwrap();
        let development = {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO categories (name, color, description)
                 VALUES ('Development', '#111111', 'Software development, source code and repositories')",
                [],
            )
            .unwrap();
            conn.last_insert_rowid()
        };
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO sessions
                    (app_name, exe_path, window_title, category_key, category_key_kind,
                     start_time, end_time, category_id)
                 VALUES ('Chrome', 'chrome', 'GitHub source code repository pull request',
                         'github.com', 'domain', '2024-01-01T00:00:00Z',
                         '2024-01-01T00:10:00Z', ?1)",
                params![development],
            )
            .unwrap();
        let candidate = {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO sessions
                    (app_name, exe_path, window_title, category_key, category_key_kind,
                     start_time, end_time)
                 VALUES ('Chrome', 'chrome', 'GitLab source code repository merge request',
                         'gitlab.com', 'domain', '2024-01-01T01:00:00Z',
                         '2024-01-01T01:10:00Z')",
                [],
            )
            .unwrap();
            conn.last_insert_rowid()
        };
        let service = AiCategorizationService::new_with_intervals(
            conn.clone(),
            ScheduleIntervals::default(),
            Arc::new(|| {}),
        )
        .unwrap();
        service.run_now().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = service.status();
            if status.last_completed_at.is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "classifier pipeline did not complete"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            service.status().last_execution_duration_ms.is_some(),
            "completed classifier jobs should record their execution time"
        );
        let db = conn.lock().unwrap();
        let suggestion: Option<(i64, f32)> = db
            .query_row(
                "SELECT id, confidence FROM category_suggestions
                 WHERE key_kind = 'domain' AND key = 'gitlab.com'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap();
        let suggestion_id = suggestion
            .map(|(id, _)| id)
            .expect("gitlab should be suggested");
        let suggestion_category: i64 = db
            .query_row(
                "SELECT category_id FROM category_suggestions WHERE id = ?1",
                params![suggestion_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(suggestion_category, development);
        assert!(suggestion.unwrap().1 >= DEFAULT_CONFIDENCE_THRESHOLD);
        let ml_category: Option<i64> = db
            .query_row(
                "SELECT ml_category_id FROM sessions WHERE id = ?1",
                params![candidate],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ml_category, None);
        drop(db);
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE category_suggestions SET status = 'dismissed' WHERE id = ?1",
                params![suggestion_id],
            )
            .unwrap();
        service.run_now().unwrap();
        let second_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = service.status();
            if status.generation >= 2
                && matches!(status.state.as_str(), "ready" | "insufficient_data")
            {
                break;
            }
            assert!(
                Instant::now() < second_deadline,
                "classifier second pipeline did not complete"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let pending: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM category_suggestions WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending, 0);

        service.run_now().unwrap();
        let reset = service.reset_categorization().unwrap();
        assert_eq!(reset.suggestions_deleted, 1);
        let status = service.status();
        assert_eq!(status.state, "idle");
        assert_eq!(status.schedule, "manual");
        assert_eq!(status.next_scheduled_at, None);
        assert_eq!(status.last_completed_at, None);
        assert_eq!(status.last_execution_duration_ms, None);
        let db = conn.lock().unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM category_suggestions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM key_embeddings", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(db);

        service.run_now().unwrap();
        let third_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = service.status();
            if status.last_completed_at.is_some() {
                break;
            }
            assert!(
                Instant::now() < third_deadline,
                "classifier did not run after reset"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(service);
        let _ = std::fs::remove_file(path);
    }
}
