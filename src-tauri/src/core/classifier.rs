//! Local, review-first categorization based on semantic embeddings of stable activity keys.
//!
//! The worker never writes an AI category onto a session. It writes key-level suggestions;
//! accepting one creates an ordinary exact rule, which makes the decision permanent,
//! auditable, and useful as future supervision.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use tracing::{error, info};

use super::embed::{Embedder, Model2Vec};
use super::store::sqlite::SharedConnection;

type CompletionCallback = Arc<dyn Fn() + Send + Sync>;
type Key = (String, String);

const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.75;
const MIN_LEAD: f32 = 0.15;
const SOFTMAX_TEMPERATURE: f32 = 0.05;
const SCHEDULE_SETTING_KEY: &str = "ai_categorization_schedule";
const LAST_COMPLETED_SETTING_KEY: &str = "ai_categorization_last_completed_at";
const CONFIDENCE_THRESHOLD_SETTING_KEY: &str = "ai_categorization_confidence_threshold";
const MODEL_VERSION_SETTING_KEY: &str = "ai_categorization_model_version";
const MODEL_ID_SETTING_KEY: &str = "ai_categorization_model_id";
const CATEGORY_SIGNATURE_SETTING_KEY: &str = "ai_categorization_category_signature";
const HOLDOUT_ACCURACY_SETTING_KEY: &str = "ai_categorization_holdout_accuracy";
const HOLDOUT_SAMPLE_COUNT_SETTING_KEY: &str = "ai_categorization_holdout_sample_count";

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

#[derive(Clone, Copy)]
struct ScheduleIntervals {
    hourly: Duration,
    daily: Duration,
    weekly: Duration,
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
    pub last_error: Option<String>,
    pub schedule: String,
    pub next_scheduled_at: Option<String>,
    pub confidence_threshold: f32,
    pub holdout_accuracy: Option<f32>,
    pub holdout_sample_count: usize,
}

impl ClassifierStatus {
    fn initial(
        schedule: AiCategorizationSchedule,
        last_completed_at: Option<String>,
        confidence_threshold: f32,
        holdout_accuracy: Option<f32>,
        holdout_sample_count: usize,
        intervals: &ScheduleIntervals,
    ) -> Self {
        Self {
            state: "idle".into(),
            generation: 0,
            update_kind: None,
            training_example_count: 0,
            trained_category_count: 0,
            last_completed_at,
            last_error: None,
            schedule: schedule.as_str().into(),
            next_scheduled_at: next_scheduled_at(schedule, intervals),
            confidence_threshold,
            holdout_accuracy,
            holdout_sample_count,
        }
    }
}

enum ServiceMessage {
    Run,
    SetSchedule(AiCategorizationSchedule),
    SetConfidenceThreshold(f32),
}

#[derive(Clone)]
pub struct AiCategorizationService {
    tx: mpsc::Sender<ServiceMessage>,
    conn: SharedConnection,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<ClassifierStatus>>,
    intervals: ScheduleIntervals,
    commit_gate: Arc<Mutex<()>>,
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
        let last_completed_at = read_setting(&conn, LAST_COMPLETED_SETTING_KEY)?;
        let holdout_accuracy = read_setting(&conn, HOLDOUT_ACCURACY_SETTING_KEY)?
            .and_then(|value| value.parse::<f32>().ok());
        let holdout_sample_count = read_setting(&conn, HOLDOUT_SAMPLE_COUNT_SETTING_KEY)?
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let commit_gate = Arc::new(Mutex::new(()));
        let status = Arc::new(Mutex::new(ClassifierStatus::initial(
            schedule,
            last_completed_at,
            confidence_threshold,
            holdout_accuracy,
            holdout_sample_count,
            &intervals,
        )));
        let service = Self {
            tx,
            conn: conn.clone(),
            generation: generation.clone(),
            status: status.clone(),
            intervals,
            commit_gate: commit_gate.clone(),
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
        let _gate = self.commit_gate.lock().unwrap();
        self.generation.fetch_add(1, Ordering::SeqCst);
        {
            let mut status = self.status.lock().unwrap();
            status.state = "queued".into();
            status.generation = self.generation.load(Ordering::SeqCst);
            status.last_error = None;
            status.next_scheduled_at = schedule_from_status(&status)
                .and_then(|schedule| next_scheduled_at(schedule, &self.intervals));
        }
        drop(_gate);
        self.tx
            .send(ServiceMessage::Run)
            .context("AI categorization worker is unavailable")
    }

