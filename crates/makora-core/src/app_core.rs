//! The composition root: opens the database and settings, wires each context's adapters to its
//! ports, and starts the background work. A front end holds one [`AppCore`] and calls the services
//! on it.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{error, info};

use crate::ai::embedding::LlamaEmbedder;
use crate::ai::llm::{self, LlamaClassifier, ModelCache};
use crate::ai::model_download::{ModelDownloadState, ModelDownloads, OllamaDownloads};
use crate::ai::providers::{KeychainProviders, ProviderSvc};
use crate::ai::router::LlmRouter;
use crate::ai::secrets::{KeyringSecrets, SecretStore};
use crate::categorization::application::ai_run::AiRunState;
use crate::categorization::application::usage::SqliteAiUsage;
use crate::categorization::ports::{Classifier, Embedder};
use crate::categorization::{
    ActivityIndexer, AiCategorizationRunner, AiUsageSvc, AutoCategorizeSchedule, BackgroundIndexer,
    CategorizationRunner, ClassificationSvc, EmbedSkips, EmbeddingSvc, RuleSvc, ScheduleAnchor,
    ScheduledCategorization, SqliteClassification, SqliteEmbeddings, SqliteRules,
};
use crate::db;
use crate::domain::{HistorySvc, LabelingSvc, SqliteHistory, SqliteLabeling};
use crate::events::EventSink;
use crate::logging;
use crate::logs::{JsonlLogReader, LogReader};
use crate::paths::AppPaths;
use crate::settings::{SettingsService, SettingsStore, SettingsSvc, TomlSettings};
use crate::shared::sync::lock;
use crate::taxonomy::{CategorySvc, SqliteCategories};
use crate::tracking;
use crate::tracking::adapters::model::WatchEvent;
use crate::tracking::adapters::platform::PlatformFocusWatcher;
use crate::tracking::adapters::watcher::FocusWatcher;
use crate::tracking::application::{SettingsExclusions, SqliteSessionRecorder};
use crate::tracking::domain::focus_machine::FocusStateMachine;
use crate::tracking::ports::{ExclusionPolicy, SessionRecorder, SessionTracker};

pub type SharedTracker = Arc<Mutex<Box<dyn SessionTracker>>>;

const HEARTBEAT: Duration = Duration::from_secs(30);

pub struct AppCore {
    pub paths: AppPaths,
    pub events: Arc<dyn EventSink>,
    pub settings: Arc<dyn SettingsSvc>,
    pub settings_store: Arc<dyn SettingsStore>,
    pub secrets: Arc<dyn SecretStore>,
    pub providers: Arc<dyn ProviderSvc>,
    pub history: Arc<dyn HistorySvc>,
    pub labeling: Arc<dyn LabelingSvc>,
    pub categories: Arc<dyn CategorySvc>,
    pub classification: Arc<dyn ClassificationSvc>,
    pub rules: Arc<dyn RuleSvc>,
    pub usage: Arc<dyn AiUsageSvc>,
    pub logs: Arc<dyn LogReader>,
    pub classifier: Arc<dyn Classifier>,
    pub embedder: Arc<dyn Embedder>,
    pub embeddings: Arc<dyn EmbeddingSvc>,
    pub embed_skips: Arc<EmbedSkips>,
    pub indexer: Arc<dyn ActivityIndexer>,
    pub models: ModelCache,
    pub ai_run: Arc<AiRunState>,
    pub runner: Arc<dyn CategorizationRunner>,
    pub anchor: Arc<ScheduleAnchor>,
    pub schedule: Arc<dyn AutoCategorizeSchedule>,
    pub download_state: Arc<ModelDownloadState>,
    pub downloads: Arc<dyn ModelDownloads>,
    pub tracker: SharedTracker,
}

impl AppCore {
    /// Builds every service. Nothing runs in the background until [`AppCore::start_background`].
    pub fn open(paths: AppPaths, events: Arc<dyn EventSink>) -> Result<Arc<Self>> {
        info!(
            version = env!("CARGO_PKG_VERSION"),
            os = std::env::consts::OS,
            arch = std::env::consts::ARCH,
            data_dir = %paths.data_dir.display(),
            config_dir = %paths.config_dir.display(),
            "Makora started"
        );
        let logs: Arc<dyn LogReader> = Arc::new(JsonlLogReader::new(logging::log_dir(&paths.data_dir)));

        let db_path = paths.database();
        let conn = db::open_sqlite(&db_path)
            .map_err(|err| anyhow::anyhow!("opening database at {}: {err:#}", db_path.display()))?;

        let settings_store: Arc<dyn SettingsStore> =
            Arc::new(TomlSettings::load(paths.config_dir.join("settings.toml")));

        let models = ModelCache::default();
        let embedder: Arc<dyn Embedder> = Arc::new(LlamaEmbedder::new(models.clone()));
        let embeddings: Arc<dyn EmbeddingSvc> = Arc::new(SqliteEmbeddings::new(conn.clone()));
        let embed_skips = Arc::new(EmbedSkips::default());
        let indexer: Arc<dyn ActivityIndexer> = Arc::new(
            BackgroundIndexer::spawn(embeddings.clone(), embedder.clone(), settings_store.clone(), embed_skips.clone())
                .context("spawning activity-embedder thread")?,
        );

        let recorder: Box<dyn SessionRecorder> =
            Box::new(SqliteSessionRecorder::new(conn.clone(), settings_store.clone(), indexer.clone()));
        let exclusions: Box<dyn ExclusionPolicy> = Box::new(SettingsExclusions::new(settings_store.clone()));
        let session_tracker = FocusStateMachine::new(recorder, exclusions).context("initializing session tracker")?;
        let tracker: SharedTracker = Arc::new(Mutex::new(Box::new(session_tracker) as Box<dyn SessionTracker>));
        llm::set_download_root(paths.data_dir.join("models"));

        let history: Arc<dyn HistorySvc> = Arc::new(SqliteHistory::new(conn.clone()));
        let categories: Arc<dyn CategorySvc> = Arc::new(SqliteCategories::new(conn.clone()));
        let classification: Arc<dyn ClassificationSvc> = Arc::new(SqliteClassification::new(conn.clone()));
        let rules: Arc<dyn RuleSvc> = Arc::new(SqliteRules::new(conn.clone()));
        let usage: Arc<dyn AiUsageSvc> = Arc::new(SqliteAiUsage::new(conn.clone()));
        let labeling: Arc<dyn LabelingSvc> = Arc::new(SqliteLabeling::new(conn));
        let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecrets);
        let classifier: Arc<dyn Classifier> = Arc::new(LlmRouter::new(
            LlamaClassifier::new(models.clone()),
            settings_store.clone(),
            secrets.clone(),
        ));
        let providers: Arc<dyn ProviderSvc> = Arc::new(KeychainProviders::new(settings_store.clone(), secrets.clone()));

