use std::collections::HashMap;

use tracing::error;
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

    fn run(self, emit: &mut dyn FnMut(Option<WindowObservation>)) {
        let mut queue = self.connection.new_event_queue::<Toplevels>();
        self.connection.display().get_registry(&queue.handle(), ());

        let mut state = Toplevels::default();
        loop {
            if let Err(err) = queue.blocking_dispatch(&mut state) {
                error!(error = %err, "wlr focus provider stopped");
                return;
            }
            for observation in state.emitted.drain(..) {
                emit(Some(observation));
            }
            if state.finished {
                error!("the compositor stopped reporting toplevels");
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
    active_title: String,
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
                let changed = state.active.as_ref() != Some(&id) || state.active_title != window.title;
                if window.activated && changed {
                    state.active = Some(id);
                    state.active_title.clone_from(&window.title);
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
        .as_chunks::<4>()
        .0
        .iter()
        .any(|chunk| u32::from_ne_bytes(*chunk) == HandleState::Activated as u32)
}