    pub fn status(&self) -> ClassifierStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn set_schedule(&self, schedule: AiCategorizationSchedule) -> Result<()> {
        write_setting(&self.conn, SCHEDULE_SETTING_KEY, schedule.as_str())?;
        {
            let mut status = self.status.lock().unwrap();
            status.schedule = schedule.as_str().into();
            status.next_scheduled_at = next_scheduled_at(schedule, &self.intervals);
        }
        self.tx
            .send(ServiceMessage::SetSchedule(schedule))
            .context("AI categorization worker is unavailable")
    }

    pub fn set_confidence_threshold(&self, threshold: f32) -> Result<()> {
        validate_confidence_threshold(threshold)?;
        write_setting(
            &self.conn,
            CONFIDENCE_THRESHOLD_SETTING_KEY,
            &threshold.to_string(),
        )?;
        self.status.lock().unwrap().confidence_threshold = threshold;
        self.tx
            .send(ServiceMessage::SetConfidenceThreshold(threshold))
            .context("AI categorization worker is unavailable")
    }
}

struct ServiceRuntime {
    conn: SharedConnection,
    embedder: Arc<dyn Embedder>,
    schedule: AiCategorizationSchedule,
    confidence_threshold: f32,
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
        intervals,
        generation,
        status,
        commit_gate,
        on_completed,
    } = runtime;
    let mut deadline = schedule
        .duration(&intervals)
        .map(|delay| Instant::now() + delay);
    loop {
        let message = match deadline {
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(message) => Some(message),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _gate = commit_gate.lock().unwrap();
                    generation.fetch_add(1, Ordering::SeqCst);
                    let mut s = status.lock().unwrap();
                    s.state = "queued".into();
                    s.generation = generation.load(Ordering::SeqCst);
                    s.last_error = None;
                    Some(ServiceMessage::Run)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            },
        };

        match message.expect("message is present unless the service stopped") {
            ServiceMessage::SetSchedule(next) => {
                schedule = next;
                deadline = schedule
                    .duration(&intervals)
                    .map(|delay| Instant::now() + delay);
                update_schedule_status(&status, schedule, &intervals);
            }
            ServiceMessage::SetConfidenceThreshold(next) => {
                confidence_threshold = next;
            }
            ServiceMessage::Run => {
                while let Ok(message) = rx.try_recv() {
                    if let ServiceMessage::SetSchedule(next) = message {
                        schedule = next;
                    } else if let ServiceMessage::SetConfidenceThreshold(next) = message {
                        confidence_threshold = next;
                    }
                }
                deadline = schedule
                    .duration(&intervals)
                    .map(|delay| Instant::now() + delay);
                update_schedule_status(&status, schedule, &intervals);
                execute_job(
                    &conn,
                    embedder.as_ref(),
                    &generation,
                    &status,
                    &commit_gate,
                    &on_completed,
                    confidence_threshold,
                );
            }
        }
    }
}

