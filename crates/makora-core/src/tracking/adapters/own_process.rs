//! Makora's own processes, whose windows are not tracked: this one, and the window process the
//! background process starts.

use std::sync::atomic::{AtomicU32, Ordering};

static WINDOW_PID: AtomicU32 = AtomicU32::new(0);

pub fn set_window_process(pid: Option<u32>) {
    WINDOW_PID.store(pid.unwrap_or(0), Ordering::Release);
}

pub fn is_own(pid: u32) -> bool {
    pid == std::process::id() || (pid != 0 && pid == WINDOW_PID.load(Ordering::Acquire))
}
