use std::cell::RefCell;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowTextW,
    GetWindowThreadProcessId, RegisterClassW, TranslateMessage, CHILDID_SELF, EVENT_OBJECT_NAMECHANGE,
    EVENT_SYSTEM_FOREGROUND, MSG, OBJID_WINDOW, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND,
    WINEVENT_OUTOFCONTEXT, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
};

use crate::tracking::adapters::model::{AppInfo, FocusEvent, WatchEvent};
use crate::tracking::adapters::presence::PresenceState;
use crate::tracking::adapters::watcher::FocusWatcher;

thread_local! {
    static EVENT_TX: RefCell<Option<Sender<WatchEvent>>> = const { RefCell::new(None) };
    static PRESENCE: RefCell<Option<(PresenceState, Sender<WatchEvent>)>> = const { RefCell::new(None) };
}

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

fn run_event_loop(tx: Sender<WatchEvent>) {
    EVENT_TX.with(|cell| *cell.borrow_mut() = Some(tx));

    unsafe {
        let focus_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let title_hook = SetWinEventHook(
            EVENT_OBJECT_NAMECHANGE,
            EVENT_OBJECT_NAMECHANGE,
            std::ptr::null_mut(),
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if focus_hook.is_null() {
            if !title_hook.is_null() {
                UnhookWinEvent(title_hook);
            }
            tracing::error!("SetWinEventHook failed; app-usage tracking is disabled");
            return;
        }
        if title_hook.is_null() {
            tracing::warn!("SetWinEventHook failed; window-title change tracking is disabled");
        }

        let foreground = GetForegroundWindow();
        if !foreground.is_null() {
            win_event_proc(
                focus_hook,
                EVENT_SYSTEM_FOREGROUND,
                foreground,
                OBJID_WINDOW,
                CHILDID_SELF as i32,
                0,
                0,
            );
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}

        if !title_hook.is_null() {
            UnhookWinEvent(title_hook);
        }
        UnhookWinEvent(focus_hook);
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
    if hwnd.is_null() || id_object != OBJID_WINDOW || id_child != CHILDID_SELF as i32 {
        return;
    }
    if event == EVENT_OBJECT_NAMECHANGE {
        if GetForegroundWindow() != hwnd {
            return;
        }
    } else if event != EVENT_SYSTEM_FOREGROUND {
        return;
    }

    let mut process_id: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    let timestamp = Utc::now();
    let event = if process_id == std::process::id() {
        if event != EVENT_SYSTEM_FOREGROUND {
            return;
        }
        WatchEvent::Unattributed { timestamp }
    } else {
        let Some(app) = app_info(process_id) else {
            return;
        };
        FocusEvent {
            app,
            window_title: window_title(hwnd),
            timestamp,
        }
        .into()
    };
    send(event);
}

fn send(event: WatchEvent) {
    EVENT_TX.with(|cell| {
        if let Some(tx) = cell.borrow().as_ref() {
            let _ = tx.send(event);
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

fn app_info(process_id: u32) -> Option<AppInfo> {
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

/// Lock/unlock and suspend/resume arrive as window messages, so a hidden top-level window (a
/// message-only one misses the power broadcast) pumps them on a thread of its own.
pub fn start_presence_watcher(tx: Sender<WatchEvent>) -> Result<()> {
    thread::Builder::new()
        .name("makora-presence-watcher".into())
        .spawn(move || run_presence_loop(tx))
        .context("spawning presence-watcher thread")?;
    Ok(())
}

fn run_presence_loop(tx: Sender<WatchEvent>) {
    let mut state = PresenceState::default();
    if !state.apply_snapshot(false, false, &tx) {
        return;
    }
    PRESENCE.with(|cell| *cell.borrow_mut() = Some((state, tx)));

    let class_name: Vec<u16> = "MakoraPresenceWatcher\0".encode_utf16().collect();
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(presence_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            tracing::error!("RegisterClassW failed; lock and sleep are not tracked");
            return;
        }
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            tracing::error!("CreateWindowExW failed; lock and sleep are not tracked");
            return;
        }
        if WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) == 0 {
            tracing::warn!("WTSRegisterSessionNotification failed; screen lock is not tracked");
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn presence_window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let change: Option<fn(&mut PresenceState, &Sender<WatchEvent>) -> bool> = match (message, wparam as u32) {
        (WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK) => Some(|state, tx| state.set_locked(true, tx)),
        (WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK) => Some(|state, tx| state.set_locked(false, tx)),
        (WM_POWERBROADCAST, PBT_APMSUSPEND) => Some(|state, tx| state.set_sleeping(true, tx)),
        (WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND) => {
            Some(|state, tx| state.set_sleeping(false, tx))
        }
        _ => None,
    };
    if let Some(change) = change {
        PRESENCE.with(|cell| {
            if let Some((state, tx)) = cell.borrow_mut().as_mut() {
                change(state, tx);
            }
        });
    }
    DefWindowProcW(window, message, wparam, lparam)
}
