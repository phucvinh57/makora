//! Embedding activities as they are saved, on a thread of its own so that tracking never waits
//! for a model. One pass embeds everything still missing, so the same pass backfills activities
//! recorded before there was an embedding model, retries after a failure, and catches up after
//! the model is changed.

use std::io;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use tracing::{info, warn};

use crate::categorization::ports::Embedder;
use crate::settings::SettingsStore;

use super::embeddings::{embed_missing, EmbedSkips, EmbeddingSvc};

pub trait ActivityIndexer: Send + Sync {
    /// Embeds, in the background, every activity that has no embedding from the configured model.
    /// Returns at once; requests that arrive during a pass are served by a single one after it.
    fn refresh(&self);
}

pub struct BackgroundIndexer {
    wake: Sender<()>,
}

impl BackgroundIndexer {
    pub fn spawn(
        embeddings: Arc<dyn EmbeddingSvc>,
        embedder: Arc<dyn Embedder>,
        settings: Arc<dyn SettingsStore>,
        skips: Arc<EmbedSkips>,
    ) -> io::Result<Self> {
        let (wake, requests) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("makora-activity-embedder".into())
            .spawn(move || {
                // The same failure — a model that is not downloaded yet — would otherwise be
                // logged for every activity saved.
                let mut last_error: Option<String> = None;
                for () in requests.iter() {
                    while requests.try_recv().is_ok() {}
                    let model = settings.get().ai.embedding_model;
                    match embed_missing(&*embeddings, &*embedder, &model, &skips, &|| false) {
                        Ok(count) => {
                            last_error = None;
                            if count > 0 {
                                info!(count, %model, "embedded activities");
                            }
                        }
                        Err(err) => {
                            let message = format!("{err:#}");
                            if last_error.as_ref() != Some(&message) {
                                warn!(error = %message, %model, "embedding activities");
                                last_error = Some(message);
                            }
                        }
                    }
                }
            })?;
        Ok(Self { wake })
    }
}

impl ActivityIndexer for BackgroundIndexer {
    fn refresh(&self) {
        let _ = self.wake.send(());
    }
}
