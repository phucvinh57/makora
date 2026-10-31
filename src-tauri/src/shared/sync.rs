use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use tracing::warn;

/// Locks `mutex`, taking it over from a thread that panicked while holding it: every value kept
/// behind these locks stays valid between statements, so one failed caller must not take down
/// every later one.
pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        warn!("recovering a lock poisoned by a panic");
        mutex.clear_poison();
        poisoned.into_inner()
    })
}

pub fn read<T: ?Sized>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| {
        warn!("recovering a lock poisoned by a panic");
        lock.clear_poison();
        poisoned.into_inner()
    })
}

pub fn write<T: ?Sized>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|poisoned| {
        warn!("recovering a lock poisoned by a panic");
        lock.clear_poison();
        poisoned.into_inner()
    })
}

/// Proof that a job holds its `running` flag; dropping it clears the flag however the job ends,
/// panics included.
pub struct RunGuard<'a>(&'a AtomicBool);

impl<'a> RunGuard<'a> {
    /// Sets `running`, or `None` when it was already set.
    pub fn start(running: &'a AtomicBool) -> Option<Self> {
        (!running.swap(true, Ordering::AcqRel)).then_some(Self(running))
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_recovers_a_poisoned_mutex() {
        let mutex = Mutex::new(1);
        let _ = std::panic::catch_unwind(|| {
            let mut guard = mutex.lock().unwrap();
            *guard = 2;
            panic!("poison it");
        });
        assert!(mutex.is_poisoned());
        assert_eq!(*lock(&mutex), 2);
        assert!(!mutex.is_poisoned());
    }

    #[test]
    fn write_recovers_a_poisoned_rwlock() {
        let rw = RwLock::new(1);
        let _ = std::panic::catch_unwind(|| {
            let mut guard = rw.write().unwrap();
            *guard = 3;
            panic!("poison it");
        });
        *write(&rw) += 1;
        assert_eq!(*read(&rw), 4);
    }

    #[test]
    fn run_guard_admits_one_job_at_a_time() {
        let running = AtomicBool::new(false);
        let first = RunGuard::start(&running);
        assert!(first.is_some());
        assert!(RunGuard::start(&running).is_none());
        drop(first);
        assert!(!running.load(Ordering::Acquire));
        assert!(RunGuard::start(&running).is_some());
    }

    #[test]
    fn run_guard_clears_the_flag_on_panic() {
        let running = AtomicBool::new(false);
        let _ = std::panic::catch_unwind(|| {
            let _guard = RunGuard::start(&running).unwrap();
            panic!("job failed");
        });
        assert!(!running.load(Ordering::Acquire));
    }
}
