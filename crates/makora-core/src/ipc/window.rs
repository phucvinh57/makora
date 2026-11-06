//! The window process and the background process that starts it. The window reads and writes the
//! database and the settings file itself; AI runs and model downloads belong to the background
//! process, so the window asks for them with a [`Call`] and is told how they go with events.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::{read_message, write_message, RunFailure};
use crate::ai::llm;
use crate::ai::model_download::{ModelDownloadError, ModelDownloadStatus, ModelDownloads};
use crate::app_core::AppCore;
use crate::categorization::application::ai_run::{AiRunError, AiRunOutcome, AiRunStatus};
use crate::categorization::domain::usage::RunTrigger;
use crate::categorization::CategorizationRunner;
use crate::events::{CoreEvent, NavTarget};
use crate::shared::sync::lock;

pub const FLAG: &str = "--window";
pub const REVIEW_FLAG: &str = "--review";

#[derive(Debug, Serialize, Deserialize)]
pub enum ToWindow {
    Show(Option<NavTarget>),
    Quit,
    Event(CoreEvent),
    Reply { id: u64, reply: Reply },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FromWindow {
    Call { id: u64, call: Call },
    SettingsSaved,
    QuitApp,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Call {
    Run { recheck: bool },
    CancelRun,
    RunStatus,
    Download { model: String },
    CancelDownload,
    DownloadStatus,
}

impl Call {
    /// Whether answering takes as long as the job, so it needs a thread of its own.
    pub fn blocks(&self) -> bool {
        matches!(self, Self::Run { .. } | Self::Download { .. })
    }

    /// The answer for a call that could not be carried out.
    pub fn failed(&self, message: &str) -> Reply {
        match self {
            Self::Run { .. } => Reply::Run(Err(RunFailure::Failed(message.to_string()))),
            Self::Download { .. } => Reply::Download(Err(DownloadFailure::Failed(message.to_string()))),
            Self::CancelRun | Self::RunStatus | Self::CancelDownload | Self::DownloadStatus => Reply::Done,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Reply {
    Run(Result<AiRunOutcome, RunFailure>),
    RunStatus(AiRunStatus),
    Download(Result<bool, DownloadFailure>),
    DownloadStatus(ModelDownloadStatus),
    Done,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DownloadFailure {
    AlreadyRunning,
    Failed(String),
}

impl From<ModelDownloadError> for DownloadFailure {
    fn from(err: ModelDownloadError) -> Self {
        match err {
            ModelDownloadError::AlreadyRunning => Self::AlreadyRunning,
            ModelDownloadError::Failed(err) => Self::Failed(format!("{err:#}")),
        }
    }
}

impl From<DownloadFailure> for ModelDownloadError {
    fn from(failure: DownloadFailure) -> Self {
        match failure {
            DownloadFailure::AlreadyRunning => Self::AlreadyRunning,
            DownloadFailure::Failed(message) => Self::Failed(anyhow!(message)),
        }
    }
}

/// The window's command line: `Some(target)` to open at, when `args` (without the program) are a
/// window process's.
pub fn parse_args(args: &[String]) -> Option<Option<NavTarget>> {
    let (flag, rest) = args.split_first()?;
    (flag == FLAG).then(|| rest.iter().any(|arg| arg == REVIEW_FLAG).then_some(NavTarget::CategoriesReview))
}

/// What the background process answers a window's call with.
pub fn answer(core: &AppCore, call: Call) -> Reply {
    match call {
        Call::Run { recheck } => Reply::Run(core.runner.run(recheck, RunTrigger::Manual).map_err(RunFailure::from)),
        Call::CancelRun => {
            core.runner.cancel();
            Reply::Done
        }
        Call::RunStatus => Reply::RunStatus(core.runner.status()),
        Call::Download { model } => Reply::Download(core.downloads.download(&model).map_err(DownloadFailure::from)),
        Call::CancelDownload => {
            core.downloads.cancel();
            Reply::Done
        }
        Call::DownloadStatus => Reply::DownloadStatus(core.downloads.status()),
    }
}

/// The window's end: calls to the background process over stdout, its messages from stdin.
pub struct WindowClient {
    out: Mutex<Box<dyn Write + Send>>,
    next_id: AtomicU64,
    /// `None` once the background process is gone.
    pending: Mutex<Option<HashMap<u64, mpsc::Sender<Reply>>>>,
}

impl WindowClient {
    pub fn new(out: impl Write + Send + 'static) -> Arc<Self> {
        Arc::new(Self {
            out: Mutex::new(Box::new(out)),
            next_id: AtomicU64::new(0),
            pending: Mutex::new(Some(HashMap::new())),
        })
    }

    fn send(&self, message: &FromWindow) -> io::Result<()> {
        write_message(&mut *lock(&self.out), message)
    }

    /// Blocks until the background process answers, or is gone.
    pub fn call(&self, call: Call) -> io::Result<Reply> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        match lock(&self.pending).as_mut() {
            Some(pending) => pending.insert(id, tx),
            None => return Err(gone()),
        };
        if let Err(err) = self.send(&FromWindow::Call { id, call }) {
            if let Some(pending) = lock(&self.pending).as_mut() {
                pending.remove(&id);
            }
            return Err(err);
        }
        rx.recv().map_err(|_| gone())
    }

    pub fn settings_saved(&self) {
        if let Err(err) = self.send(&FromWindow::SettingsSaved) {
            warn!(error = %err, "could not tell the app the settings changed");
        }
    }

    pub fn quit_app(&self) {
        if let Err(err) = self.send(&FromWindow::QuitApp) {
            warn!(error = %err, "could not ask the app to quit");
        }
    }

    /// Reads the background process's messages until it is gone: each reply goes to its call,
    /// anything else to `on_message`.
    pub fn serve(&self, mut reader: impl Read, mut on_message: impl FnMut(ToWindow)) {
        loop {
            match read_message::<ToWindow>(&mut reader) {
                Ok(Some(ToWindow::Reply { id, reply })) => {
                    let waiter = lock(&self.pending).as_mut().and_then(|pending| pending.remove(&id));
                    if let Some(waiter) = waiter {
                        let _ = waiter.send(reply);
                    }
                }
                Ok(Some(message)) => on_message(message),
                Ok(None) => break,
                Err(err) => {
                    warn!(error = %err, "unreadable message from the app");
                    break;
                }
            }
        }
        lock(&self.pending).take();
    }
}

fn gone() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the Makora background process is gone")
}

pub struct RemoteRunner(pub Arc<WindowClient>);

impl CategorizationRunner for RemoteRunner {
    fn run(&self, recheck: bool, _trigger: RunTrigger) -> Result<AiRunOutcome, AiRunError> {
        match self.0.call(Call::Run { recheck }).map_err(anyhow::Error::from)? {
            Reply::Run(result) => result.map_err(AiRunError::from),
            other => Err(anyhow!("unexpected answer {other:?}").into()),
        }
    }

    fn status(&self) -> AiRunStatus {
        match self.0.call(Call::RunStatus) {
            Ok(Reply::RunStatus(status)) => status,
            _ => AiRunStatus {
                running: false,
                recheck: false,
                done: 0,
                total: 0,
            },
        }
    }

    fn cancel(&self) {
        let _ = self.0.call(Call::CancelRun);
    }
}

pub struct RemoteDownloads(pub Arc<WindowClient>);

impl ModelDownloads for RemoteDownloads {
    fn status(&self) -> ModelDownloadStatus {
        match self.0.call(Call::DownloadStatus) {
            Ok(Reply::DownloadStatus(status)) => status,
            _ => ModelDownloadStatus {
                running: false,
                model: String::new(),
                downloaded: 0,
                total: 0,
            },
        }
    }

    fn cancel(&self) {
        let _ = self.0.call(Call::CancelDownload);
    }

    fn download(&self, model: &str) -> Result<bool, ModelDownloadError> {
        let call = Call::Download {
            model: model.to_string(),
        };
        match self.0.call(call).map_err(anyhow::Error::from)? {
            Reply::Download(result) => result.map_err(ModelDownloadError::from),
            other => Err(anyhow!("unexpected answer {other:?}").into()),
        }
    }

    fn installed(&self) -> Vec<llm::InstalledModel> {
        llm::installed_models()
    }

    fn is_installed(&self, model: &str) -> bool {
        llm::resolve_model(model.trim()).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;
    use std::thread;

    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_a_window_command_line() {
        assert_eq!(parse_args(&args(&[])), None);
        assert_eq!(parse_args(&args(&["--ai-worker"])), None);
        assert_eq!(parse_args(&args(&[FLAG])), Some(None));
        assert_eq!(parse_args(&args(&[FLAG, REVIEW_FLAG])), Some(Some(NavTarget::CategoriesReview)));
    }

    /// A pipe end that hands each written frame to the test.
    struct Frames(mpsc::Sender<Vec<u8>>);

    impl Write for Frames {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.send(buf.to_vec()).map_err(|_| gone())?;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_call_gets_its_reply_and_other_messages_pass_through() {
        let (written_tx, written) = mpsc::channel();
        let client = WindowClient::new(Frames(written_tx));
        let caller = {
            let client = client.clone();
            thread::spawn(move || client.call(Call::RunStatus))
        };

        let sent: Vec<u8> = written.recv().unwrap().into_iter().chain(written.recv().unwrap()).collect();
        let Some(FromWindow::Call { id, call: Call::RunStatus }) = read_message(&mut sent.as_slice()).unwrap() else {
            panic!("expected a RunStatus call");
        };

        let mut incoming = Vec::new();
        write_message(&mut incoming, &ToWindow::Show(None)).unwrap();
        let status = AiRunStatus {
            running: true,
            recheck: false,
            done: 3,
            total: 7,
        };
        write_message(&mut incoming, &ToWindow::Reply {
            id,
            reply: Reply::RunStatus(status),
        })
        .unwrap();
        let mut shown = 0;
        client.serve(BufReader::new(incoming.as_slice()), |message| {
            assert!(matches!(message, ToWindow::Show(None)));
            shown += 1;
        });

        assert_eq!(shown, 1);
        let Ok(Reply::RunStatus(status)) = caller.join().unwrap() else {
            panic!("expected the status");
        };
        assert_eq!((status.done, status.total), (3, 7));
        assert!(client.call(Call::RunStatus).is_err(), "calls fail once the app is gone");
    }
}
