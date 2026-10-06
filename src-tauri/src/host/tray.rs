use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl};

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let builder = TrayIconBuilder::new();
    #[cfg(target_os = "macos")]
    let builder = builder
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.icon(
        app.default_window_icon()
            .cloned()
            .expect("app icon configured in tauri.conf.json"),
    );

    builder
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

pub fn show_main_window(app: &AppHandle) {
    open_main_window(app, None);
}

/// Shows the main window on a frontend route such as `/categories?tab=review`.
pub fn show_main_window_at(app: &AppHandle, route: &str) {
    open_main_window(app, Some(route));
}

fn open_main_window(app: &AppHandle, route: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(route) = route {
            let _ = window.emit("navigate", route);
        }
        return;
    }

    let Some(mut config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
        .cloned()
    else {
        tracing::error!("no \"main\" window config to rebuild the window from");
        return;
    };
    // A fresh page has no listener for `navigate` yet, so it starts on the route instead.
    if let Some(route) = route {
        config.url = WebviewUrl::App(route.trim_start_matches('/').into());
    }

    let window = match tauri::WebviewWindowBuilder::from_config(app, &config)
        .and_then(|builder| builder.build())
    {
        Ok(window) => window,
        Err(err) => {
            tracing::error!(
                error = format!("{err:#}"),
                "failed to rebuild the main window"
            );
            return;
        }
    };
    let _ = window.show();
    let _ = window.set_focus();
}
