use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use tracing::{info, warn};
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::MatchRule;

use crate::core::model::{PresenceEvent, WatchEvent};

const LOGIN_SERVICE: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";
const USER_INTERFACE: &str = "org.freedesktop.login1.User";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const RETRY_DELAY: Duration = Duration::from_secs(5);

pub fn start_presence_watcher(tx: Sender<WatchEvent>) -> Result<()> {
    let mut state = PresenceState::default();
    let initial = match WatchConnection::connect() {
        Ok(watch) => {
            info!("presence watcher using systemd-logind");
            if !state.apply_snapshot(watch.locked, watch.sleeping, &tx) {
                return Ok(());
            }
            Some(watch)
        }
        Err(err) => {
            warn!(
                error = format!("{err:#}"),
                "systemd-logind presence watcher unavailable; retrying"
            );
            None
        }
    };

    thread::Builder::new()
        .name("makora-presence-watcher".into())
        .spawn(move || run(tx, state, initial))
        .context("spawning presence-watcher thread")?;
    Ok(())
}

fn run(tx: Sender<WatchEvent>, mut state: PresenceState, mut initial: Option<WatchConnection>) {
    loop {
        let watch = match initial.take() {
            Some(watch) => watch,
            None => match WatchConnection::connect() {
                Ok(watch) => {
                    info!("systemd-logind presence watcher reconnected");
                    if !state.apply_snapshot(watch.locked, watch.sleeping, &tx) {
                        return;
                    }
                    watch
                }
                Err(err) => {
                    warn!(
                        error = format!("{err:#}"),
                        "systemd-logind presence watcher failed to connect; retrying"
                    );
                    thread::sleep(RETRY_DELAY);
                    continue;
                }
            },
        };

        if let Err(err) = watch_messages(&watch, &mut state, &tx) {
            warn!(
                error = format!("{err:#}"),
                "systemd-logind presence watcher dropped; retrying"
            );
        }
        thread::sleep(RETRY_DELAY);
    }
}

struct WatchConnection {
    connection: Connection,
    session_path: OwnedObjectPath,
    locked: bool,
    sleeping: bool,
}

impl WatchConnection {
    fn connect() -> Result<Self> {
        let connection = Connection::system().context("connecting to the system bus")?;
        let manager = Proxy::new(&connection, LOGIN_SERVICE, MANAGER_PATH, MANAGER_INTERFACE)
            .context("building the logind manager proxy")?;
        let session_path = resolve_graphical_session(&connection, &manager)?;

        let dbus = DBusProxy::new(&connection).context("opening the system bus proxy")?;
        dbus.add_match_rule(locked_hint_rule(session_path.as_str())?)
            .context("subscribing to LockedHint changes")?;
        dbus.add_match_rule(prepare_for_sleep_rule()?)
            .context("subscribing to PrepareForSleep")?;

        let locked = {
            let session = Proxy::new(
                &connection,
                LOGIN_SERVICE,
                session_path.as_str(),
                SESSION_INTERFACE,
            )
            .context("building the logind session proxy")?;
            session
                .get_property::<bool>("LockedHint")
                .context("reading LockedHint")?
        };
        let sleeping = manager
            .get_property::<bool>("PreparingForSleep")
            .context("reading PreparingForSleep")?;

        Ok(Self {
            connection,
            session_path,
            locked,
            sleeping,
        })
    }
}

fn resolve_graphical_session(
    connection: &Connection,
    manager: &Proxy<'_>,
) -> Result<OwnedObjectPath> {
    if let Ok(path) = manager.call::<_, _, OwnedObjectPath>("GetSessionByPID", &std::process::id())
    {
        let (class, can_lock) = {
            let session = Proxy::new(connection, LOGIN_SERVICE, path.as_str(), SESSION_INTERFACE)
                .context("validating this process's login session")?;
            (
                session.get_property::<String>("Class").unwrap_or_default(),
                session.get_property::<bool>("CanLock").unwrap_or(false),
            )
        };
        if class == "user" && can_lock {
            return Ok(path);
        }
    }

    let user_path: OwnedObjectPath = manager
        .call("GetUserByPID", &std::process::id())
        .context("resolving this process's logind user")?;
    let user = Proxy::new(
        connection,
        LOGIN_SERVICE,
        user_path.as_str(),
        USER_INTERFACE,
    )
    .context("building the logind user proxy")?;
    let (session_id, session_path): (String, OwnedObjectPath) = user
        .get_property("Display")
        .context("reading the user's graphical display session")?;
    if session_id.is_empty() || session_path.as_str() == "/" {
        return Err(anyhow!("logind reports no graphical display session"));
    }
    Ok(session_path)
}