fn execute_job(
    conn: &SharedConnection,
    embedder: &dyn Embedder,
    generation: &AtomicU64,
    status: &Mutex<ClassifierStatus>,
    commit_gate: &Mutex<()>,
    on_completed: &CompletionCallback,
    confidence_threshold: f32,
) {
    let target = generation.load(Ordering::SeqCst);
    info!(generation = target, "AI categorization job started");
    {
        let mut s = status.lock().unwrap();
        s.state = "running".into();
        s.generation = target;
        s.last_error = None;
    }
    let result = update(
        conn,
        embedder,
        target,
        generation,
        commit_gate,
        confidence_threshold,
    );
    let current_generation = generation.load(Ordering::SeqCst);
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
    on_completed: &CompletionCallback,
) {
    let completed_at = Utc::now().to_rfc3339();
    info!(
        state,
        examples,
        categories,
        kind = kind.as_str(),
        "AI categorization job finished"
    );
    let result = (|| {
        write_setting(conn, LAST_COMPLETED_SETTING_KEY, &completed_at)?;
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
    intervals: &ScheduleIntervals,
) {
    let mut status = status.lock().unwrap();
    status.schedule = schedule.as_str().into();
    status.next_scheduled_at = next_scheduled_at(schedule, intervals);
}

fn next_scheduled_at(
    schedule: AiCategorizationSchedule,
    intervals: &ScheduleIntervals,
) -> Option<String> {
    let delay = schedule.duration(intervals)?;
    let delay = chrono::Duration::from_std(delay).ok()?;
    Some((Utc::now() + delay).to_rfc3339())
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
    conn.lock().unwrap().execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
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

#[derive(Debug, Clone)]
struct CategoryInfo {
    id: i64,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Clone)]
struct LabeledKey {
    category_id: i64,
    weight: f32,
}

#[derive(Debug, Clone)]
struct Candidate {
    key_kind: String,
    key: String,
    category_id: i64,
    confidence: f32,
    seconds: f64,
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

fn update(
    conn: &SharedConnection,
    embedder: &dyn Embedder,
    target: u64,
    generation: &AtomicU64,
    commit_gate: &Mutex<()>,
    confidence_threshold: f32,
) -> Result<UpdateResult> {
    let documents = {
        let conn = conn.lock().unwrap();
        super::keydoc::build(&conn)?
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
                changed.push(document.clone());
            }
        }
    }
    if !changed.is_empty() {
        let texts = changed
            .iter()
            .map(|document| document.document.clone())
            .collect::<Vec<_>>();
        let vectors = embedder.embed(&texts)?;
        for (document, vector) in changed.iter().zip(vectors) {
            embeddings.insert((document.key_kind.clone(), document.key.clone()), vector);
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
    let (holdout_accuracy, holdout_sample_count) =
        evaluate_holdout(&categories, &embeddings, &labels, &description_vectors);
    let candidates = documents
        .iter()
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
    for document in &documents {
        let key = (document.key_kind.clone(), document.key.clone());
        if changed_keys.contains(&key) {
            let vector = embeddings
                .get(&key)
                .context("newly embedded key was not returned by the embedder")?;
            tx.execute(
                "INSERT INTO key_embeddings (key_kind, key, doc_hash, model_id, vector)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key_kind, key) DO UPDATE SET
                    doc_hash = excluded.doc_hash,
                    model_id = excluded.model_id,
                    vector = excluded.vector",
                params![
                    document.key_kind,
                    document.key,
                    document.doc_hash,
                    embedder.model_id(),
                    vector_to_blob(vector),
                ],
            )?;
        }
    }
    tx.execute(
        "DELETE FROM category_suggestions WHERE status = 'pending'",
        [],
    )?;
    let dismissed = load_dismissed_keys(&tx)?;
    for candidate in candidates {
        if dismissed.contains(&(candidate.key_kind.clone(), candidate.key.clone())) {
            continue;
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
    write_setting_tx(&tx, MODEL_VERSION_SETTING_KEY, &model_version.to_string())?;
    write_setting_tx(&tx, MODEL_ID_SETTING_KEY, embedder.model_id())?;
    write_setting_tx(&tx, CATEGORY_SIGNATURE_SETTING_KEY, &category_signature)?;
    write_setting_tx(
        &tx,
        HOLDOUT_SAMPLE_COUNT_SETTING_KEY,
        &holdout_sample_count.to_string(),
    )?;
    if let Some(accuracy) = holdout_accuracy {
        write_setting_tx(&tx, HOLDOUT_ACCURACY_SETTING_KEY, &accuracy.to_string())?;
    }
    tx.commit()?;
    Ok(UpdateResult::Completed {
        examples: labels.len(),
        categories: label_vectors.len(),
        holdout_accuracy,
        holdout_sample_count,
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

fn load_categories(conn: &SharedConnection) -> Result<Vec<CategoryInfo>> {
    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, description FROM categories ORDER BY id")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CategoryInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn load_labeled_keys(conn: &SharedConnection) -> Result<HashMap<Key, LabeledKey>> {
    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT category_key_kind, category_key, category_id, rule_category_id
         FROM sessions
         WHERE category_id IS NOT NULL OR rule_category_id IS NOT NULL",
    )?;
    let mut evidence: HashMap<Key, (HashSet<i64>, HashSet<i64>)> = HashMap::new();
    for row in stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })? {
        let (kind, key, manual, rule) = row?;
        let entry = evidence.entry((kind, key)).or_default();
        if let Some(category) = manual {
            entry.0.insert(category);
        } else if let Some(category) = rule {
            entry.1.insert(category);
        }
    }
    Ok(evidence
        .into_iter()
        .filter_map(|(key, (manual, rules))| {
            if manual.len() == 1 {
                Some((
                    key,
                    LabeledKey {
                        category_id: *manual.iter().next().unwrap(),
                        weight: 3.0,
                    },
                ))
            } else if manual.is_empty() && rules.len() == 1 {
                Some((
                    key,
                    LabeledKey {
                        category_id: *rules.iter().next().unwrap(),
                        weight: 1.0,
                    },
                ))
            } else {
                None
            }
        })
        .collect())
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
    categories
        .iter()
        .filter_map(|category| {
            let examples = labels
                .iter()
                .filter(|(key, label)| label.category_id == category.id && excluded != Some(key))
                .filter_map(|(key, label)| embeddings.get(key).map(|vector| (vector, label.weight)))
                .collect::<Vec<_>>();
            let n = examples.len() as f32;
            let mut prototype = vec![0.0; dimension];
            for (vector, weight) in &examples {
                for (target, value) in prototype.iter_mut().zip(vector.iter()) {
                    *target += *value * *weight;
                }
            }
            if n > 0.0 {
                normalize(&mut prototype);
            }
            let description = description_vectors
                .get(&category.id)
                .cloned()
                .unwrap_or_else(|| vec![0.0; dimension]);
            let mut blended = blend_vectors(&prototype, &description, n);
            normalize(&mut blended);
            (blended.iter().any(|value| *value != 0.0)).then_some((category.id, blended))
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

fn score_embedding(
    embedding: &[f32],
    label_vectors: &HashMap<i64, Vec<f32>>,
) -> Option<(i64, f32)> {
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
    (confidence - next >= MIN_LEAD).then_some((category, confidence))
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
) -> (Option<f32>, usize) {
    let mut correct = 0usize;
    let mut sample_count = 0usize;
    for (key, label) in labels {
        let Some(embedding) = embeddings.get(key) else {
            continue;
        };
        let label_vectors = build_label_vectors(
            categories,
            embeddings,
            labels,
            description_vectors,
            Some(key),
        );
        let Some(predicted) = best_category(embedding, &label_vectors) else {
            continue;
        };
        sample_count += 1;
        if predicted == label.category_id {
            correct += 1;
        }
    }
    (
        if sample_count > 0 {
            Some(correct as f32 / sample_count as f32)
        } else {
            None
        },
        sample_count,
    )
}

fn best_category(embedding: &[f32], label_vectors: &HashMap<i64, Vec<f32>>) -> Option<i64> {
    label_vectors
        .iter()
        .map(|(category, vector)| (*category, cosine(embedding, vector)))
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
        drop(service);
        let _ = std::fs::remove_file(path);
    }
}
