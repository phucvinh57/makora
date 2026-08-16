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

use crate::core::model::{AppInfo, FocusEvent, WatchEvent};
use crate::core::watcher::FocusWatcher;

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

fn run_watcher(tx: Sender<WatchEvent>) {
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let center: *mut AnyObject = msg_send![workspace, notificationCenter];

        let queue: *mut AnyObject = msg_send![class!(NSOperationQueue), new];
        let name = ns_string("NSWorkspaceDidActivateApplicationNotification");

        let block = RcBlock::new(move |notification: *mut AnyObject| {
            autoreleasepool(|_pool| {
                let event = focus_event_from_notification(notification);
                if let Some(event) = event {
                    let _ = tx.send(event.into());
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
    }

    loop {
        thread::park();
    }
}

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

    if pid == std::process::id() as i32 {
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
        pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        pub fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
    }
}
