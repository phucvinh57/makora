//! One Makora per user: a second launch asks the running one to show its window, then exits.
//! An exclusive file lock decides which launch runs; the socket only carries the request.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{Listener, ListenerOptions, Name, Stream};
use tracing::warn;

use crate::runtime::commands::{CommandSender, UiCommand};

const SHOW: &str = "show";
const MAX_REQUEST_BYTES: u64 = 64;
const SIGNAL_ATTEMPTS: u32 = 20;
const SIGNAL_RETRY: Duration = Duration::from_millis(100);

static INSTANCE: OnceLock<LocalSocketLock> = OnceLock::new();

type OnShow = Arc<dyn Fn() + Send + Sync>;

pub enum Claim {
    /// This process is the running instance and now listens for later launches.
    Primary,
    /// Another instance was running and has been asked to show itself.
    Secondary,
    /// Neither could be settled; the app runs without the guarantee.
    Unavailable(io::Error),
}

pub trait InstanceLock {
    /// Takes the per-user lock for this process's lifetime; `Ok(false)` when another holds it.
    fn acquire(&self) -> io::Result<bool>;
    /// Asks a running instance to show itself; `Ok(false)` when none is listening.
    fn signal(&self) -> io::Result<bool>;
    /// Becomes the running instance; `on_show` runs for each later launch.
    fn serve(&self, on_show: OnShow) -> io::Result<()>;
}

pub fn claim(commands: CommandSender) -> Claim {
    let lock = match LocalSocketLock::for_current_user() {
        Ok(lock) => lock,
        Err(err) => return Claim::Unavailable(err),
    };
    let claim = claim_with(&lock, commands);
    if matches!(claim, Claim::Primary) {
        let _ = INSTANCE.set(lock);
    }
    claim
}

pub fn claim_with(lock: &dyn InstanceLock, commands: CommandSender) -> Claim {
    match lock.acquire() {
        Ok(true) => match lock.serve(Arc::new(move || {
            let _ = commands.try_send(UiCommand::Show);
        })) {
            Ok(()) => Claim::Primary,
            Err(err) => Claim::Unavailable(err),
        },
        Ok(false) => {
            // The holder may still be starting and not listening yet.
            for _ in 0..SIGNAL_ATTEMPTS {
                if let Ok(true) = lock.signal() {
                    break;
                }
                thread::sleep(SIGNAL_RETRY);
            }
            Claim::Secondary
        }
        Err(err) => Claim::Unavailable(err),
    }
}

pub struct LocalSocketLock {
    name: Name<'static>,
    lock_path: PathBuf,
    private_dir: Option<PathBuf>,
    held: Mutex<Option<File>>,
}

impl LocalSocketLock {
    #[cfg(unix)]
    pub fn for_current_user() -> io::Result<Self> {
        use interprocess::local_socket::{GenericFilePath, ToFsName};

        let dir = makora_lib::tracking::adapters::bridge_wire::socket_dir()?;
        Ok(Self {
            name: dir
                .join("ui.sock")
                .to_fs_name::<GenericFilePath>()?
                .into_owned(),
            lock_path: dir.join("ui.lock"),
            private_dir: Some(dir),
            held: Mutex::new(None),
        })
    }

    #[cfg(windows)]
    pub fn for_current_user() -> io::Result<Self> {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        use makora_lib::paths::APP_ID;
        use makora_lib::tracking::adapters::bridge_wire::user_sid;

        Ok(Self {
            name: format!("{APP_ID}-ui-{}", user_sid()?)
                .to_ns_name::<GenericNamespaced>()?
                .into_owned(),
            lock_path: std::env::temp_dir().join(format!("{APP_ID}-ui.lock")),
            private_dir: None,
            held: Mutex::new(None),
        })
    }

    pub fn new(name: Name<'static>, lock_path: PathBuf) -> Self {
        Self {
            name,
            lock_path,
            private_dir: None,
            held: Mutex::new(None),
        }
    }

