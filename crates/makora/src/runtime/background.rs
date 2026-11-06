//! The background process: tracking, the tray, notifications and the AI schedule, with no UI.
//! The window is a process of its own, started when it is asked for, so closing it gives back
//! everything the UI held: GPUI, fonts and the GPU driver.

use std::io::BufReader;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_channel::Receiver;
use makora_lib::app_core::AppCore;
use makora_lib::events::{CoreEvent, NavTarget};
use makora_lib::ipc::window::{self as wire, answer, FromWindow, ToWindow};
use makora_lib::ipc::{own_command, own_program, read_message, write_message};
use makora_lib::shared::sync::lock;
use makora_lib::tracking::adapters::own_process;
use tracing::{error, info, warn};

use crate::runtime::commands::{CommandReceiver, CommandSender, UiCommand};
use crate::runtime::main_loop;

/// How long a window gets to save its preferences and exit when the app quits.
const WINDOW_EXIT_GRACE: Duration = Duration::from_secs(3);

struct WindowProcess {
    child: Child,
    stdin: ChildStdin,
}

/// The window process, while one is running.
#[derive(Clone, Default)]
struct WindowSlot(Arc<Mutex<Option<WindowProcess>>>);

impl WindowSlot {
    /// Sends `message` to the window, or to the window with `pid` only. `false` when there is no
    /// such window or it cannot be reached.
    fn send(&self, pid: Option<u32>, message: &ToWindow) -> bool {
        let mut slot = lock(&self.0);
        let Some(window) = slot.as_mut().filter(|window| pid.is_none_or(|pid| window.child.id() == pid)) else {
            return false;
        };
        write_message(&mut window.stdin, message).is_ok()
    }

    fn show(&self, target: Option<NavTarget>, core: &Arc<AppCore>, commands: &CommandSender) {
        if let Some(pid) = lock(&self.0).as_ref().map(|window| window.child.id()) {
            allow_window_to_take_focus(pid);
        }
        if self.send(None, &ToWindow::Show(target)) {
            return;
        }
        let mut slot = lock(&self.0);
        if let Some(mut stale) = slot.take() {
            own_process::set_window_process(None);
            let _ = stale.child.kill();
            let _ = stale.child.wait();
        }
        match self.spawn(target, core, commands) {
            Ok(window) => *slot = Some(window),
            Err(err) => error!(error = format!("{err:#}"), "could not open the window"),
        }
    }

