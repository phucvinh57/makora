use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{anyhow, Result};
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

use crate::model::{AppInfo, FocusEvent};
use crate::watcher::FocusWatcher;

// `class!(NSWorkspace)` and friends resolve at link time, not through any
// generated binding crate, so the frameworks that own those classes have to
// be linked explicitly. Cocoa is the umbrella framework for AppKit
// (NSWorkspace) + Foundation (NSString, NSOperationQueue, ...); the `ax`
// module below links ApplicationServices separately for the Accessibility
// calls it makes.
#[link(name = "Cocoa", kind = "framework")]
extern "C" {}

/// Watches app activation via `NSWorkspace`'s
/// `didActivateApplicationNotification` and reports every focus change.
///
/// This is event-driven, not polled: macOS posts the notification the
/// instant a different app becomes frontmost. Unlike the Windows watcher
/// (which hooks per-window foreground changes), `NSWorkspace` only reports
/// *app*-level activation, but that's all `SessionTracker` needs — window
/// title is filled in separately via the Accessibility API, which requires
/// the user to have granted this process Accessibility permission (System
/// Settings > Privacy & Security > Accessibility). Until granted, sessions
/// are still recorded, just with an empty window title.
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
    fn start(&mut self, tx: Sender<FocusEvent>) -> Result<()> {
        let handle = thread::Builder::new()
            .name("makora-focus-watcher".into())
            .spawn(move || run_watcher(tx))
            .map_err(|e| anyhow!("spawning focus watcher thread: {e}"))?;
        self.thread = Some(handle);
        Ok(())
    }
}

/// Registers the `NSWorkspace` observer and parks. Delivery happens on a
/// dedicated `NSOperationQueue` (see below), not by pumping a run loop on
/// this thread, so this thread's only remaining job is to stay alive for the
/// process's lifetime — dropping it would drop the block (and the `tx` it
/// owns) along with it.
fn run_watcher(tx: Sender<FocusEvent>) {
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let center: *mut AnyObject = msg_send![workspace, notificationCenter];

        // A dedicated queue means delivery doesn't depend on any thread
        // (this one included) running an `NSRunLoop` — the queue manages its
        // own GCD-backed thread(s) and invokes the block there directly.
        let queue: *mut AnyObject = msg_send![class!(NSOperationQueue), new];
        let name = ns_string("NSWorkspaceDidActivateApplicationNotification");

        let block = RcBlock::new(move |notification: *mut AnyObject| {
            // This thread never runs Cocoa's usual per-iteration autorelease
            // pool (no NSRunLoop, no NSApplication), so without one here,
            // autoreleased objects created while resolving the event (the
            // NSString/NSURL churn in `focus_event_from_notification`) would
            // otherwise leak for the lifetime of the process.
            autoreleasepool(|_pool| {
                let event = unsafe { focus_event_from_notification(notification) };
                if let Some(event) = event {
                    let _ = tx.send(event);
                }
            });
        });

        let _observer: *mut AnyObject = msg_send![
            center,
            addObserverForName: name,
            object: std::ptr::null_mut::<AnyObject>(),
            queue: queue,
            usingBlock: &*block,
        ];
        // `_observer` is intentionally dropped: `NSNotificationCenter` keeps
        // its own strong reference to it until `removeObserver:` is called,
        // so nothing here needs to hold onto it for the registration to stay
        // live.
    }

    loop {
        thread::park();
    }
}

/// Extracts a [`FocusEvent`] from an `NSWorkspaceDidActivateApplicationNotification`.
///
/// Returns `None` if any expected field is missing — that's a signal to drop
/// the observation, not a bug: `NSWorkspaceApplicationKey` is documented as
/// always present on this notification, but this stays defensive rather than
/// unwrapping across an FFI boundary.
unsafe fn focus_event_from_notification(notification: *mut AnyObject) -> Option<FocusEvent> {
    if notification.is_null() {
        return None;
    }

    let user_info: *mut AnyObject = msg_send![notification, userInfo];
    if user_info.is_null() {
        return None;
    }

    let key = ns_string("NSWorkspaceApplicationKey");
    let app: *mut AnyObject = msg_send![user_info, objectForKey: key];
    if app.is_null() {
        return None;
    }

    let pid: i32 = msg_send![app, processIdentifier];
    if pid <= 0 {
        return None;
    }

    let exe_url: *mut AnyObject = msg_send![app, executableURL];
    let exe_path = ns_url_path(exe_url)?;
    let app_name = Path::new(&exe_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe_path.clone());

    Some(FocusEvent {
        app: AppInfo {
            process_id: pid as u32,
            exe_path,
            app_name,
        },
        window_title: focused_window_title(pid).unwrap_or_default(),
        timestamp: Utc::now(),
    })
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

/// Reads the title of `pid`'s focused window via the Accessibility API.
///
/// Requires this process to have been granted Accessibility permission;
/// until then `AXUIElementCopyAttributeValue` returns `kAXErrorAPIDisabled`
/// and this simply yields `None`, which callers treat as an empty title
/// rather than a hard failure.
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
    // Worst case for a UTF-16 CFString is 3 UTF-8 bytes per unit, plus a NUL.
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

/// Minimal bindings for the handful of Accessibility (`AXUIElement`) calls
/// used above. No published crate wraps these in a way that composes with
/// `core-foundation-sys`'s types, so they're declared directly against
/// ApplicationServices, matching how `windows.rs` calls its Win32 APIs
/// directly rather than through a higher-level wrapper.
mod ax {
    use core_foundation_sys::base::CFTypeRef;
    use core_foundation_sys::string::CFStringRef;

    pub type AXError = i32;
    pub type AXUIElementRef = CFTypeRef;

    pub const SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
    }
}
