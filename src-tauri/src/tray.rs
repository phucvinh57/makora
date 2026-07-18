use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Builds the tray icon: left-click (or the "Show" menu item) restores the
/// main window, "Quit" exits the app. This is the only way back once the
/// window has been closed.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("app icon configured in tauri.conf.json"),
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Restores the main window, rebuilding it from `tauri.conf.json`'s window
/// config if it was destroyed (see `lib.rs`'s `ExitRequested` handling) —
/// closing the window frees its WebView2 process, so there's nothing to
/// show/focus until a fresh one is built.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
        .cloned()
    else {
        eprintln!("makora: no \"main\" window config to rebuild the window from");
        return;
    };

    let window = match tauri::WebviewWindowBuilder::from_config(app, &config)
        .and_then(|builder| builder.build())
    {
        Ok(window) => window,
        Err(err) => {
            eprintln!("makora: failed to rebuild main window: {err:#}");
            return;
        }
    };
    let _ = window.show();
    let _ = window.set_focus();
}
