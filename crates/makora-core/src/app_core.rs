//! The composition root. [`Services`] opens the database and settings and wires each context's
//! adapters to its ports, with no background work, so any process can open it. [`AppCore`] adds
//! the tracker, the AI runner and schedule and the model downloads, and starts the background
//! work; [`ClientCore`] is what a window calls.

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
use crate::categorization::{
    AiCategorizationRunner, AiUsageSvc, AutoCategorizeSchedule, CategorizationRunner, ClassificationSvc, EmbedSkips,
    EmbeddingSvc, RuleSvc, ScheduleAnchor, ScheduledCategorization, SqliteClassification, SqliteEmbeddings,
    SqliteRules,
};
use crate::db::{self, Db};
use crate::domain::{HistorySvc, LabelingSvc, SqliteHistory, SqliteLabeling};
use crate::events::EventSink;
#[cfg(target_os = "linux")]
use crate::ipc::worker::WorkerRunner;
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

/// The services over the database, the settings file and the keychain. Opening them starts no
/// thread and leaves the tracker's open session alone.
pub struct Services {
    pub paths: AppPaths,
    pub events: Arc<dyn EventSink>,
    pub(crate) db: Db,
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
    pub embeddings: Arc<dyn EmbeddingSvc>,
    pub anchor: Arc<ScheduleAnchor>,
}

impl Services {
    pub fn open(paths: AppPaths, events: Arc<dyn EventSink>) -> Result<Self> {
        let logs: Arc<dyn LogReader> = Arc::new(JsonlLogReader::new(logging::log_dir(&paths.data_dir)));

        let db_path = paths.database();
        let db = db::open_sqlite(&db_path)
            .map_err(|err| anyhow::anyhow!("opening database at {}: {err:#}", db_path.display()))?;

        let settings_store: Arc<dyn SettingsStore> = Arc::new(TomlSettings::load(paths.config_dir.join("settings.toml")));
        llm::set_download_root(paths.data_dir.join("models"));

        let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecrets);
        let providers: Arc<dyn ProviderSvc> = Arc::new(KeychainProviders::new(settings_store.clone(), secrets.clone()));
        let anchor = Arc::new(ScheduleAnchor::new(paths.data_dir.join("ai-last-run")));

        Ok(Self {
            history: Arc::new(SqliteHistory::new(db.clone())),
            labeling: Arc::new(SqliteLabeling::new(db.clone())),
            categories: Arc::new(SqliteCategories::new(db.clone())),
            classification: Arc::new(SqliteClassification::new(db.clone())),
            rules: Arc::new(SqliteRules::new(db.clone())),
            usage: Arc::new(SqliteAiUsage::new(db.clone())),
            embeddings: Arc::new(SqliteEmbeddings::new(db.clone())),
            paths,
            events,
            db,
            settings_store,
            secrets,
            providers,
            logs,
            anchor,
        })
    }

    /// Runs AI categorization in this process, loading the models into `models`.
    pub fn local_runner(&self, state: Arc<AiRunState>, models: ModelCache) -> Arc<dyn CategorizationRunner> {
        Arc::new(AiCategorizationRunner {
            state,
            anchor: self.anchor.clone(),
            events: self.events.clone(),
            settings: self.settings_store.clone(),
            categories: self.categories.clone(),
            labeling: self.labeling.clone(),
            classification: self.classification.clone(),
            classifier: Arc::new(LlmRouter::new(
                LlamaClassifier::new(models.clone()),
                self.settings_store.clone(),
                self.secrets.clone(),
            )),
            embedder: Arc::new(LlamaEmbedder::new(models)),
            embeddings: self.embeddings.clone(),
            embed_skips: Arc::new(EmbedSkips::default()),
            usage: self.usage.clone(),
        })
    }

    pub fn schedule(&self, runner: Arc<dyn CategorizationRunner>) -> Arc<dyn AutoCategorizeSchedule> {
        Arc::new(ScheduledCategorization {
            anchor: self.anchor.clone(),
            settings: self.settings_store.clone(),
            runner,
            categories: self.categories.clone(),
            labeling: self.labeling.clone(),
            events: self.events.clone(),
        })
    }

    pub fn settings_svc(&self, schedule: Arc<dyn AutoCategorizeSchedule>) -> Arc<dyn SettingsSvc> {
        Arc::new(SettingsService::new(self.settings_store.clone(), schedule, self.labeling.clone()))
    }
}

