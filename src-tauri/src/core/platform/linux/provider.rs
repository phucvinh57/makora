//! The seam between "how this desktop reports focus" and "what makora does
//! with it".
//!
//! GNOME and wlroots-style compositors disagree about almost everything —
//! transport, identifiers, whether a process id is available at all — but
//! both can answer the one question the tracker needs: *which toplevel is
//! focused right now*. A provider answers exactly that and nothing else;
//! turning the answer into an [`AppInfo`](crate::core::model::AppInfo) is
//! [`super::proc`]'s job, so every provider produces identical session
//! identity for the same app.

/// One "this toplevel is now focused" observation, in the terms a Wayland
/// desktop speaks.
///
/// Only emitted when something *is* focused: a provider that learns focus
/// was dropped entirely (GNOME does this whenever the Activities overview
/// opens) stays silent rather than reporting an empty observation, so the
/// open session survives a trip through the overview — matching Windows,
/// where opening the Start menu is not a focus change either.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowObservation {
    /// The owning process, when the source can supply one. GNOME can (via
    /// `Meta.Window.get_pid`); `wlr-foreign-toplevel-management-v1` carries
    /// no pid at all, which is why this is optional and why
    /// [`super::proc`] has an app-id fallback.
    pub pid: Option<u32>,
    /// The desktop-file id (`org.gnome.Nautilus.desktop`) or Wayland
    /// `app_id` (`firefox`), depending on the provider.
    pub app_id: String,
    /// X11-style `WM_CLASS`, when known. A fallback for `app_id`.
    pub wm_class: String,
    pub title: String,
}

/// A source of focused-toplevel observations for one Linux desktop stack.
///
/// Deliberately not object-safe: provider selection happens once at startup
/// in [`super::select_and_run`] against a fixed, statically known list, so
/// there is nothing to gain from dynamic dispatch.
pub trait ToplevelProvider {
    /// Shown in the startup log line so it's obvious which path is live.
    const NAME: &'static str;

    /// Cheap "is this the right provider for the running session?" check.
    /// Must not block and must not panic when the desktop is absent.
    fn probe() -> Option<Self>
    where
        Self: Sized;

    /// Block for the lifetime of the process, calling `emit` on every focus
    /// change. Returning at all means the provider gave up permanently.
    fn run(self, emit: &mut dyn FnMut(WindowObservation));
}
