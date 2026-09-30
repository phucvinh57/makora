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

use crate::tracking::adapters::model::{AppInfo, FocusEvent, WatchEvent};
use crate::tracking::adapters::watcher::FocusWatcher;

thread_local! {
    static EVENT_TX: RefCell<Option<Sender<WatchEvent>>> = RefCell::new(None);
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
            tracing::error!("SetWinEventHook failed; app-usage tracking is disabled");
            return;
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}

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
