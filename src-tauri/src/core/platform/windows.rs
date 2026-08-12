use std::cell::RefCell;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, GetWindowTextW, GetWindowThreadProcessId, CHILDID_SELF, EVENT_SYSTEM_FOREGROUND,
    MSG, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::core::model::{AppInfo, FocusEvent, WatchEvent};
use crate::core::watcher::FocusWatcher;

// The WinEvent callback is a plain `extern "system" fn`, not a closure, so it
// can't capture the channel. Out-of-context hooks deliver their callback on
// the thread that registered them, so a thread-local set right before the
// message loop starts is a safe way to hand it the sender.
thread_local! {
    static EVENT_TX: RefCell<Option<Sender<WatchEvent>>> = RefCell::new(None);
}

/// Watches the Windows foreground window via `SetWinEventHook`
/// (`EVENT_SYSTEM_FOREGROUND`) and reports every focus change.
///
/// This is event-driven, not polled: Windows calls back the instant focus
/// moves to a different window, so session boundaries line up with the
/// exact switch moment and no CPU is burned while focus is unchanged.
#[derive(Default)]
pub struct WindowsFocusWatcher {
    thread: Option<thread::JoinHandle<()>>,
}

impl WindowsFocusWatcher {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FocusWatcher for WindowsFocusWatcher {
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()> {
        let handle = thread::Builder::new()
            .name("makora-focus-watcher".into())
            .spawn(move || run_event_loop(tx))
            .map_err(|e| anyhow!("spawning focus watcher thread: {e}"))?;
        self.thread = Some(handle);
        Ok(())
    }
}

/// Registers the hook and pumps messages for its lifetime. WinEvent hooks
/// require the registering thread to run a Win32 message loop even though
/// no window is created — the OS delivers callbacks through that queue.
fn run_event_loop(tx: Sender<WatchEvent>) {
    EVENT_TX.with(|cell| *cell.borrow_mut() = Some(tx));

    unsafe {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if hook.is_null() {
            eprintln!("makora: SetWinEventHook failed; app-usage tracking is disabled");
            return;
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            // No TranslateMessage/DispatchMessage needed: WinEvent callbacks
            // for an out-of-context hook fire directly while this thread
            // pumps GetMessageW, they aren't posted as window messages.
        }

        UnhookWinEvent(hook);
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    // OBJID_WINDOW/CHILDID_SELF filters out events for controls *within* a
    // window (scrollbars, menus, ...) so only whole-window focus counts.
    if event != EVENT_SYSTEM_FOREGROUND
        || hwnd.is_null()
        || id_object != OBJID_WINDOW
        || id_child != CHILDID_SELF as i32
    {
        return;
    }

    let Some(app) = app_info_for_window(hwnd) else {
        return;
    };

    let focus_event = FocusEvent {
        app,
        window_title: window_title(hwnd),
        timestamp: Utc::now(),
    };

    EVENT_TX.with(|cell| {
        if let Some(tx) = cell.borrow().as_ref() {
            let _ = tx.send(focus_event.into());
        }
    });
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

fn app_info_for_window(hwnd: HWND) -> Option<AppInfo> {
    let mut process_id: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
    if process_id == 0 {
        return None;
    }

    let exe_path = process_exe_path(process_id)?;
    let app_name = Path::new(&exe_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe_path.clone());

    Some(AppInfo {
        process_id,
        exe_path,
        app_name,
    })
}

/// Resolves a process id to its executable's full path.
///
/// `PROCESS_QUERY_LIMITED_INFORMATION` is used deliberately: it's the access
/// right unprivileged processes can request even against elevated or
/// protected ones, so this works without running makora as administrator.
fn process_exe_path(process_id: u32) -> Option<String> {
    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if handle.is_null() {
            return None;
        }

        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}
