use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::MatchRule;

use super::provider::{ToplevelProvider, WindowObservation};

pub const BUS_NAME: &str = "dev.phucvinh57.MakoraShell";
pub const OBJECT_PATH: &str = "/dev/phucvinh57/MakoraShell";
pub const INTERFACE: &str = "dev.phucvinh57.MakoraShell.Focus1";

pub const PROTOCOL_VERSION: u32 = 1;

const RETRY_DELAY: Duration = Duration::from_secs(5);

type FocusPayload = (bool, u32, String, String, String);

pub struct GnomeProvider;

impl ToplevelProvider for GnomeProvider {
    const NAME: &'static str = "GNOME shell extension";

    fn probe() -> Option<Self> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        desktop
            .split(':')
            .any(|component| component.eq_ignore_ascii_case("GNOME"))
            .then_some(Self)
    }

    fn run(self, emit: &mut dyn FnMut(WindowObservation)) {
        loop {
            if let Err(err) = watch(emit) {
                warn!(
                    error = format!("{err:#}"),
                    "GNOME focus provider failed; retrying"
                );
            }
            thread::sleep(RETRY_DELAY);
        }
    }
}

fn watch(emit: &mut dyn FnMut(WindowObservation)) -> Result<()> {
    let connection = Connection::session().context("connecting to the session bus")?;
    let dbus = DBusProxy::new(&connection).context("opening the bus driver proxy")?;

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
        warn!(
            "the Makora GNOME extension is not running — tracking starts as soon as it is \
             enabled. Install it from gnome-extension/ and run: \
             gnome-extensions enable makora@phucvinh57.dev"
        );
    }

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
                if !new_owner.is_empty() {
                    info!("the Makora GNOME extension is now running");
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

fn sync_focused_window(proxy: &Proxy<'_>, emit: &mut dyn FnMut(WindowObservation)) {
    match proxy.call::<_, _, FocusPayload>("GetFocusedWindow", &()) {
        Ok(payload) => {
            if let Some(observation) = observation(payload) {
                emit(observation);
            }
        }
        Err(err) => warn!(error = %err, "could not read the focused window"),
    }
}

fn check_protocol_version(proxy: &Proxy<'_>) {
    match proxy.get_property::<u32>("Version") {
        Ok(version) if version == PROTOCOL_VERSION => {}
        Ok(version) => warn!(
            installed = version,
            expected = PROTOCOL_VERSION,
            "the installed GNOME extension speaks a different protocol version — reinstall it \
             from gnome-extension/ if focus tracking misbehaves"
        ),
        Err(err) => warn!(error = %err, "could not read the GNOME extension version"),
    }
}

fn observation(payload: FocusPayload) -> Option<WindowObservation> {
    let (present, pid, app_id, wm_class, title) = payload;
    if !present {
        return None;
    }
    Some(WindowObservation {
        pid: (pid != 0).then_some(pid),
        app_id,
        wm_class,
        title,
    })
}
