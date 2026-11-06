//! An AI categorization run in a process of its own, so the models, llama.cpp and the GPU driver
//! it loads are gone the moment the run ends. [`WorkerRunner`] starts `makora --ai-worker` and
//! relays its progress; [`serve`] is that process.

use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::process::{ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::{own_command, read_message, write_message, RunFailure};
use crate::ai::llm::ModelCache;
use crate::app_core::Services;
use crate::categorization::application::ai_run::{AiRunError, AiRunOutcome, AiRunState, AiRunStatus, RunMode, RunRequest};
use crate::categorization::domain::usage::RunTrigger;
use crate::categorization::CategorizationRunner;
use crate::events::{CoreEvent, EventSink};
use crate::paths::AppPaths;
use crate::shared::sync::{lock, RunGuard};

pub const FLAG: &str = "--ai-worker";

/// The first message tells the worker what to run; activity ids would not fit a command line.
#[derive(Debug, Serialize, Deserialize)]
enum ToWorker {
    Start { request: RunRequest, trigger: RunTrigger },
    Cancel,
}

#[derive(Debug, Serialize, Deserialize)]
enum FromWorker {
    Event(CoreEvent),
    Outcome(Result<AiRunOutcome, RunFailure>),
}

/// Runs each categorization in a `makora --ai-worker` child of `program`.
pub struct WorkerRunner {
    program: PathBuf,
    events: Arc<dyn EventSink>,
    running: AtomicBool,
    mode: Mutex<RunMode>,
    done: AtomicUsize,
    total: AtomicUsize,
    /// Asked before the worker could be told; it is told as soon as it starts.
    cancel_asked: AtomicBool,
    stdin: Mutex<Option<ChildStdin>>,
}

impl WorkerRunner {
    pub fn new(program: PathBuf, events: Arc<dyn EventSink>) -> Self {
        Self {
            program,
            events,
            running: AtomicBool::new(false),
            mode: Mutex::new(RunMode::Label),
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            cancel_asked: AtomicBool::new(false),
            stdin: Mutex::new(None),
        }
    }

    /// Relays the worker's events; `finished` is set once `CategorizeFinished` has gone out.
    fn run_child(&self, request: &RunRequest, trigger: RunTrigger, finished: &mut bool) -> Result<AiRunOutcome, AiRunError> {
        let mut command = own_command(&self.program);
        command.arg(FLAG);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("starting the AI worker")?;
        let mut stdin = child.stdin.take();
        if let Some(stdin) = stdin.as_mut() {
            let start = ToWorker::Start {
                request: request.clone(),
                trigger,
            };
            if let Err(err) = write_message(stdin, &start) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow::Error::from(err).context("telling the AI worker what to run").into());
            }
        }
        *lock(&self.stdin) = stdin;
        if self.cancel_asked.load(Ordering::Acquire) {
            self.cancel();
        }
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("the AI worker has no stdout").into());
        };
        let mut stdout = BufReader::new(stdout);

        let mut outcome = None;
        loop {
            match read_message::<FromWorker>(&mut stdout) {
                Ok(Some(FromWorker::Event(event))) => {
                    match event {
                        CoreEvent::CategorizeProgress { done, total } => {
                            self.done.store(done, Ordering::Release);
                            self.total.store(total, Ordering::Release);
                        }
                        CoreEvent::CategorizeFinished => *finished = true,
                        _ => {}
                    }
                    self.events.emit(event);
                }
                Ok(Some(FromWorker::Outcome(result))) => outcome = Some(result),
                Ok(None) => break,
                Err(err) => {
                    warn!(error = %err, "unreadable message from the AI worker; stopping it");
                    let _ = child.kill();
                    break;
                }
            }
        }
        lock(&self.stdin).take();
        drop(stdout);
        let status = child.wait().context("waiting for the AI worker")?;
        match outcome {
            Some(result) => result.map_err(AiRunError::from),
            None => Err(anyhow!("the AI worker stopped without an answer ({status})").into()),
        }
    }
}

impl CategorizationRunner for WorkerRunner {
    fn run(&self, request: &RunRequest, trigger: RunTrigger) -> Result<AiRunOutcome, AiRunError> {
        let _running = RunGuard::start(&self.running).ok_or(AiRunError::AlreadyRunning)?;
        self.cancel_asked.store(false, Ordering::Release);
        *lock(&self.mode) = request.mode;
        self.done.store(0, Ordering::Release);
        self.total.store(0, Ordering::Release);
        let mut finished = false;
        let outcome = self.run_child(request, trigger, &mut finished);
        if !finished {
            self.events.emit(CoreEvent::CategorizeFinished);
        }
        outcome
    }

