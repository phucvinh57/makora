//! The intentionally small, local-only activity classifier.  The model is a persisted
//! TF-IDF vectorizer plus a multinomial Naive Bayes count table; SQLite pins remain the
//! source of truth and the artifact is only a startup cache.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use url::Url;

use super::store::sqlite::SharedConnection;

type CompletionCallback = Arc<dyn Fn() + Send + Sync>;

const MIN_EXAMPLES: usize = 5;
const MAX_VOCABULARY: usize = 4096;
const VECTOR_VERSION: u32 = 1;
const MIN_CONFIDENCE: f32 = 0.75;
const MIN_LEAD: f32 = 0.15;
const SCHEDULE_SETTING_KEY: &str = "ai_categorization_schedule";
const LAST_COMPLETED_SETTING_KEY: &str = "ai_categorization_last_completed_at";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
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
}

impl ClassifierStatus {
    fn initial(
        artifact: Option<&Artifact>,
        schedule: AiCategorizationSchedule,
        last_completed_at: Option<String>,
        intervals: &ScheduleIntervals,
    ) -> Self {
        Self {
            state: "idle".into(),
            generation: 0,
            update_kind: None,
            training_example_count: artifact.map_or(0, |model| model.labels.len()),
            trained_category_count: artifact.map_or(0, |model| model.category_ids.len()),
            last_completed_at,
            last_error: None,
            schedule: schedule.as_str().into(),
            next_scheduled_at: next_scheduled_at(schedule, intervals),
        }
    }
}

enum ServiceMessage {
    Run,
    SetSchedule(AiCategorizationSchedule),
}

#[derive(Clone)]
pub struct AiCategorizationService {
    tx: mpsc::Sender<ServiceMessage>,
    conn: SharedConnection,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<ClassifierStatus>>,
    intervals: ScheduleIntervals,
}

impl AiCategorizationService {
    pub(crate) fn new_with_completion(
        conn: SharedConnection,
        artifact_path: PathBuf,
        on_completed: CompletionCallback,
    ) -> Result<Self> {
        Self::new_with_intervals(
            conn,
            artifact_path,
            ScheduleIntervals::default(),
            on_completed,
        )
    }

    fn new_with_intervals(
        conn: SharedConnection,
        artifact_path: PathBuf,
        intervals: ScheduleIntervals,
        on_completed: CompletionCallback,
    ) -> Result<Self> {
        let artifact = load_artifact(&artifact_path).ok().flatten();
        let schedule = load_schedule(&conn)?;
        let last_completed_at = read_setting(&conn, LAST_COMPLETED_SETTING_KEY)?;
        let (tx, rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let status = Arc::new(Mutex::new(ClassifierStatus::initial(
            artifact.as_ref(),
            schedule,
            last_completed_at,
            &intervals,
        )));
        let service = Self {
            tx,
            conn: conn.clone(),
            generation: generation.clone(),
            status: status.clone(),
            intervals,
        };
        std::thread::Builder::new()
            .name("makora-classifier".into())
            .spawn(move || {
                run_service(
                    ServiceRuntime {
                        conn,
                        artifact_path,
                        artifact,
                        schedule,
                        intervals,
                        generation,
                        status,
                        on_completed,
                    },
                    rx,
                )
            })
            .context("spawning classifier worker")?;
        Ok(service)
    }

    pub fn run_now(&self) -> Result<()> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        {
            let mut status = self.status.lock().unwrap();
            status.state = "queued".into();
            status.generation = self.generation.load(Ordering::SeqCst);
            status.last_error = None;
            status.next_scheduled_at = schedule_from_status(&status)
                .and_then(|schedule| next_scheduled_at(schedule, &self.intervals));
        }
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
}

struct ServiceRuntime {
    conn: SharedConnection,
    artifact_path: PathBuf,
    artifact: Option<Artifact>,
    schedule: AiCategorizationSchedule,
    intervals: ScheduleIntervals,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<ClassifierStatus>>,
    on_completed: CompletionCallback,
}

fn run_service(runtime: ServiceRuntime, rx: mpsc::Receiver<ServiceMessage>) {
    let ServiceRuntime {
        conn,
        artifact_path,
        mut artifact,
        mut schedule,
        intervals,
        generation,
        status,
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
            ServiceMessage::Run => {
                // Collapse triggers that arrived before work began. Schedule changes still take
                // effect immediately and the latest generation owns the eventual database write.
                while let Ok(message) = rx.try_recv() {
                    if let ServiceMessage::SetSchedule(next) = message {
                        schedule = next;
                    }
                }
                deadline = schedule
                    .duration(&intervals)
                    .map(|delay| Instant::now() + delay);
                update_schedule_status(&status, schedule, &intervals);
                execute_job(
                    &conn,
                    &artifact_path,
                    &mut artifact,
                    &generation,
                    &status,
                    &on_completed,
                );
            }
        }
    }
}