    #[cfg(unix)]
    fn create_private_dir(&self) -> io::Result<()> {
        use std::fs::{self, DirBuilder, Permissions};
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

        if let Some(dir) = &self.private_dir {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
            fs::set_permissions(dir, Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    #[cfg(windows)]
    fn create_private_dir(&self) -> io::Result<()> {
        Ok(())
    }

    #[cfg(unix)]
    fn bind(&self) -> io::Result<Listener> {
        // Only the lock holder binds, so a socket left at the name is stale.
        ListenerOptions::new()
            .name(self.name.borrow())
            .try_overwrite(true)
            .create_sync()
    }

    #[cfg(windows)]
    fn bind(&self) -> io::Result<Listener> {
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        use interprocess::os::windows::security_descriptor::SecurityDescriptor;
        use makora_lib::tracking::adapters::bridge_wire::user_sid;
        use widestring::U16CString;

        let sddl = U16CString::from_str(format!("D:P(A;;GA;;;{})", user_sid()?))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        ListenerOptions::new()
            .name(self.name.borrow())
            .security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
            .create_sync()
    }
}

impl InstanceLock for LocalSocketLock {
    fn acquire(&self) -> io::Result<bool> {
        self.create_private_dir()?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock_path)?;
        match file.try_lock() {
            Ok(()) => {
                *self.held.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
                Ok(true)
            }
            Err(TryLockError::WouldBlock) => Ok(false),
            Err(TryLockError::Error(err)) => Err(err),
        }
    }

    fn signal(&self) -> io::Result<bool> {
        match Stream::connect(self.name.borrow()) {
            Ok(mut stream) => {
                stream.write_all(format!("{SHOW}\n").as_bytes())?;
                stream.flush()?;
                Ok(true)
            }
            Err(err) if is_absent(&err) => Ok(false),
            Err(err) => Err(err),
        }
    }

    fn serve(&self, on_show: OnShow) -> io::Result<()> {
        let listener = self.bind()?;
        thread::Builder::new()
            .name("makora-single-instance".into())
            .spawn(move || accept(listener, on_show))?;
        Ok(())
    }
}

fn is_absent(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

/// Each request is read on its own thread, so a client that never writes holds up nobody else.
fn accept(listener: Listener, on_show: OnShow) {
    for connection in listener.incoming() {
        let stream = match connection {
            Ok(stream) => stream,
            Err(err) => {
                warn!(error = %err, "failed to accept a second launch");
                continue;
            }
        };
        let on_show = on_show.clone();
        let spawned = thread::Builder::new()
            .name("makora-second-launch".into())
            .spawn(move || handle(stream, &*on_show));
        if let Err(err) = spawned {
            warn!(error = %err, "failed to handle a second launch");
        }
    }
}

fn handle(stream: Stream, on_show: &(dyn Fn() + Send + Sync)) {
    let mut request = String::new();
    match BufReader::new(stream.take(MAX_REQUEST_BYTES)).read_line(&mut request) {
        Ok(_) if request.trim() == SHOW => on_show(),
        Ok(_) => warn!("ignored an unknown request from a second launch"),
        Err(err) => warn!(error = %err, "failed to read a second launch's request"),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use interprocess::local_socket::{GenericFilePath, ToFsName};

    use super::*;

    fn temp_lock(tag: &str) -> LocalSocketLock {
        let base = std::env::temp_dir().join(format!("makora-{tag}-{}", std::process::id()));
        LocalSocketLock::new(
            base.with_extension("sock")
                .to_fs_name::<GenericFilePath>()
                .unwrap()
                .into_owned(),
            base.with_extension("lock"),
        )
    }

    #[test]
    fn nobody_answers_before_an_instance_serves() {
        assert!(!temp_lock("idle").signal().unwrap());
    }

    #[test]
    fn a_second_launch_reaches_the_first() {
        let lock = temp_lock("pair");
        let (tx, rx) = mpsc::channel();
        lock.serve(Arc::new(move || tx.send(()).unwrap())).unwrap();

        assert!(lock.signal().unwrap());
        rx.recv_timeout(Duration::from_secs(5))
            .expect("the first instance was asked to show");
    }

    #[test]
    fn only_one_launch_holds_the_lock() {
        let first = temp_lock("held");
        let second = temp_lock("held");
        assert!(first.acquire().unwrap());
        assert!(!second.acquire().unwrap());
    }

    #[test]
    fn claim_forwards_show_to_the_ui() {
        let first = temp_lock("claim");
        let second = temp_lock("claim");
        let (commands, rx) = crate::runtime::commands::channel();
        assert!(matches!(
            claim_with(&first, commands.clone()),
            Claim::Primary
        ));
        assert!(matches!(claim_with(&second, commands), Claim::Secondary));
        let received = rx.recv_blocking().unwrap();
        assert_eq!(received, UiCommand::Show);
    }
}
