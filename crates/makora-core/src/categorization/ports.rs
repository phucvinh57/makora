//! What the categorization context needs from the outside world: something that turns text into
//! vectors, and something that reads a taxonomy and some activities and says which category each
//! belongs to. [`Embedder`] and [`Classifier`] are implemented in [`crate::ai`]; nothing here
//! knows about them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use crate::domain::types::Category;
use crate::shared::ids::{CategoryId, ProviderId};

use super::domain::retrieval::RelatedActivity;

/// A unit-length vector, so that the dot product of two is their cosine similarity.
pub type Embedding = Vec<f32>;

#[derive(Debug, Clone)]
pub struct ActivityText {
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
}

/// The language model a run labels with: `provider` is `None` for the local one, or the id of a
/// remote provider the classifier knows how to reach.
#[derive(Debug, Clone)]
pub struct LlmSpec {
    pub provider: Option<ProviderId>,
    pub model: String,
}

pub trait Embedder: Send + Sync {
    /// The embedding of each of `texts`, in order.
    fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Embedding>>;
}

pub trait Classifier: Send + Sync {
    /// Classifies each activity into one of the categories of `taxonomy`, reporting
    /// `(index, category)` through `on_answer` as each one is answered, not necessarily in order —
    /// the category is `None` when none fits. Returning `false` from `on_answer` stops the run,
    /// keeping what was reported so far.
    fn classify(
        &self,
        llm: &LlmSpec,
        taxonomy: &[Category],
        activities: &[ActivityText],
        related: &dyn Fn(usize) -> Vec<RelatedActivity>,
        meter: &UsageMeter,
        on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
    ) -> Result<()>;
}

/// The tokens one request to a language model took. `cached_input` is the part of `input` served
/// from a prompt cache: the provider's, or the KV cache the local model keeps between activities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub cached_input: u64,
    pub output: u64,
}

/// What a run cost: the classifier's requests, and the run's own phases. Times are in
/// milliseconds; `busy_ms` sums the latency of every request, so with requests in flight at once
/// it can exceed `llm_ms`, the wall time of the labeling phase. `prefill_ms` and `decode_ms` are
/// only known for the local model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct RunUsage {
    pub requests: u64,
    pub failed_requests: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub llm_ms: u64,
    pub busy_ms: u64,
    pub prefill_ms: u64,
    pub decode_ms: u64,
    pub embed_count: u64,
    pub embed_ms: u64,
}

/// Tallies a classifier's requests as they happen, from any number of threads.
#[derive(Debug, Default)]
pub struct UsageMeter {
    requests: AtomicU64,
    failed_requests: AtomicU64,
    input_tokens: AtomicU64,
    cached_input_tokens: AtomicU64,
    output_tokens: AtomicU64,
    busy_us: AtomicU64,
    prefill_us: AtomicU64,
    decode_us: AtomicU64,
}

impl UsageMeter {
    pub fn record(&self, tokens: TokenUsage, latency: Duration) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.add_tokens(tokens);
        add_micros(&self.busy_us, latency);
    }

    pub fn record_local(&self, tokens: TokenUsage, prefill: Duration, decode: Duration) {
        self.record(tokens, prefill + decode);
        add_micros(&self.prefill_us, prefill);
        add_micros(&self.decode_us, decode);
    }

    pub fn record_failure(&self, latency: Duration) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
        add_micros(&self.busy_us, latency);
    }

    /// The requests so far; the run-level fields (`llm_ms`, `embed_*`) are left at zero.
    pub fn snapshot(&self) -> RunUsage {
        let get = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        RunUsage {
            requests: get(&self.requests),
            failed_requests: get(&self.failed_requests),
            input_tokens: get(&self.input_tokens),
            cached_input_tokens: get(&self.cached_input_tokens),
            output_tokens: get(&self.output_tokens),
            busy_ms: get(&self.busy_us) / 1000,
            prefill_ms: get(&self.prefill_us) / 1000,
            decode_ms: get(&self.decode_us) / 1000,
            ..RunUsage::default()
        }
    }

    fn add_tokens(&self, tokens: TokenUsage) {
        self.input_tokens.fetch_add(tokens.input, Ordering::Relaxed);
        self.cached_input_tokens.fetch_add(tokens.cached_input, Ordering::Relaxed);
        self.output_tokens.fetch_add(tokens.output, Ordering::Relaxed);
    }
}

fn add_micros(counter: &AtomicU64, duration: Duration) {
    counter.fetch_add(u64::try_from(duration.as_micros()).unwrap_or(u64::MAX), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_adds_up_requests_from_many_threads() {
        let meter = UsageMeter::default();
        let tokens = TokenUsage {
            input: 100,
            cached_input: 60,
            output: 3,
        };
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..25 {
                        meter.record(tokens, Duration::from_millis(2));
                    }
                });
            }
        });
        meter.record_failure(Duration::from_millis(10));
        meter.record_local(tokens, Duration::from_millis(5), Duration::from_micros(1500));

        let usage = meter.snapshot();
        assert_eq!(usage.requests, 102);
        assert_eq!(usage.failed_requests, 1);
        assert_eq!(usage.input_tokens, 101 * 100);
        assert_eq!(usage.cached_input_tokens, 101 * 60);
        assert_eq!(usage.output_tokens, 101 * 3);
        assert_eq!(usage.busy_ms, 100 * 2 + 10 + 6);
        assert_eq!(usage.prefill_ms, 5);
        assert_eq!(usage.decode_ms, 1);
        assert_eq!(usage.llm_ms, 0);
    }
}
