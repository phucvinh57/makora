//! What the background process's main thread does while it works. On macOS it runs the AppKit
//! event loop, which the tray and the workspace notifications tracking observes both need, and the
//! work runs on a thread of its own. Elsewhere the tray runs its own loop and the work runs here.

use crate::runtime::commands::CommandSender;
use crate::runtime::tray;

/// Runs `work` to its end, with the tray in place meanwhile.
#[cfg(not(target_os = "macos"))]
pub fn run(commands: CommandSender, work: impl FnOnce() + Send + 'static) {
    let tray = tray::start(commands);
    work();
    if let Some(tray) = tray {
        tray.remove();
    }
}

/// Runs `work` to its end, with the tray in place meanwhile.
#[cfg(target_os = "macos")]
pub fn run(commands: CommandSender, work: impl FnOnce() + Send + 'static) {
    macos::run(commands, work);
}

#[cfg(target_os = "macos")]
mod macos {
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSEvent,
        NSEventModifierFlags, NSEventType,
    };
    use objc2_foundation::NSPoint;
    use tracing::{error, warn};

    use super::tray;
    use crate::runtime::commands::{CommandSender, UiCommand};

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the delegate does not implement Drop.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "MakoraAppDelegate"]
        #[ivars = CommandSender]
        struct AppDelegate;

        // SAFETY: NSObjectProtocol has no requirements.
        unsafe impl NSObjectProtocol for AppDelegate {}

        // SAFETY: NSApplicationDelegate has no requirements, and the signature matches AppKit's.
        unsafe impl NSApplicationDelegate for AppDelegate {
            // Opening Makora again from Finder, Spotlight or Launchpad reaches this process.
            #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
            fn should_handle_reopen(
                &self,
                _sender: &NSApplication,
                _has_visible_windows: bool,
            ) -> bool {
                let _ = self.ivars().try_send(UiCommand::Show);
                false
            }
        }
    );

    impl AppDelegate {
        fn new(commands: CommandSender, mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(commands);
            // SAFETY: NSObject's designated initializer.
            unsafe { msg_send![super(this), init] }
        }
    }

    pub fn run(commands: CommandSender, work: impl FnOnce() + Send + 'static) {
        let Some(mtm) = MainThreadMarker::new() else {
            error!("the background process is not on the main thread; running without a tray");
            work();
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        // No Dock icon here: the window process shows one while the window is open.
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let delegate = AppDelegate::new(commands.clone(), mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let tray = tray::start(commands);

        let worker = std::thread::Builder::new()
            .name("makora-background".into())
            .spawn(move || {
                let _stop = StopOnDrop;
                work();
            });
        match worker {
            Ok(worker) => {
                app.run();
                let _ = worker.join();
            }
            Err(err) => error!(error = %err, "could not start the background work"),
        }
        if let Some(tray) = tray {
            tray.remove();
        }
    }

    /// Ends the event loop once the work is over, even when it panicked.
    struct StopOnDrop;

    impl Drop for StopOnDrop {
        fn drop(&mut self) {
            DispatchQueue::main().exec_async(stop);
        }
    }

    /// Ends `NSApplication::run`. `stop:` takes effect once the loop handles an event, so this
    /// posts one.
    fn stop() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        app.stop(None);
        let wake = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        );
        match wake {
            Some(event) => app.postEvent_atStart(&event, false),
            None => warn!("could not wake the event loop to stop it"),
        }
    }
}
