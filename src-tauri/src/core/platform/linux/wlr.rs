use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_registry;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    Event as HandleEvent, State as HandleState, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    Event as ManagerEvent, ZwlrForeignToplevelManagerV1, EVT_TOPLEVEL_OPCODE,
};

use super::provider::{ToplevelProvider, WindowObservation};

const MANAGER_VERSION: u32 = 3;

pub struct WlrProvider {
    connection: Connection,
}

impl ToplevelProvider for WlrProvider {
    const NAME: &'static str = "wlr-foreign-toplevel-management";

    fn probe() -> Option<Self> {
        let connection = Connection::connect_to_env().ok()?;
        let mut queue = connection.new_event_queue::<Probe>();
        connection.display().get_registry(&queue.handle(), ());

        let mut probe = Probe::default();
        queue.roundtrip(&mut probe).ok()?;
        probe.found.then_some(Self { connection })
    }

    fn run(self, emit: &mut dyn FnMut(WindowObservation)) {
        let mut queue = self.connection.new_event_queue::<Toplevels>();
        self.connection.display().get_registry(&queue.handle(), ());

        let mut state = Toplevels::default();
        loop {
            if let Err(err) = queue.blocking_dispatch(&mut state) {
                eprintln!("makora: wlr focus provider: {err}");
                return;
            }
            for observation in state.emitted.drain(..) {
                emit(observation);
            }
            if state.finished {
                eprintln!("makora: the compositor stopped reporting toplevels");
                return;
            }
        }
    }
}

#[derive(Default)]
struct Probe {
    found: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
    fn event(
        state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, .. } = event {
            if interface == ZwlrForeignToplevelManagerV1::interface().name {
                state.found = true;
            }
        }
    }
}

#[derive(Default)]
struct Toplevel {
    app_id: String,
    title: String,
    activated: bool,
    pending_app_id: Option<String>,
    pending_title: Option<String>,
    pending_activated: Option<bool>,
}

impl Toplevel {
    fn commit(&mut self) {
        if let Some(app_id) = self.pending_app_id.take() {
            self.app_id = app_id;
        }
        if let Some(title) = self.pending_title.take() {
            self.title = title;
        }
        if let Some(activated) = self.pending_activated.take() {
            self.activated = activated;
        }
    }
}

#[derive(Default)]
struct Toplevels {
    windows: HashMap<ObjectId, Toplevel>,
    active: Option<ObjectId>,
    emitted: Vec<WindowObservation>,
    finished: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Toplevels {
    fn event(
        _state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        if interface == ZwlrForeignToplevelManagerV1::interface().name {
            registry.bind::<ZwlrForeignToplevelManagerV1, _, _>(
                name,
                version.min(MANAGER_VERSION),
                queue,
                (),
            );
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Toplevels {
    fn event(
        state: &mut Self,
        _manager: &ZwlrForeignToplevelManagerV1,
        event: ManagerEvent,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        if let ManagerEvent::Finished = event {
            state.finished = true;
        }
    }

    event_created_child!(Toplevels, ZwlrForeignToplevelManagerV1, [
        EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Toplevels {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: HandleEvent,
        _data: &(),
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        let id = handle.id();
        match event {
            HandleEvent::Title { title } => {
                state.windows.entry(id).or_default().pending_title = Some(title);
            }
            HandleEvent::AppId { app_id } => {
                state.windows.entry(id).or_default().pending_app_id = Some(app_id);
            }
            HandleEvent::State { state: flags } => {
                state.windows.entry(id).or_default().pending_activated = Some(is_activated(&flags));
            }
            HandleEvent::Done => {
                let Some(window) = state.windows.get_mut(&id) else {
                    return;
                };
                window.commit();
                if window.activated && state.active.as_ref() != Some(&id) {
                    state.active = Some(id);
                    state.emitted.push(WindowObservation {
                        pid: None,
                        app_id: window.app_id.clone(),
                        wm_class: window.app_id.clone(),
                        title: window.title.clone(),
                    });
                }
            }
            HandleEvent::Closed => {
                state.windows.remove(&id);
                if state.active.as_ref() == Some(&id) {
                    state.active = None;
                }
            }
            _ => {}
        }
    }
}

fn is_activated(flags: &[u8]) -> bool {
    flags
        .chunks_exact(4)
        .filter_map(|chunk| chunk.try_into().ok().map(u32::from_ne_bytes))
        .any(|flag| flag == HandleState::Activated as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(states: &[HandleState]) -> Vec<u8> {
        states
            .iter()
            .flat_map(|state| (*state as u32).to_ne_bytes())
            .collect()
    }

    #[test]
    fn probing_is_safe_on_any_session() {
        let found = WlrProvider::probe().is_some();
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            assert!(!found, "no Wayland display, so there is nothing to bind");
        }
    }

    #[test]
    fn activated_is_found_among_other_states() {
        let flags = encode(&[
            HandleState::Maximized,
            HandleState::Activated,
            HandleState::Fullscreen,
        ]);
        assert!(is_activated(&flags));
    }

    #[test]
    fn a_window_without_activated_is_not_focused() {
        assert!(!is_activated(&encode(&[HandleState::Maximized])));
        assert!(!is_activated(&[]));
    }

    #[test]
    fn a_truncated_payload_does_not_panic() {
        assert!(!is_activated(&[0x01, 0x00]));
    }

    #[test]
    fn pending_properties_only_apply_once_committed() {
        let mut window = Toplevel {
            pending_title: Some("Inbox".into()),
            pending_activated: Some(true),
            ..Default::default()
        };
        assert_eq!(window.title, "");
        assert!(!window.activated);

        window.commit();
        assert_eq!(window.title, "Inbox");
        assert!(window.activated);

        window.pending_title = Some("Drafts".into());
        window.commit();
        assert_eq!(window.title, "Drafts");
        assert!(window.activated);
    }
}