/// What a window calls: the services it reads and writes, and the AI runs and model downloads,
/// which may happen in another process.
pub struct ClientCore {
    pub paths: AppPaths,
    pub settings: Arc<dyn SettingsSvc>,
    pub providers: Arc<dyn ProviderSvc>,
    pub history: Arc<dyn HistorySvc>,
    pub labeling: Arc<dyn LabelingSvc>,
    pub categories: Arc<dyn CategorySvc>,
    pub classification: Arc<dyn ClassificationSvc>,
    pub rules: Arc<dyn RuleSvc>,
    pub usage: Arc<dyn AiUsageSvc>,
    pub logs: Arc<dyn LogReader>,
    pub runner: Arc<dyn CategorizationRunner>,
    pub schedule: Arc<dyn AutoCategorizeSchedule>,
    pub downloads: Arc<dyn ModelDownloads>,
    /// The models loaded in this process, when AI runs here.
    pub models: Option<ModelCache>,
}

impl ClientCore {
    pub fn new(
        services: &Services,
        runner: Arc<dyn CategorizationRunner>,
        downloads: Arc<dyn ModelDownloads>,
        models: Option<ModelCache>,
    ) -> Self {
        let schedule = services.schedule(runner.clone());
        Self {
            paths: services.paths.clone(),
            settings: services.settings_svc(schedule.clone()),
            providers: services.providers.clone(),
            history: services.history.clone(),
            labeling: services.labeling.clone(),
            categories: services.categories.clone(),
            classification: services.classification.clone(),
            rules: services.rules.clone(),
            usage: services.usage.clone(),
            logs: services.logs.clone(),
            runner,
            schedule,
            downloads,
            models,
        }
    }
}

pub struct AppCore {
    pub services: Services,
    pub settings: Arc<dyn SettingsSvc>,
    pub models: Option<ModelCache>,
    pub runner: Arc<dyn CategorizationRunner>,
    pub schedule: Arc<dyn AutoCategorizeSchedule>,
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
        let services = Services::open(paths, events)?;

        let recorder: Box<dyn SessionRecorder> =
            Box::new(SqliteSessionRecorder::new(services.db.clone(), services.settings_store.clone()));
        let exclusions: Box<dyn ExclusionPolicy> = Box::new(SettingsExclusions::new(services.settings_store.clone()));
        let session_tracker = FocusStateMachine::new(recorder, exclusions).context("initializing session tracker")?;
        let tracker: SharedTracker = Arc::new(Mutex::new(Box::new(session_tracker) as Box<dyn SessionTracker>));

        let (runner, models) = ai_runner(&services);
        let schedule = services.schedule(runner.clone());
        let settings = services.settings_svc(schedule.clone());
        let downloads: Arc<dyn ModelDownloads> =
            Arc::new(OllamaDownloads::new(Arc::new(ModelDownloadState::default()), services.events.clone()));

        Ok(Arc::new(Self {
            services,
            settings,
            models,
            runner,
            schedule,
            downloads,
            tracker,
        }))
    }

    /// The services a window in this process calls.
    pub fn client(&self) -> ClientCore {
        ClientCore::new(&self.services, self.runner.clone(), self.downloads.clone(), self.models.clone())
    }

    /// Starts tracking focus and presence, the browser bridge and the categorization schedule.
    /// Call it once.
    pub fn start_background(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel::<WatchEvent>();
        tracking::adapters::platform::start_presence_watcher(tx.clone()).context("starting presence watcher")?;
        let mut watcher = PlatformFocusWatcher::new();
        watcher.start(tx).context("starting focus watcher")?;

        let tracker = self.tracker.clone();
        std::thread::Builder::new()
            .name("makora-activity-tracker".into())
            .spawn(move || record_watch_events(&tracker, &rx))
            .context("spawning activity-tracker thread")?;

        tracking::adapters::native_host::register(&self.services.paths.data_dir);
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

/// On Linux each run is a worker process, so no model stays loaded here; elsewhere runs share this
/// process with the window.
#[cfg(target_os = "linux")]
fn ai_runner(services: &Services) -> (Arc<dyn CategorizationRunner>, Option<ModelCache>) {
    // This very binary, even after a package upgrade replaced the file on disk.
    let program = std::path::PathBuf::from("/proc/self/exe");
    (Arc::new(WorkerRunner::new(program, services.events.clone())), None)
}

#[cfg(not(target_os = "linux"))]
fn ai_runner(services: &Services) -> (Arc<dyn CategorizationRunner>, Option<ModelCache>) {
    let models = ModelCache::default();
    (services.local_runner(Arc::new(AiRunState::default()), models.clone()), Some(models))
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
