//! GNOME focus watching, via the shell extension in `gnome-extension/`.
//!
//! Mutter exposes no usable focus API to ordinary clients (see the module
//! docs in [`super`]), so the extension runs *inside* gnome-shell, watches
//! `global.display`'s `notify::focus-window`, and re-publishes what it sees
//! on the session bus. This module is the client half.
//!
//! The bus name, object path, interface and protocol version below are
//! duplicated in `gnome-extension/extension.js` — change one, change the
//! other. `gnome-extension/README.md` documents the contract.

use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::MatchRule;

use super::provider::{ToplevelProvider, WindowObservation};

pub const BUS_NAME: &str = "dev.phucvinh57.MakoraShell";
pub const OBJECT_PATH: &str = "/dev/phucvinh57/MakoraShell";
pub const INTERFACE: &str = "dev.phucvinh57.MakoraShell.Focus1";

/// Bumped only for a breaking change to the signal or method signature.
pub const PROTOCOL_VERSION: u32 = 1;

/// How long to wait before rebuilding everything after the session bus
/// itself fails. Deliberately unhurried: the common causes (bus restart,
/// logout in progress) are not fixed by retrying quickly.
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// `(present, pid, app_id, wm_class, title)` — the body of both
/// `FocusChanged` and `GetFocusedWindow`.
type FocusPayload = (bool, u32, String, String, String);

pub struct GnomeProvider;

impl ToplevelProvider for GnomeProvider {
    const NAME: &'static str = "GNOME shell extension";

    fn probe() -> Option<Self> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        // Ubuntu reports "ubuntu:GNOME", so match on the component rather
        // than the whole value.
        desktop
            .split(':')
            .any(|component| component.eq_ignore_ascii_case("GNOME"))
            .then_some(Self)
    }

    fn run(self, emit: &mut dyn FnMut(WindowObservation)) {
        loop {
            if let Err(err) = watch(emit) {
                eprintln!("makora: GNOME focus provider: {err:#}");
            }
            thread::sleep(RETRY_DELAY);
        }
    }
}

