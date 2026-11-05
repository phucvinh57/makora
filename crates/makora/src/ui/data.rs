//! Loading data without blocking the UI thread: the core call runs on a background thread and its
//! result is applied to the view on the UI thread. Keep the returned [`Task`] in a field;
//! replacing it drops the stale load before it lands.
//!
//! ```ignore
//! self.load = data::fetch(cx, |core| core.history.sessions(&query), |view, result, cx| {
//!     view.rows = result.unwrap_or_default();
//! });
//! ```

use std::sync::Arc;

use gpui_kit::{AppContext, AsyncWindowContext, Context, Task, Window};
use makora_lib::app_core::AppCore;

use crate::ui::app_state::Core;

/// Runs `load` off the UI thread, then `apply` to the view, which is notified afterwards. Nothing
/// is applied once the view is gone or the task is dropped.
pub fn fetch<V, T>(
    cx: &mut Context<V>,
    load: impl FnOnce(&AppCore) -> T + Send + 'static,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + 'static,
) -> Task<()>
where
    V: 'static,
    T: Send + 'static,
{
    let job = background(cx, load);
    cx.spawn(async move |view, cx| {
        let value = job.await;
        let _ = view.update(cx, |view, cx| {
            apply(view, value, cx);
            cx.notify();
        });
    })
}

/// [`fetch`] for an `apply` that needs the window, to show a toast or move focus.
pub fn fetch_in<V, T>(
    window: &Window,
    cx: &mut Context<V>,
    load: impl FnOnce(&AppCore) -> T + Send + 'static,
    apply: impl FnOnce(&mut V, T, &mut Window, &mut Context<V>) + 'static,
) -> Task<()>
where
    V: 'static,
    T: Send + 'static,
{
    let job = background(cx, load);
    cx.spawn_in(window, async move |view, cx: &mut AsyncWindowContext| {
        let value = job.await;
        let _ = view.update_in(cx, |view, window, cx| {
            apply(view, value, window, cx);
            cx.notify();
        });
    })
}

/// Runs a core call that blocks for long (a categorization run, a model download) on a thread of
/// its own, so it does not hold one of the executor's threads.
pub fn blocking<V, T>(
    cx: &Context<V>,
    load: impl FnOnce(&AppCore) -> T + Send + 'static,
) -> Task<Option<T>>
where
    V: 'static,
    T: Send + 'static,
{
    let core: Arc<AppCore> = Core::get(cx);
    let (tx, rx) = async_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("makora-ui-job".into())
        .spawn(move || {
            let _ = tx.send_blocking(load(&core));
        });
    if let Err(err) = spawned {
        tracing::error!(error = %err, "failed to start a background job");
    }
    cx.background_spawn(async move { rx.recv().await.ok() })
}

/// Runs a core call on a background thread; await the task from a foreground one.
pub fn background<V, T>(
    cx: &Context<V>,
    load: impl FnOnce(&AppCore) -> T + Send + 'static,
) -> Task<T>
where
    V: 'static,
    T: Send + 'static,
{
    let core: Arc<AppCore> = Core::get(cx);
    cx.background_spawn(async move { load(&core) })
}