        let ai_run = Arc::new(AiRunState::default());
        let anchor = Arc::new(ScheduleAnchor::new(paths.data_dir.join("ai-last-run")));
        let runner: Arc<dyn CategorizationRunner> = Arc::new(AiCategorizationRunner {
            state: ai_run.clone(),
            anchor: anchor.clone(),
            events: events.clone(),
            settings: settings_store.clone(),
            categories: categories.clone(),
            labeling: labeling.clone(),
            classification: classification.clone(),
            classifier: classifier.clone(),
            embedder: embedder.clone(),
            embeddings: embeddings.clone(),
            embed_skips: embed_skips.clone(),
            usage: usage.clone(),
        });
        let schedule: Arc<dyn AutoCategorizeSchedule> = Arc::new(ScheduledCategorization {
            anchor: anchor.clone(),
            settings: settings_store.clone(),
            runner: runner.clone(),
            categories: categories.clone(),
            labeling: labeling.clone(),
            events: events.clone(),
        });
        let settings: Arc<dyn SettingsSvc> =
            Arc::new(SettingsService::new(settings_store.clone(), indexer.clone(), schedule.clone()));

        let download_state = Arc::new(ModelDownloadState::default());
        let downloads: Arc<dyn ModelDownloads> =
            Arc::new(OllamaDownloads::new(download_state.clone(), indexer.clone(), events.clone()));

        Ok(Arc::new(Self {
            paths,
            events,
            settings,
            settings_store,
            secrets,
            providers,
            history,
            labeling,
            categories,
            classification,
            rules,
            usage,
            logs,
            classifier,
            embedder,
            embeddings,
            embed_skips,
            indexer,
            models,
            ai_run,
            runner,
            anchor,
            schedule,
            download_state,
            downloads,
            tracker,
        }))
    }

    /// Starts tracking focus and presence, the browser bridge, the embedding backfill and the
    /// categorization schedule. Call it once.
    pub fn start_background(&self) -> Result<()> {
        // Activities recorded before they were embedded as they were saved.
        self.indexer.refresh();

        let (tx, rx) = mpsc::channel::<WatchEvent>();
        tracking::adapters::platform::start_presence_watcher(tx.clone()).context("starting presence watcher")?;
        let mut watcher = PlatformFocusWatcher::new();
        watcher.start(tx).context("starting focus watcher")?;

        let tracker = self.tracker.clone();
        std::thread::Builder::new()
            .name("makora-activity-tracker".into())
            .spawn(move || record_watch_events(&tracker, &rx))
            .context("spawning activity-tracker thread")?;

        tracking::adapters::native_host::register(&self.paths.data_dir);
        tracking::adapters::ingest::spawn(self.tracker.clone());

        self.schedule.clone().start().context("spawning AI scheduler thread")?;
        Ok(())
    }

    /// Closes the session in progress so no time is lost, and flushes the logs.
    pub fn shutdown(&self) {
        info!("Makora shutting down");
        if let Err(err) = lock(&self.tracker).close_current(Utc::now()) {
            error!(error = format!("{err:#}"), "failed to close session on shutdown");
        }
        logging::flush();
    }
}

fn record_watch_events(tracker: &SharedTracker, rx: &mpsc::Receiver<WatchEvent>) {
    let mut next_heartbeat = Instant::now() + HEARTBEAT;
    loop {
        let result = match rx.recv_timeout(next_heartbeat.saturating_duration_since(Instant::now())) {
            Ok(WatchEvent::Focus(event)) => lock(tracker).handle_focus(event.into()),
            Ok(WatchEvent::Unattributed { timestamp }) => lock(tracker).close_current(timestamp),
            Ok(WatchEvent::Presence(event)) => lock(tracker).handle_presence(event.into()),
            Err(RecvTimeoutError::Timeout) => Ok(()),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Err(err) = result {
            error!(error = format!("{err:#}"), "failed to record session");
        }
        if Instant::now() >= next_heartbeat {
            next_heartbeat = Instant::now() + HEARTBEAT;
            if let Err(err) = lock(tracker).heartbeat(Utc::now()) {
                error!(error = format!("{err:#}"), "failed to record heartbeat");
            }
        }
    }
}