/// One attempt at a complete watch: connect, subscribe, sync, then stream
/// until something breaks.
///
/// Returning `Ok(())` means the message stream ended, which only happens
/// when the bus connection closes — the caller retries either way.
fn watch(emit: &mut dyn FnMut(WindowObservation)) -> Result<()> {
    let connection = Connection::session().context("connecting to the session bus")?;
    let dbus = DBusProxy::new(&connection).context("opening the bus driver proxy")?;

    // Both rules go on before the first query, so nothing that happens
    // during the query is missed.
    dbus.add_match_rule(focus_changed_rule()?)
        .context("subscribing to FocusChanged")?;
    dbus.add_match_rule(name_owner_changed_rule()?)
        .context("subscribing to NameOwnerChanged")?;

    let proxy = Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE)
        .context("building the focus proxy")?;

    if dbus.name_has_owner(BUS_NAME.try_into()?)? {
        check_protocol_version(&proxy);
        sync_focused_window(&proxy, emit);
    } else {
        eprintln!(
            "makora: the Makora GNOME extension is not running — tracking starts \
             as soon as it is enabled. Install it from gnome-extension/ and run: \
             gnome-extensions enable makora@phucvinh57.dev"
        );
    }

    // A single message stream rather than two signal iterators, because a
    // blocking iterator can only be waited on one at a time and both
    // signals matter: `FocusChanged` is the event, `NameOwnerChanged` is
    // how we notice the extension being enabled (or gnome-shell being
    // restarted) and re-read the focus we slept through.
    for message in MessageIterator::from(connection) {
        let message = message.context("reading from the session bus")?;
        match message.header().member().map(|member| member.as_str()) {
            Some("FocusChanged") => {
                let payload: FocusPayload = message
                    .body()
                    .deserialize()
                    .context("decoding a FocusChanged signal")?;
                if let Some(observation) = observation(payload) {
                    emit(observation);
                }
            }
            Some("NameOwnerChanged") => {
                let (_name, _old_owner, new_owner): (String, String, String) = message
                    .body()
                    .deserialize()
                    .context("decoding a NameOwnerChanged signal")?;
                // Filtered to our name by the match rule, so a non-empty
                // new owner means the extension just appeared.
                if !new_owner.is_empty() {
                    eprintln!("makora: the Makora GNOME extension is now running");
                    check_protocol_version(&proxy);
                    sync_focused_window(&proxy, emit);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn focus_changed_rule() -> Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(BUS_NAME)?
        .path(OBJECT_PATH)?
        .interface(INTERFACE)?
        .member("FocusChanged")?
        .build()
        .to_owned())
}

fn name_owner_changed_rule() -> Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender("org.freedesktop.DBus")?
        .path("/org/freedesktop/DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .add_arg(BUS_NAME)?
        .build()
        .to_owned())
}

/// Asks what is focused *right now*.
///
/// Without this the app would record nothing until the user next switched
/// apps — the whole reason the extension exposes a method as well as a
/// signal. Failure is logged and ignored: the next `FocusChanged` recovers.
fn sync_focused_window(proxy: &Proxy<'_>, emit: &mut dyn FnMut(WindowObservation)) {
    match proxy.call::<_, _, FocusPayload>("GetFocusedWindow", &()) {
        Ok(payload) => {
            if let Some(observation) = observation(payload) {
                emit(observation);
            }
        }
        Err(err) => eprintln!("makora: could not read the focused window: {err}"),
    }
}

/// Warns, but does not refuse to run, when the installed extension speaks a
/// different protocol version — a mismatch is usually a stale copy under
/// `~/.local/share/gnome-shell/extensions/`, and a silent no-op would be far
/// harder to diagnose than a wrong-but-working stream of events.
fn check_protocol_version(proxy: &Proxy<'_>) {
    match proxy.get_property::<u32>("Version") {
        Ok(version) if version == PROTOCOL_VERSION => {}
        Ok(version) => eprintln!(
            "makora: the installed GNOME extension speaks protocol v{version}, this build \
             expects v{PROTOCOL_VERSION} — reinstall it from gnome-extension/ if focus \
             tracking misbehaves"
        ),
        Err(err) => eprintln!("makora: could not read the GNOME extension version: {err}"),
    }
}

/// Maps a wire payload to an observation, dropping "nothing is focused".
///
/// GNOME reports a null focus window whenever the Activities overview
/// opens. Treating that as a focus change would end the session every time
/// the user glanced at the overview, so it is dropped and the open session
/// continues — the same call Windows makes by ignoring the shell.
fn observation(payload: FocusPayload) -> Option<WindowObservation> {
    let (present, pid, app_id, wm_class, title) = payload;
    if !present {
        return None;
    }
    Some(WindowObservation {
        // The extension sends 0 for "unknown"; `Meta.Window.get_pid` can
        // fail for windows the compositor has no process for.
        pid: (pid != 0).then_some(pid),
        app_id,
        wm_class,
        title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_present_window_becomes_an_observation() {
        let observed = observation((
            true,
            4321,
            "org.gnome.Nautilus.desktop".into(),
            "org.gnome.Nautilus".into(),
            "Home".into(),
        ))
        .expect("a focused window is an observation");
        assert_eq!(observed.pid, Some(4321));
        assert_eq!(observed.app_id, "org.gnome.Nautilus.desktop");
        assert_eq!(observed.title, "Home");
    }

    #[test]
    fn an_absent_focus_window_is_dropped() {
        // The Activities overview: the session must survive it.
        assert_eq!(
            observation((false, 0, String::new(), String::new(), String::new())),
            None
        );
    }

    #[test]
    fn a_zero_pid_is_reported_as_unknown() {
        let observed = observation((true, 0, "Alacritty".into(), "Alacritty".into(), "~".into()))
            .expect("still focused");
        assert_eq!(observed.pid, None);
    }

    #[test]
    fn match_rules_build() {
        // Cheap guard against a typo in the bus name or path, which would
        // otherwise show up only as an app that silently records nothing.
        focus_changed_rule().expect("focus rule");
        name_owner_changed_rule().expect("name owner rule");
    }
}