fn execute_job(
    conn: &SharedConnection,
    artifact_path: &PathBuf,
    artifact: &mut Option<Artifact>,
    generation: &AtomicU64,
    status: &Mutex<ClassifierStatus>,
    on_completed: &CompletionCallback,
) {
    let target = generation.load(Ordering::SeqCst);
    {
        let mut s = status.lock().unwrap();
        s.state = "running".into();
        s.generation = target;
        s.last_error = None;
    }
    let result = update(conn, artifact, target, generation, artifact_path);
    let current_generation = generation.load(Ordering::SeqCst);
    let mut s = status.lock().unwrap();
    s.generation = current_generation;
    match result {
        Ok(UpdateResult::Ready {
            examples,
            categories,
            kind,
        }) => complete_job(
            conn,
            &mut s,
            "ready",
            examples,
            categories,
            kind,
            on_completed,
        ),
        Ok(UpdateResult::Insufficient {
            examples,
            categories,
            kind,
        }) => complete_job(
            conn,
            &mut s,
            "insufficient_data",
            examples,
            categories,
            kind,
            on_completed,
        ),
        Ok(UpdateResult::Stale) => {
            s.state = if current_generation == target {
                "idle"
            } else {
                "queued"
            }
            .into();
        }
        Err(error) => {
            s.state = "error".into();
            s.last_error = Some(error.to_string());
        }
    }
}