    fn status(&self) -> AiRunStatus {
        AiRunStatus {
            running: self.running.load(Ordering::Acquire),
            mode: *lock(&self.mode),
            done: self.done.load(Ordering::Acquire),
            total: self.total.load(Ordering::Acquire),
        }
    }

    fn cancel(&self) {
        self.cancel_asked.store(true, Ordering::Release);
        if let Some(stdin) = lock(&self.stdin).as_mut() {
            if let Err(err) = write_message(stdin, &ToWorker::Cancel) {
                warn!(error = %err, "could not ask the AI worker to stop");
            }
        }
    }
}

/// Whether `args` (without the program) are a worker's command line.
pub fn parse_args(args: &[String]) -> bool {
    args.first().is_some_and(|flag| flag == FLAG)
}

struct StdoutEvents(Arc<Mutex<BufWriter<io::Stdout>>>);

impl EventSink for StdoutEvents {
    fn emit(&self, event: CoreEvent) {
        if let Err(err) = write_message(&mut *lock(&self.0), &FromWorker::Event(event)) {
            warn!(error = %err, "could not report AI progress");
        }
    }
}

/// Runs the categorization the first message on stdin asks for in this process and reports it on
/// stdout. A `Cancel` on stdin, or stdin closing because the app is gone, stops the run after
/// the activity in progress.
pub fn serve(paths: AppPaths) -> Result<()> {
    let (request, trigger) = match read_message::<ToWorker>(&mut io::stdin().lock()) {
        Ok(Some(ToWorker::Start { request, trigger })) => (request, trigger),
        Ok(Some(ToWorker::Cancel) | None) => return Ok(()),
        Err(err) => return Err(anyhow::Error::from(err).context("reading what the AI worker should run")),
    };
    let stdout = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
    let services = Services::open(paths, Arc::new(StdoutEvents(stdout.clone())))?;
    let state = Arc::new(AiRunState::default());
    let runner = services.local_runner(state.clone(), ModelCache::default());

    let stopped = Arc::new(AtomicBool::new(false));
    let (cancel, stop) = (state.clone(), stopped.clone());
    std::thread::Builder::new()
        .name("makora-worker-stdin".into())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            loop {
                let message = read_message::<ToWorker>(&mut stdin);
                stop.store(true, Ordering::Release);
                cancel.cancel();
                if !matches!(message, Ok(Some(ToWorker::Cancel))) {
                    return;
                }
            }
        })
        .context("spawning the worker's stdin thread")?;

    // A stop that came while the services opened would be cleared by the run's own reset.
    let outcome = if stopped.load(Ordering::Acquire) {
        Err(RunFailure::Failed("stopped before it started".into()))
    } else {
        runner.run(&request, trigger).map_err(RunFailure::from)
    };
    write_message(&mut *lock(&stdout), &FromWorker::Outcome(outcome)).context("reporting the AI run")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::ActivityId;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_a_worker_command_line() {
        assert!(!parse_args(&args(&[])));
        assert!(!parse_args(&args(&["--window"])));
        assert!(parse_args(&args(&[FLAG])));
    }

    #[test]
    fn the_start_message_carries_a_scoped_request() {
        let request = RunRequest::scoped(RunMode::Recheck, vec![ActivityId(3), ActivityId(9)]);
        let mut bytes = Vec::new();
        write_message(&mut bytes, &ToWorker::Start {
            request: request.clone(),
            trigger: RunTrigger::Scheduled,
        })
        .unwrap();
        let Some(ToWorker::Start { request: read, trigger }) = read_message(&mut bytes.as_slice()).unwrap() else {
            panic!("expected a start message");
        };
        assert_eq!(read, request);
        assert_eq!(trigger, RunTrigger::Scheduled);
    }

    #[test]
    fn run_failures_cross_the_wire() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &FromWorker::Outcome(Err(AiRunError::NoCategories.into()))).unwrap();
        let Some(FromWorker::Outcome(Err(failure))) = read_message(&mut bytes.as_slice()).unwrap() else {
            panic!("expected a failed outcome");
        };
        assert!(matches!(AiRunError::from(failure), AiRunError::NoCategories));
    }
}
