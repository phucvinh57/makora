use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use block2::RcBlock;
use chrono::Utc;
use core_foundation_sys::base::{kCFAllocatorDefault, CFIndex, CFRelease, CFTypeRef};
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringGetLength,
    CFStringRef,
};
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

use crate::tracking::adapters::model::{AppInfo, FocusEvent, WatchEvent};
use crate::tracking::adapters::presence::PresenceState;
use crate::tracking::adapters::watcher::FocusWatcher;

#[link(name = "Cocoa", kind = "framework")]
extern "C" {}

#[derive(Default)]
pub struct MacFocusWatcher {
    thread: Option<thread::JoinHandle<()>>,
}

impl MacFocusWatcher {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FocusWatcher for MacFocusWatcher {
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()> {
        let handle = thread::Builder::new()
            .name("makora-focus-watcher".into())
            .spawn(move || run_watcher(tx))
            .map_err(|e| anyhow!("spawning focus watcher thread: {e}"))?;
        self.thread = Some(handle);
        Ok(())
    }
}

/// A registered notification observer. The center keeps its own references, but holding these
/// for as long as the observing thread lives keeps that from depending on it.
struct Observer {
    _block: RcBlock<dyn Fn(*mut AnyObject)>,
    _token: *mut AnyObject,
}

unsafe fn observe(
    center: *mut AnyObject,
    name: &str,
    queue: *mut AnyObject,
    handler: impl Fn(*mut AnyObject) + 'static,
) -> Observer {
    let block: RcBlock<dyn Fn(*mut AnyObject)> = RcBlock::new(handler);
    let token: *mut AnyObject = msg_send![
        center,
        addObserverForName: ns_string(name),
        object: std::ptr::null_mut::<AnyObject>(),
        queue: queue,
        usingBlock: &*block,
    ];
    Observer {
        _block: block,
        _token: token,
    }
}

fn run_watcher(tx: Sender<WatchEvent>) {
    let active_focus = Arc::new(Mutex::new(None::<FocusEvent>));
    let _activation = unsafe {
        if ax::AXIsProcessTrusted() == 0 {
            tracing::warn!(
                "macOS Accessibility permission is not granted; tracking will continue, but window titles will remain empty until Makora is allowed in System Settings → Privacy & Security → Accessibility"
            );
        }

        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let center: *mut AnyObject = msg_send![workspace, notificationCenter];

        let queue: *mut AnyObject = msg_send![class!(NSOperationQueue), new];
        let active_focus_for_observer = Arc::clone(&active_focus);
        let tx_for_observer = tx.clone();

        let observer = autoreleasepool(|_pool| {
            observe(center, "NSWorkspaceDidActivateApplicationNotification", queue, move |notification| {
                autoreleasepool(|_pool| {
                    let mut active_focus = active_focus_for_observer.lock().unwrap_or_else(PoisonError::into_inner);
                    let event = match activated_from_notification(notification) {
                        Some(Activated::App(event)) => {
                            *active_focus = Some(event.clone());
                            event.into()
                        }
                        Some(Activated::Makora) => {
                            *active_focus = None;
                            WatchEvent::Unattributed { timestamp: Utc::now() }
                        }
                        None => return,
                    };
                    let _ = tx_for_observer.send(event);
                });
            })
        });

        autoreleasepool(|_pool| {
            let current_app: *mut AnyObject = msg_send![workspace, frontmostApplication];
            if let Some(Activated::App(event)) = activated_application(current_app) {
                *active_focus.lock().unwrap_or_else(PoisonError::into_inner) = Some(event.clone());
                let _ = tx.send(event.into());
            }
        });
        observer
    };

    loop {
        thread::park_timeout(Duration::from_millis(500));
        let Some(event) = active_focus.lock().unwrap_or_else(PoisonError::into_inner).clone() else {
            continue;
        };
        if frontmost_process_id() != Some(event.app.process_id as i32) {
            continue;
        }
        let Some(title) = focused_window_title(event.app.process_id as i32) else {
            continue;
        };
        if title == event.window_title {
            continue;
        }

        let updated = {
            let mut active_focus = active_focus.lock().unwrap_or_else(PoisonError::into_inner);
            match active_focus.as_mut() {
                Some(current)
                    if current.app.process_id == event.app.process_id
                        && current.window_title != title =>
                {
                    current.window_title = title;
                    current.timestamp = Utc::now();
                    Some(current.clone())
                }
                _ => None,
            }
        };
        if let Some(updated) = updated {
            let _ = tx.send(updated.into());
        }
    }
}

type PresenceChange = fn(&mut PresenceState, &Sender<WatchEvent>) -> bool;

const PRESENCE_NOTIFICATIONS: [(bool, &str, PresenceChange); 6] = [
    (false, "NSWorkspaceWillSleepNotification", |state, tx| state.set_sleeping(true, tx)),
    (false, "NSWorkspaceDidWakeNotification", |state, tx| state.set_sleeping(false, tx)),
    (false, "NSWorkspaceScreensDidSleepNotification", |state, tx| state.set_screens_off(true, tx)),
    (false, "NSWorkspaceScreensDidWakeNotification", |state, tx| state.set_screens_off(false, tx)),
    (true, "com.apple.screenIsLocked", |state, tx| state.set_locked(true, tx)),
    (true, "com.apple.screenIsUnlocked", |state, tx| state.set_locked(false, tx)),
];

/// Sleep and screen sleep come from the workspace notification center, lock and unlock from the
/// distributed one. The thread only keeps the observers alive; the blocks run on their queue.
pub fn start_presence_watcher(tx: Sender<WatchEvent>) -> Result<()> {
    thread::Builder::new()
        .name("makora-presence-watcher".into())
        .spawn(move || run_presence_watcher(tx))
        .context("spawning presence-watcher thread")?;
    Ok(())
}

fn run_presence_watcher(tx: Sender<WatchEvent>) {
    let mut state = PresenceState::default();
    if !state.apply_snapshot(false, false, &tx) {
        return;
    }
    let presence = Arc::new(Mutex::new((state, tx)));
    let _observers: Vec<Observer> = unsafe {
        autoreleasepool(|_pool| {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let workspace_center: *mut AnyObject = msg_send![workspace, notificationCenter];
            let distributed_center: *mut AnyObject =
                msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
            let queue: *mut AnyObject = msg_send![class!(NSOperationQueue), new];
            PRESENCE_NOTIFICATIONS
                .into_iter()
                .map(|(distributed, name, change)| {
                    let center = if distributed { distributed_center } else { workspace_center };
                    let presence = Arc::clone(&presence);
                    observe(center, name, queue, move |_notification| {
                        let mut presence = presence.lock().unwrap_or_else(PoisonError::into_inner);
                        let (state, tx) = &mut *presence;
                        change(state, tx);
                    })
                })
                .collect()
        })
    };
    loop {
        thread::park();
    }
}

fn frontmost_process_id() -> Option<i32> {
    unsafe {
        autoreleasepool(|_pool| {
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let pid: i32 = msg_send![app, processIdentifier];
            (pid > 0).then_some(pid)
        })
    }
}

enum Activated {
    App(FocusEvent),
    Makora,
}

unsafe fn activated_from_notification(notification: *mut AnyObject) -> Option<Activated> {
    if notification.is_null() {
        return None;
    }

    let user_info: *mut AnyObject = msg_send![notification, userInfo];
    if user_info.is_null() {
        return None;
    }

    let key = ns_string("NSWorkspaceApplicationKey");
    let app: *mut AnyObject = msg_send![user_info, objectForKey: key];
    activated_application(app)
}

unsafe fn activated_application(app: *mut AnyObject) -> Option<Activated> {
    if app.is_null() {
        return None;
    }

    let pid: i32 = msg_send![app, processIdentifier];
    if pid <= 0 {
        return None;
    }

    if pid == std::process::id() as i32 {
        return Some(Activated::Makora);
    }

    let exe_url: *mut AnyObject = msg_send![app, executableURL];
    let exe_path = ns_url_path(exe_url)?;
    let app_name = Path::new(&exe_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe_path.clone());

    Some(Activated::App(FocusEvent {
        app: AppInfo {
            process_id: pid as u32,
            exe_path,
            app_name,
        },
        window_title: focused_window_title(pid).unwrap_or_default(),
        timestamp: Utc::now(),
    }))
}

unsafe fn ns_url_path(url: *mut AnyObject) -> Option<String> {
    if url.is_null() {
        return None;
    }
    let path: *mut AnyObject = msg_send![url, path];
    ns_string_to_string(path)
}

unsafe fn ns_string_to_string(obj: *mut AnyObject) -> Option<String> {
    if obj.is_null() {
        return None;
    }
    let utf8: *const c_char = msg_send![obj, UTF8String];
    if utf8.is_null() {
        return None;
    }
    Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
}

unsafe fn ns_string(s: &str) -> *mut AnyObject {
    let c = CString::new(s).expect("literal has no interior NUL");
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

fn focused_window_title(pid: i32) -> Option<String> {
    unsafe {
        let app_element = ax::AXUIElementCreateApplication(pid);
        if app_element.is_null() {
            return None;
        }

        let focused_window = ax_copy_attribute(app_element, "AXFocusedWindow");
        CFRelease(app_element);
        let focused_window = focused_window?;

        let title_ref = ax_copy_attribute(focused_window, "AXTitle");
        CFRelease(focused_window);
        let title_ref = title_ref?;

        let title = cfstring_to_string(title_ref as CFStringRef);
        CFRelease(title_ref);
        title
    }
}

unsafe fn ax_copy_attribute(element: ax::AXUIElementRef, attribute: &str) -> Option<CFTypeRef> {
    let name = CString::new(attribute).ok()?;
    let attr_ref =
        CFStringCreateWithCString(kCFAllocatorDefault, name.as_ptr(), kCFStringEncodingUTF8);
    if attr_ref.is_null() {
        return None;
    }

    let mut value: CFTypeRef = std::ptr::null();
    let err = ax::AXUIElementCopyAttributeValue(element, attr_ref, &mut value);
    CFRelease(attr_ref as CFTypeRef);

    if err != ax::SUCCESS || value.is_null() {
        return None;
    }
    Some(value)
}

unsafe fn cfstring_to_string(s: CFStringRef) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let len = CFStringGetLength(s);
    let capacity = (len as usize).saturating_mul(3) + 1;
    let mut buf = vec![0 as c_char; capacity];
    if CFStringGetCString(
        s,
        buf.as_mut_ptr(),
        capacity as CFIndex,
        kCFStringEncodingUTF8,
    ) == 0
    {
        return None;
    }
    Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

mod ax {
    use core_foundation_sys::base::CFTypeRef;
    use core_foundation_sys::string::CFStringRef;

    pub type AXError = i32;
    pub type AXUIElementRef = CFTypeRef;

    pub const SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXIsProcessTrusted() -> core_foundation_sys::base::Boolean;
        pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
    }
}