fn complete_job(
    conn: &SharedConnection,
    status: &mut ClassifierStatus,
    state: &str,
    examples: usize,
    categories: usize,
    kind: UpdateKind,
    on_completed: &CompletionCallback,
) {
    let completed_at = Utc::now().to_rfc3339();
    match write_setting(conn, LAST_COMPLETED_SETTING_KEY, &completed_at) {
        Ok(()) => {
            status.state = state.into();
            status.update_kind = Some(kind.as_str().into());
            status.training_example_count = examples;
            status.trained_category_count = categories;
            status.last_completed_at = Some(completed_at);
            on_completed();
        }
        Err(error) => {
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

#[derive(Debug, Clone)]
struct Example {
    id: i64,
    category_id: i64,
    document: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LabelSnapshot {
    id: i64,
    category_id: i64,
    document: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Artifact {
    vectorizer_version: u32,
    model_version: i64,
    vocabulary: Vec<String>,
    idf: Vec<f32>,
    category_ids: Vec<i64>,
    class_documents: Vec<f32>,
    feature_counts: Vec<Vec<f32>>,
    feature_totals: Vec<f32>,
    labels: Vec<LabelSnapshot>,
    incremental_additions: usize,
}

enum UpdateResult {
    Ready {
        examples: usize,
        categories: usize,
        kind: UpdateKind,
    },
    Insufficient {
        examples: usize,
        categories: usize,
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

fn update(
    conn: &SharedConnection,
    artifact: &mut Option<Artifact>,
    target: u64,
    generation: &AtomicU64,
    artifact_path: &PathBuf,
) -> Result<UpdateResult> {
    let examples = snapshot_examples(conn)?;
    let (deduped, total_categories) = deduplicate(examples);
    let eligible = eligible_examples(&deduped);
    if eligible.is_empty() || eligible_categories(&deduped).len() < 2 {
        if generation.load(Ordering::SeqCst) != target {
            return Ok(UpdateResult::Stale);
        }
        clear_predictions(conn)?;
        remove_artifact(artifact_path)?;
        *artifact = None;
        return Ok(UpdateResult::Insufficient {
            examples: deduped.len(),
            categories: total_categories,
            kind: UpdateKind::FullRebuild,
        });
    }

    let mut model = None;
    let mut update_kind = UpdateKind::FullRebuild;
    if let Some(current) = artifact.as_ref() {
        if current.vectorizer_version == VECTOR_VERSION {
            let old: HashMap<i64, (i64, String)> = current
                .labels
                .iter()
                .map(|x| (x.id, (x.category_id, x.document.clone())))
                .collect();
            let additions: Vec<_> = eligible
                .iter()
                .filter(|x| !old.contains_key(&x.id))
                .cloned()
                .collect();
            let unchanged = old.iter().all(|(id, (category, doc))| {
                deduped
                    .iter()
                    .any(|x| x.id == *id && x.category_id == *category && x.document == *doc)
            });
            let tokens = additions
                .iter()
                .flat_map(|x| tokenize_document(&x.document))
                .collect::<Vec<_>>();
            let oov = tokens
                .iter()
                .filter(|term| current.vocabulary.binary_search(term).is_err())
                .count();
            if unchanged
                && !additions.is_empty()
                && current.incremental_additions + additions.len() <= 100
                && oov * 5 <= tokens.len().max(1)
            {
                model = Some(incremental(current.clone(), &additions));
                update_kind = UpdateKind::Incremental;
            } else if unchanged && additions.is_empty() {
                model = Some(current.clone());
                update_kind = UpdateKind::Cached;
            }
        }
    }
    let mut model = model.unwrap_or_else(|| fit(&eligible));
    if !matches!(update_kind, UpdateKind::Cached) {
        model.model_version = model.model_version.max(0) + 1;
    }
    model.labels = deduped
        .iter()
        .map(|x| LabelSnapshot {
            id: x.id,
            category_id: x.category_id,
            document: x.document.clone(),
        })
        .collect();
    if generation.load(Ordering::SeqCst) != target {
        return Ok(UpdateResult::Stale);
    }
    score_predictions(conn, &model)?;
    if generation.load(Ordering::SeqCst) != target {
        return Ok(UpdateResult::Stale);
    }
    persist(artifact_path, &model)?;
    *artifact = Some(model.clone());
    Ok(UpdateResult::Ready {
        examples: eligible.len(),
        categories: model.category_ids.len(),
        kind: update_kind,
    })
}

fn snapshot_examples(conn: &SharedConnection) -> Result<Vec<Example>> {
    let conn = conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, category_id, app_name, exe_path, window_title, url FROM sessions WHERE category_id IS NOT NULL")?;
    let examples = stmt
        .query_map([], |row| {
            Ok(Example {
                id: row.get(0)?,
                category_id: row.get(1)?,
                document: document(
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                    &row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?.as_deref(),
                ),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(examples)
}

fn deduplicate(examples: Vec<Example>) -> (Vec<Example>, usize) {
    let total = examples
        .iter()
        .map(|x| x.category_id)
        .collect::<HashSet<_>>()
        .len();
    let mut grouped: BTreeMap<String, Vec<Example>> = BTreeMap::new();
    for example in examples {
        grouped
            .entry(example.document.clone())
            .or_default()
            .push(example);
    }
    let unique = grouped
        .into_values()
        .filter_map(|mut group| {
            let labels = group.iter().map(|x| x.category_id).collect::<HashSet<_>>();
            if labels.len() == 1 {
                group.sort_by_key(|x| x.id);
                group.into_iter().next()
            } else {
                None
            }
        })
        .collect();
    (unique, total)
}

fn eligible_categories(examples: &[Example]) -> BTreeSet<i64> {
    let mut counts = BTreeMap::new();
    for x in examples {
        *counts.entry(x.category_id).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .filter_map(|(id, n)| (n >= MIN_EXAMPLES).then_some(id))
        .collect()
}
fn eligible_examples(examples: &[Example]) -> Vec<Example> {
    let categories = eligible_categories(examples);
    examples
        .iter()
        .filter(|x| categories.contains(&x.category_id))
        .cloned()
        .collect()
}

fn fit(examples: &[Example]) -> Artifact {
    let mut df: BTreeMap<String, usize> = BTreeMap::new();
    for x in examples {
        for t in tokenize_document(&x.document)
            .into_iter()
            .collect::<BTreeSet<_>>()
        {
            *df.entry(t).or_default() += 1;
        }
    }
    let vocabulary: Vec<String> = df
        .iter()
        .filter(|(_, n)| **n >= 2)
        .map(|(term, _)| term.clone())
        .take(MAX_VOCABULARY)
        .collect();
    let idf = vocabulary
        .iter()
        .map(|term| {
            ((examples.len() + 1) as f32 / (*df.get(term).unwrap_or(&0) + 1) as f32).ln() + 1.0
        })
        .collect();
    let category_ids = examples
        .iter()
        .map(|x| x.category_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut artifact = Artifact {
        vectorizer_version: VECTOR_VERSION,
        model_version: 0,
        vocabulary,
        idf,
        category_ids,
        class_documents: vec![],
        feature_counts: vec![],
        feature_totals: vec![],
        labels: vec![],
        incremental_additions: 0,
    };
    artifact.class_documents = vec![0.0; artifact.category_ids.len()];
    artifact.feature_counts =
        vec![vec![0.0; artifact.vocabulary.len()]; artifact.category_ids.len()];
    artifact.feature_totals = vec![0.0; artifact.category_ids.len()];
    incorporate(&mut artifact, examples);
    artifact
}

fn incremental(mut artifact: Artifact, additions: &[Example]) -> Artifact {
    incorporate(&mut artifact, additions);
    artifact.incremental_additions += additions.len();
    artifact
}
fn incorporate(artifact: &mut Artifact, examples: &[Example]) {
    for example in examples {
        if let Ok(class) = artifact.category_ids.binary_search(&example.category_id) {
            artifact.class_documents[class] += 1.0;
            let vector = vectorize(&example.document, &artifact.vocabulary, &artifact.idf);
            for (i, value) in vector {
                artifact.feature_counts[class][i] += value;
                artifact.feature_totals[class] += value;
            }
        }
    }
}

fn score_predictions(conn: &SharedConnection, model: &Artifact) -> Result<()> {
    let candidates = {
        let conn = conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, app_name, exe_path, window_title, url FROM sessions WHERE end_time IS NOT NULL AND category_id IS NULL AND rule_category_id IS NULL")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    document(
                        &r.get::<_, String>(1)?,
                        &r.get::<_, String>(2)?,
                        &r.get::<_, String>(3)?,
                        r.get::<_, Option<String>>(4)?.as_deref(),
                    ),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let mut conn = conn.lock().unwrap();
    let tx = conn.transaction()?;
    tx.execute("UPDATE sessions SET ml_category_id = NULL, ml_confidence = NULL, ml_model_version = NULL WHERE category_id IS NOT NULL OR rule_category_id IS NOT NULL", [])?;
    let mut write = tx.prepare("UPDATE sessions SET ml_category_id = ?1, ml_confidence = ?2, ml_model_version = ?3 WHERE id = ?4")?;
    for (id, doc) in candidates {
        let result = predict(model, &doc);
        match result {
            Some((category, confidence)) => {
                write.execute(params![category, confidence, model.model_version, id])?;
            }
            None => {
                write.execute(params![
                    Option::<i64>::None,
                    Option::<f32>::None,
                    Option::<i64>::None,
                    id
                ])?;
            }
        }
    }
    drop(write);
    tx.commit()?;
    Ok(())
}
fn clear_predictions(conn: &SharedConnection) -> Result<()> {
    conn.lock().unwrap().execute("UPDATE sessions SET ml_category_id = NULL, ml_confidence = NULL, ml_model_version = NULL WHERE ml_category_id IS NOT NULL", [])?;
    Ok(())
}

fn predict(model: &Artifact, doc: &str) -> Option<(i64, f32)> {
    let vector = vectorize(doc, &model.vocabulary, &model.idf);
    if vector.is_empty() {
        return None;
    }
    let total_docs: f32 = model.class_documents.iter().sum();
    let vocab = model.vocabulary.len() as f32;
    let mut scores = model
        .class_documents
        .iter()
        .enumerate()
        .map(|(class, docs)| {
            let mut score = (*docs / total_docs).ln();
            for (i, value) in &vector {
                score += value
                    * ((model.feature_counts[class][*i] + 1.0)
                        / (model.feature_totals[class] + vocab))
                        .ln();
            }
            score
        })
        .collect::<Vec<_>>();
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    for s in &mut scores {
        *s = (*s - max).exp();
    }
    let normalizer: f32 = scores.iter().sum();
    for s in &mut scores {
        *s /= normalizer;
    }
    let mut ranked = scores.into_iter().enumerate().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (class, confidence) = ranked[0];
    let next = ranked.get(1).map(|x| x.1).unwrap_or(0.0);
    (confidence >= MIN_CONFIDENCE && confidence - next >= MIN_LEAD)
        .then_some((model.category_ids[class], confidence))
}

fn document(app: &str, exe: &str, title: &str, raw_url: Option<&str>) -> String {
    let (domain, path) = raw_url
        .and_then(|x| Url::parse(x).ok())
        .map(|url| {
            (
                url.host_str().unwrap_or_default().to_owned(),
                url.path().to_owned(),
            )
        })
        .unwrap_or_default();
    format!("app={app}\u{1f}exe={exe}\u{1f}domain={domain}\u{1f}path={path}\u{1f}title={title}")
}
fn tokenize_document(document: &str) -> Vec<String> {
    document
        .split('\u{1f}')
        .flat_map(|field| {
            let (name, value) = field.split_once('=').unwrap_or(("text", field));
            let words = value
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|x| !x.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut out = words
                .iter()
                .map(|w| format!("{name}:{w}"))
                .collect::<Vec<_>>();
            out.extend(
                words
                    .windows(2)
                    .map(|p| format!("{name}:{}_{}", p[0], p[1])),
            );
            out
        })
        .collect()
}
fn vectorize(document: &str, vocabulary: &[String], idf: &[f32]) -> Vec<(usize, f32)> {
    let mut counts = BTreeMap::new();
    for term in tokenize_document(document) {
        if let Ok(i) = vocabulary.binary_search(&term) {
            *counts.entry(i).or_insert(0usize) += 1;
        }
    }
    let mut values = counts
        .into_iter()
        .map(|(i, n)| (i, (1.0 + (n as f32).ln()) * idf[i]))
        .collect::<Vec<_>>();
    let norm = values.iter().map(|(_, x)| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for (_, x) in &mut values {
            *x /= norm;
        }
    }
    values
}

fn load_artifact(path: &PathBuf) -> Result<Option<Artifact>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("reading classifier artifact")?,
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}
fn persist(path: &PathBuf, artifact: &Artifact) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec(artifact)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn remove_artifact(path: &PathBuf) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::sqlite::open_connection;

    fn temp_service(
        schedule: AiCategorizationSchedule,
        intervals: ScheduleIntervals,
    ) -> (AiCategorizationService, SharedConnection, PathBuf, PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64;
        let db_path = std::env::temp_dir().join(format!("makora-ai-service-{unique}.sqlite"));
        let artifact_path = db_path.with_extension("json");
        let conn = open_connection(&db_path).unwrap();
        write_setting(&conn, SCHEDULE_SETTING_KEY, schedule.as_str()).unwrap();
        let service = AiCategorizationService::new_with_intervals(
            conn.clone(),
            artifact_path.clone(),
            intervals,
            Arc::new(|| {}),
        )
        .unwrap();
        (service, conn, db_path, artifact_path)
    }

    fn short_intervals(delay: Duration) -> ScheduleIntervals {
        ScheduleIntervals {
            hourly: delay,
            daily: delay,
            weekly: delay,
        }
    }

    fn wait_for_completion(service: &AiCategorizationService) -> ClassifierStatus {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = service.status();
            if status.last_completed_at.is_some() {
                return status;
            }
            assert!(Instant::now() < deadline, "AI job did not complete in time");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_category_named_distraction_is_an_ordinary_training_label() {
        let (service, conn, db_path, artifact_path) = temp_service(
            AiCategorizationSchedule::Manual,
            ScheduleIntervals::default(),
        );
        let category_id = {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO categories (name, color) VALUES ('Distraction', '#a1a1aa')",
                [],
            )
            .unwrap();
            let category_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO sessions (
                    app_name, exe_path, window_title, category_key, category_key_kind,
                    start_time, end_time, category_id
                 ) VALUES (
                    'feed.exe', 'feed.exe', 'Feed', 'feed.exe', 'exe',
                    '2024-06-15T00:00:00Z', '2024-06-15T00:10:00Z', ?1
                 )",
                params![category_id],
            )
            .unwrap();
            category_id
        };

        let examples = snapshot_examples(&conn).unwrap();
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].category_id, category_id);

        drop(service);
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(artifact_path);
    }

    #[test]
    fn tokenizer_separates_fields_and_generates_bigrams() {
        let tokens = tokenize_document(&document(
            "Code",
            "/usr/bin/code",
            "Read Rust docs",
            Some("https://docs.rs/serde"),
        ));
        assert!(tokens.contains(&"app:code".into()));
        assert!(tokens.contains(&"title:read_rust".into()));
        assert!(tokens.contains(&"domain:docs".into()));
    }
    #[test]
    fn conflicting_documents_are_discarded() {
        let (rows, _) = deduplicate(vec![
            Example {
                id: 1,
                category_id: 1,
                document: "same".into(),
            },
            Example {
                id: 2,
                category_id: 2,
                document: "same".into(),
            },
        ]);
        assert!(rows.is_empty());
    }

    #[test]
    fn schedule_values_are_strictly_validated() {
        assert_eq!(
            AiCategorizationSchedule::parse("weekly").unwrap(),
            AiCategorizationSchedule::Weekly
        );
        assert!(AiCategorizationSchedule::parse("sometimes").is_err());
    }

    #[test]
    fn manual_schedule_never_runs_without_an_explicit_trigger() {
        let (service, _conn, db_path, artifact_path) = temp_service(
            AiCategorizationSchedule::Manual,
            short_intervals(Duration::from_millis(15)),
        );
        std::thread::sleep(Duration::from_millis(60));

        let status = service.status();
        assert_eq!(status.generation, 0);
        assert_eq!(status.state, "idle");
        assert_eq!(status.next_scheduled_at, None);

        drop(service);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(artifact_path);
    }

    #[test]
    fn enabled_schedule_waits_a_fresh_interval_then_runs_in_the_background() {
        let delay = Duration::from_millis(80);
        let (service, conn, db_path, artifact_path) =
            temp_service(AiCategorizationSchedule::Hourly, short_intervals(delay));
        write_setting(&conn, LAST_COMPLETED_SETTING_KEY, "2000-01-01T00:00:00Z").unwrap();

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            service.status().generation,
            0,
            "an overdue persisted completion must not cause startup catch-up"
        );

        let status = wait_for_completion(&service);
        assert!(status.generation >= 1);
        assert_eq!(status.state, "insufficient_data");
        assert_eq!(status.schedule, "hourly");
        assert!(status.next_scheduled_at.is_some());

        service
            .set_schedule(AiCategorizationSchedule::Manual)
            .unwrap();
        assert_eq!(
            read_setting(&conn, SCHEDULE_SETTING_KEY)
                .unwrap()
                .as_deref(),
            Some("manual")
        );
        drop(service);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(artifact_path);
    }

    #[test]
    fn manual_trigger_is_queued_and_resets_the_automatic_deadline() {
        let delay = Duration::from_secs(60);
        let (service, _conn, db_path, artifact_path) =
            temp_service(AiCategorizationSchedule::Daily, short_intervals(delay));
        let before = service.status().next_scheduled_at.unwrap();
        std::thread::sleep(Duration::from_millis(5));

        service.run_now().unwrap();
        let queued = service.status();
        assert!(matches!(queued.state.as_str(), "queued" | "running"));
        assert!(queued.next_scheduled_at.unwrap() >= before);
        assert_eq!(wait_for_completion(&service).state, "insufficient_data");

        drop(service);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(artifact_path);
    }

    #[test]
    fn predictions_are_materialized_only_after_a_manual_background_run() {
        let (service, conn, db_path, artifact_path) = temp_service(
            AiCategorizationSchedule::Manual,
            short_intervals(Duration::from_millis(20)),
        );
        let (development, music) = {
            let conn = conn.lock().unwrap();
            conn.execute(
                "INSERT INTO categories (name, color) VALUES ('Development', '#111111')",
                [],
            )
            .unwrap();
            let development = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO categories (name, color) VALUES ('Music', '#222222')",
                [],
            )
            .unwrap();
            (development, conn.last_insert_rowid())
        };
        for (category, app, title) in [
            (development, "Code", "rust compiler workspace"),
            (music, "Spotify", "music playlist album"),
        ] {
            for index in 0..5 {
                conn.lock()
                    .unwrap()
                    .execute(
                        "INSERT INTO sessions (
                            app_name, exe_path, window_title, category_key,
                            category_key_kind, start_time, end_time, category_id
                         ) VALUES (?1, ?2, ?3, ?2, 'exe', ?4, ?5, ?6)",
                        params![
                            app,
                            app.to_lowercase(),
                            format!("{title} {index}"),
                            format!("2024-06-15T0{index}:00:00Z"),
                            format!("2024-06-15T0{index}:10:00Z"),
                            category,
                        ],
                    )
                    .unwrap();
            }
        }
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO sessions (
                    app_name, exe_path, window_title, category_key,
                    category_key_kind, start_time, end_time
                 ) VALUES (
                    'Code', 'code', 'rust compiler workspace', 'code', 'exe',
                    '2024-06-16T00:00:00Z', '2024-06-16T00:10:00Z'
                 )",
                [],
            )
            .unwrap();
        let candidate = conn.lock().unwrap().last_insert_rowid();
        let predicted_category = || {
            conn.lock()
                .unwrap()
                .query_row(
                    "SELECT ml_category_id FROM sessions WHERE id = ?1",
                    params![candidate],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .unwrap()
        };

        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(predicted_category(), None);
        service.run_now().unwrap();
        assert_eq!(wait_for_completion(&service).state, "ready");
        assert_eq!(predicted_category(), Some(development));

        drop(service);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(artifact_path);
    }
}