    fn spawn(&self, target: Option<NavTarget>, core: &Arc<AppCore>, commands: &CommandSender) -> Result<WindowProcess> {
        let mut command = own_command(&own_program().context("finding the running executable")?);
        command.arg(wire::FLAG);
        if target.is_some() {
            command.arg(wire::REVIEW_FLAG);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("starting the window process")?;
        let stdin = child.stdin.take().context("the window process has no stdin")?;
        let stdout = child.stdout.take().context("the window process has no stdout")?;
        let pid = child.id();
        allow_window_to_take_focus(pid);
        own_process::set_window_process(Some(pid));
        info!(pid, "window opened");

        let (slot, core, commands) = (self.clone(), core.clone(), commands.clone());
        std::thread::Builder::new()
            .name("makora-window-link".into())
            .spawn(move || slot.serve(pid, stdout, &core, &commands))
            .context("spawning the window link thread")?;
        Ok(WindowProcess { child, stdin })
    }

    /// Answers the window with `pid` until it closes, then forgets it.
    fn serve(&self, pid: u32, stdout: ChildStdout, core: &Arc<AppCore>, commands: &CommandSender) {
        let mut stdout = BufReader::new(stdout);
        loop {
            match read_message::<FromWindow>(&mut stdout) {
                Ok(Some(FromWindow::Call { id, call })) if call.blocks() => {
                    let unanswered = call.failed("the app could not start the job");
                    let (slot, core) = (self.clone(), core.clone());
                    let spawned = std::thread::Builder::new()
                        .name("makora-window-call".into())
                        .spawn(move || {
                            let reply = answer(&core, call);
                            slot.send(Some(pid), &ToWindow::Reply { id, reply });
                        });
                    if let Err(err) = spawned {
                        error!(error = %err, "could not answer the window");
                        self.send(Some(pid), &ToWindow::Reply { id, reply: unanswered });
                    }
                }
                Ok(Some(FromWindow::Call { id, call })) => {
                    let reply = answer(core, call);
                    self.send(Some(pid), &ToWindow::Reply { id, reply });
                }
                Ok(Some(FromWindow::SettingsSaved)) => core.services.settings_store.reload(),
                Ok(Some(FromWindow::QuitApp)) => {
                    let _ = commands.try_send(UiCommand::Quit);
                }
                Ok(None) => break,
                Err(err) => {
                    warn!(error = %err, "unreadable message from the window");
                    break;
                }
            }
        }
        let closed = {
            let mut slot = lock(&self.0);
            if slot.as_ref().is_some_and(|window| window.child.id() == pid) {
                own_process::set_window_process(None);
                slot.take()
            } else {
                None
            }
        };
        if let Some(mut window) = closed {
            drop(window.stdin);
            let _ = window.child.wait();
            info!(pid, "window closed");
        }
    }

    /// Asks the window to quit, and stops it if it has not within [`WINDOW_EXIT_GRACE`].
    fn close(&self) {
        let Some(WindowProcess { mut child, mut stdin }) = lock(&self.0).take() else {
            return;
        };
        own_process::set_window_process(None);
        let _ = write_message(&mut stdin, &ToWindow::Quit);
        drop(stdin);
        let deadline = Instant::now() + WINDOW_EXIT_GRACE;
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        warn!("the window did not quit in time; stopping it");
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Sends the core's events to the window, and opens it where a notification click asks.
fn forward_events(events: Receiver<CoreEvent>, window: WindowSlot, commands: CommandSender) -> Result<()> {
    std::thread::Builder::new()
        .name("makora-events".into())
        .spawn(move || {
            while let Ok(event) = events.recv_blocking() {
                match event {
                    CoreEvent::Navigate(target) => {
                        let _ = commands.try_send(UiCommand::ShowAt(target));
                    }
                    event => {
                        window.send(None, &ToWindow::Event(event));
                    }
                }
            }
        })
        .context("spawning the events thread")?;
    Ok(())
}

/// Lets the window process `pid` bring its window to the front: Windows only lets the foreground
/// process, or one it allows, do that.
#[cfg(windows)]
fn allow_window_to_take_focus(pid: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    // SAFETY: takes no pointers; a refusal only means the window may open behind others.
    unsafe {
        AllowSetForegroundWindow(pid);
    }
}

#[cfg(not(windows))]
fn allow_window_to_take_focus(_pid: u32) {}

/// Runs until Quit, from the tray or the window, then closes the window and the session.
pub fn run(core: Arc<AppCore>, commands: CommandSender, command_rx: CommandReceiver, events: Receiver<CoreEvent>) {
    main_loop::run(commands.clone(), move || serve(core, commands, command_rx, events));
}

fn serve(core: Arc<AppCore>, commands: CommandSender, command_rx: CommandReceiver, events: Receiver<CoreEvent>) {
    let window = WindowSlot::default();
    if let Err(err) = forward_events(events, window.clone(), commands.clone()) {
        error!(error = format!("{err:#}"), "the window will not hear from the core");
    }
    window.show(None, &core, &commands);
    while let Ok(command) = command_rx.recv_blocking() {
        match command {
            UiCommand::Show => window.show(None, &core, &commands),
            UiCommand::ShowAt(target) => window.show(Some(target), &core, &commands),
            UiCommand::Quit => break,
        }
    }
    window.close();
    core.runner.cancel();
    core.shutdown();
}