fn watch_messages(
    watch: &WatchConnection,
    state: &mut PresenceState,
    tx: &Sender<WatchEvent>,
) -> Result<()> {
    for message in MessageIterator::from(watch.connection.clone()) {
        let message = message.context("reading from the system bus")?;
        match message.header().member().map(|member| member.as_str()) {
            Some("PrepareForSleep") => {
                let sleeping: bool = message
                    .body()
                    .deserialize()
                    .context("decoding PrepareForSleep")?;
                if !state.set_sleeping(sleeping, tx) {
                    return Ok(());
                }
            }
            Some("PropertiesChanged") => {
                let (interface, changed, invalidated): (
                    String,
                    HashMap<String, OwnedValue>,
                    Vec<String>,
                ) = message
                    .body()
                    .deserialize()
                    .context("decoding PropertiesChanged")?;
                if interface != SESSION_INTERFACE {
                    continue;
                }

                let locked = if let Some(value) = changed.get("LockedHint") {
                    Some(bool::try_from(value).context("decoding LockedHint")?)
                } else if invalidated.iter().any(|name| name == "LockedHint") {
                    let session = Proxy::new(
                        &watch.connection,
                        LOGIN_SERVICE,
                        watch.session_path.as_str(),
                        SESSION_INTERFACE,
                    )
                    .context("rebuilding the logind session proxy")?;
                    Some(
                        session
                            .get_property::<bool>("LockedHint")
                            .context("refreshing invalidated LockedHint")?,
                    )
                } else {
                    None
                };
                if let Some(locked) = locked {
                    if !state.set_locked(locked, tx) {
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn locked_hint_rule(path: &str) -> Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(LOGIN_SERVICE)?
        .path(path)?
        .interface(PROPERTIES_INTERFACE)?
        .member("PropertiesChanged")?
        .add_arg(SESSION_INTERFACE)?
        .build()
        .to_owned())
}

fn prepare_for_sleep_rule() -> Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(LOGIN_SERVICE)?
        .path(MANAGER_PATH)?
        .interface(MANAGER_INTERFACE)?
        .member("PrepareForSleep")?
        .build()
        .to_owned())
}

#[derive(Default)]
struct PresenceState {
    locked: bool,
    sleeping: bool,
    last_present: Option<bool>,
}

impl PresenceState {
    fn apply_snapshot(&mut self, locked: bool, sleeping: bool, tx: &Sender<WatchEvent>) -> bool {
        self.locked = locked;
        self.sleeping = sleeping;
        self.publish_if_changed(tx)
    }

    fn set_locked(&mut self, locked: bool, tx: &Sender<WatchEvent>) -> bool {
        self.locked = locked;
        self.publish_if_changed(tx)
    }

    fn set_sleeping(&mut self, sleeping: bool, tx: &Sender<WatchEvent>) -> bool {
        self.sleeping = sleeping;
        self.publish_if_changed(tx)
    }

    fn transition(&mut self) -> Option<bool> {
        let present = !self.locked && !self.sleeping;
        if self.last_present == Some(present) {
            return None;
        }
        self.last_present = Some(present);
        Some(present)
    }

    fn publish_if_changed(&mut self, tx: &Sender<WatchEvent>) -> bool {
        let Some(present) = self.transition() else {
            return true;
        };
        tx.send(
            PresenceEvent {
                present,
                timestamp: Utc::now(),
            }
            .into(),
        )
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_and_sleep_are_composed_without_premature_resume() {
        let mut state = PresenceState::default();
        assert_eq!(state.transition(), Some(true));

        state.locked = true;
        assert_eq!(state.transition(), Some(false));
        state.sleeping = true;
        assert_eq!(state.transition(), None);
        state.sleeping = false;
        assert_eq!(state.transition(), None, "still locked after wake");
        state.locked = false;
        assert_eq!(state.transition(), Some(true));
    }

    #[test]
    fn duplicate_state_observations_are_idempotent() {
        let mut state = PresenceState::default();
        assert_eq!(state.transition(), Some(true));
        assert_eq!(state.transition(), None);
        state.locked = true;
        assert_eq!(state.transition(), Some(false));
        assert_eq!(state.transition(), None);
    }
}
