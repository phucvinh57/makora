//! The one main window: shown, raised or rebuilt on demand. Closing it only hides the app; the
//! tray brings it back.

use std::sync::{Arc, LazyLock};

use gpui_kit::component::Root;
use gpui_kit::{
    App, AppContext, Entity, Global, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions,
    px, size,
};
use makora_lib::paths::APP_ID;
use tracing::{error, warn};

use crate::ui::route::Route;
use crate::ui::shell::Shell;

pub const TITLE: &str = "Makora";
const WINDOW_PNG: &[u8] = include_bytes!("../../icons/128x128.png");

static WINDOW_ICON: LazyLock<Option<Arc<image::RgbaImage>>> = LazyLock::new(|| {
    match image::load_from_memory_with_format(WINDOW_PNG, image::ImageFormat::Png) {
        Ok(icon) => Some(Arc::new(icon.into_rgba8())),
        Err(err) => {
            warn!(error = %err, "the window icon could not be decoded");
            None
        }
    }
});

#[derive(Clone)]
struct MainWindow {
    handle: WindowHandle<Root>,
    shell: Entity<Shell>,
}

#[derive(Default)]
struct MainWindowSlot(Option<MainWindow>);

impl Global for MainWindowSlot {}

/// Forgets the window once it closes, so the next show builds a new one.
pub fn init(cx: &mut App) {
    cx.set_global(MainWindowSlot::default());
    cx.on_window_closed(|cx, id| {
        let slot = cx.global_mut::<MainWindowSlot>();
        if slot
            .0
            .as_ref()
            .is_some_and(|main| main.handle.window_id() == id)
        {
            slot.0 = None;
        }
    })
    .detach();
}

/// Raises the main window, opening it if it is closed, and moves it to `route` when one is given.
pub fn show_main_window(route: Option<Route>, cx: &mut App) {
    if let Some(main) = cx
        .try_global::<MainWindowSlot>()
        .and_then(|slot| slot.0.clone())
    {
        let raised = main.handle.update(cx, |_, window, cx| {
            window.activate_window();
            if let Some(route) = route.clone() {
                main.shell
                    .update(cx, |shell, cx| shell.navigate(route, window, cx));
            }
        });
        if raised.is_ok() {
            return;
        }
    }
    open(route, cx);
}

/// Runs `f` against the main window when it is open; does nothing otherwise.
pub fn with_main_window(cx: &mut App, f: impl FnOnce(&mut gpui_kit::Window, &mut App)) {
    if let Some(main) = cx
        .try_global::<MainWindowSlot>()
        .and_then(|slot| slot.0.clone())
    {
        let _ = cx.update_window(main.handle.into(), |_, window, cx| f(window, cx));
    }
}

fn open(route: Option<Route>, cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1000.), px(700.)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some(TITLE.into()),
            ..Default::default()
        }),
        window_min_size: Some(size(px(900.), px(620.))),
        app_id: Some(APP_ID.into()),
        icon: WINDOW_ICON.clone(),
        ..Default::default()
    };
    let mut shell = None;
    let opened = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| Shell::new(route, window, cx));
        shell = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    });
    match (opened, shell) {
        (Ok(handle), Some(shell)) => {
            cx.global_mut::<MainWindowSlot>().0 = Some(MainWindow { handle, shell });
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
        (Err(err), _) => error!(error = format!("{err:#}"), "failed to open the main window"),
        (Ok(_), None) => error!("the main window opened without its content"),
    }
}
