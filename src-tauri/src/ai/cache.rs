//! Models kept loaded between runs, the LLM next to the small embedding model that runs before it,
//! then released on request or after ten idle minutes, so an idle app holds no weights.
//!
//! Loading a model takes seconds, so it happens outside the lock: asking whether a model is
//! loaded, or unloading, never waits for a load. Callers asking for a model already being loaded
//! wait for that load instead of starting their own.

use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use tracing::info;

use crate::shared::sync::lock;

const IDLE_TTL: Duration = Duration::from_secs(10 * 60);
const REAPER_POLL: Duration = Duration::from_secs(15);

/// The LLM and the embedding model a run uses one after the other.
const MAX_LOADED: usize = 2;

struct Loaded<M> {
    name: String,
    model: Arc<M>,
    last_used: Instant,
}

struct Slot<M> {
    loaded: Vec<Loaded<M>>,
    loading: HashSet<String>,
    reaper_running: bool,
}

struct Inner<M> {
    slot: Mutex<Slot<M>>,
    load_finished: Condvar,
}

pub struct Cache<M> {
    inner: Arc<Inner<M>>,
}

impl<M> Clone for Cache<M> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<M> Default for Cache<M> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                slot: Mutex::new(Slot {
                    loaded: Vec::new(),
                    loading: HashSet::new(),
                    reaper_running: false,
                }),
                load_finished: Condvar::new(),
            }),
        }
    }
}

/// Takes `name` off the in-progress list however its load ends, and wakes whoever waits on it.
struct LoadingMark<'a, M> {
    inner: &'a Inner<M>,
    name: &'a str,
}

impl<M> Drop for LoadingMark<'_, M> {
    fn drop(&mut self) {
        lock(&self.inner.slot).loading.remove(self.name);
        self.inner.load_finished.notify_all();
    }
}

impl<M: Send + Sync + 'static> Cache<M> {
    fn lock(&self) -> MutexGuard<'_, Slot<M>> {
        lock(&self.inner.slot)
    }

    pub fn is_loaded(&self) -> bool {
        !self.lock().loaded.is_empty()
    }

    /// Drops the cached models. A run in progress keeps its own handles and finishes normally.
    pub fn unload(&self) {
        let mut slot = self.lock();
        if !slot.loaded.is_empty() {
            slot.loaded.clear();
            info!("models unloaded");
        }
    }

    pub fn touch(&self, name: &str) {
        if let Some(loaded) = self.lock().loaded.iter_mut().find(|l| l.name == name) {
            loaded.last_used = Instant::now();
        }
    }

    /// The model `name`, loaded with `load` unless it is cached or another caller is loading it.
    pub fn acquire_with(&self, name: &str, load: impl FnOnce() -> Result<M>) -> Result<Arc<M>> {
        let mut slot = self.lock();
        loop {
            if let Some(loaded) = slot.loaded.iter_mut().find(|l| l.name == name) {
                loaded.last_used = Instant::now();
                return Ok(loaded.model.clone());
            }
            if !slot.loading.contains(name) {
                break;
            }
            slot = self
                .inner
                .load_finished
                .wait(slot)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        slot.loading.insert(name.to_string());
        drop(slot);
        let _mark = LoadingMark {
            inner: &self.inner,
            name,
        };

        let model = Arc::new(load()?);

        let mut slot = self.lock();
        while slot.loaded.len() >= MAX_LOADED {
            let Some(oldest) = slot
                .loaded
                .iter()
                .enumerate()
                .min_by_key(|(_, loaded)| loaded.last_used)
                .map(|(position, _)| position)
            else {
                break;
            };
            let evicted = slot.loaded.swap_remove(oldest);
            info!(model = %evicted.name, "model evicted");
        }
        slot.loaded.push(Loaded {
            name: name.to_string(),
            model: model.clone(),
            last_used: Instant::now(),
        });
        if !slot.reaper_running {
            slot.reaper_running = true;
            let cache = self.clone();
            if let Err(err) = std::thread::Builder::new()
                .name("makora-model-reaper".into())
                .spawn(move || cache.reap_when_idle())
            {
                slot.reaper_running = false;
                bail!("spawning the model reaper thread: {err}");
            }
        }
        Ok(model)
    }

    fn reap_when_idle(&self) {
        loop {
            std::thread::sleep(REAPER_POLL);
            let mut slot = self.lock();
            slot.loaded.retain(|loaded| {
                // A strong count above one means a run is using it right now.
                let keep = Arc::strong_count(&loaded.model) > 1 || loaded.last_used.elapsed() < IDLE_TTL;
                if !keep {
                    info!(model = %loaded.name, "model unloaded after being idle");
                }
                keep
            });
            if slot.loaded.is_empty() {
                slot.reaper_running = false;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use anyhow::anyhow;

    use super::*;

    #[test]
    fn concurrent_callers_share_one_load() {
        let cache: Cache<usize> = Cache::default();
        let loads = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        cache
                            .acquire_with("m", || {
                                std::thread::sleep(Duration::from_millis(50));
                                Ok(loads.fetch_add(1, Ordering::SeqCst))
                            })
                            .unwrap()
                    })
                })
                .collect();
            for handle in handles {
                assert_eq!(*handle.join().unwrap(), 0);
            }
        });
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_lock_is_free_while_a_model_loads() {
        let cache: Cache<u8> = Cache::default();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let loader = scope.spawn(|| {
                cache.acquire_with("m", move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(7)
                })
            });
            started_rx.recv().unwrap();
            assert!(!cache.is_loaded());
            cache.unload();
            release_tx.send(()).unwrap();
            assert_eq!(*loader.join().unwrap().unwrap(), 7);
        });
        assert!(cache.is_loaded());
    }

    #[test]
    fn a_failed_load_lets_the_next_caller_retry() {
        let cache: Cache<u8> = Cache::default();
        assert!(cache.acquire_with("m", || Err(anyhow!("no such file"))).is_err());
        assert_eq!(*cache.acquire_with("m", || Ok(1)).unwrap(), 1);
    }

    #[test]
    fn at_most_two_models_stay_loaded() {
        let cache: Cache<u8> = Cache::default();
        for (n, name) in ["a", "b", "c"].into_iter().enumerate() {
            cache.acquire_with(name, || Ok(n as u8)).unwrap();
        }
        assert_eq!(cache.lock().loaded.len(), MAX_LOADED);
        assert_eq!(*cache.acquire_with("c", || Ok(99)).unwrap(), 2);
    }
}
