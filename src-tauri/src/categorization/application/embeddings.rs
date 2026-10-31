//! The embedding of every activity, kept in the database so that finding the activities most like
//! another is a read, not a model run. Vectors are made once, as activities are saved
//! ([`super::indexing`]), and what is still missing is caught up before an AI run reads them.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{bail, Result};
use diesel::Connection;
use tracing::warn;

use crate::categorization::domain::retrieval::{activity_text, Candidate, RelatedActivity};
use crate::categorization::ports::{Embedder, Embedding};
use crate::db::{self, Db};
use crate::shared::ids::{raw, ActivityId};
use crate::shared::sync::lock;
use crate::tracking::domain::source_key::{source_url, url_location, KeyKind};

/// Texts embedded at a time while catching up.
const EMBED_BATCH: usize = 64;
/// Ids looked up in one query, well below SQLite's cap on bound variables.
const LOOKUP_CHUNK: usize = 500;

#[derive(Debug, Clone)]
pub struct ActivityToEmbed {
    pub id: ActivityId,
    pub text: String,
}

pub trait EmbeddingSvc: Send + Sync {
    /// Up to `limit` activities with no vector from `model`, the most recently created first.
    fn get_missing(&self, model: &str, limit: usize) -> Result<Vec<ActivityToEmbed>>;
    fn save(&self, model: &str, vectors: &[(ActivityId, Embedding)]) -> Result<()>;
    /// The vectors from `model` of those among `activity_ids` that have one.
    fn get_vectors(&self, model: &str, activity_ids: &[ActivityId]) -> Result<HashMap<ActivityId, Embedding>>;
    /// Every activity the user has filed, by pin or by rule, that has a vector from `model`.
    fn get_candidates(&self, model: &str) -> Result<Vec<Candidate>>;
}

pub struct SqliteEmbeddings {
    db: Db,
}

impl SqliteEmbeddings {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl EmbeddingSvc for SqliteEmbeddings {
    fn get_missing(&self, model: &str, limit: usize) -> Result<Vec<ActivityToEmbed>> {
        let mut conn = self.db.read()?;
        let rows = db::activity_embeddings::ActivityEmbeddingsRepo::get_missing(&mut *conn, model, limit as i64)?;
        Ok(rows
            .into_iter()
            .map(|row| ActivityToEmbed {
                id: row.id.into(),
                text: activity_text(
                    &row.source_name,
                    location(row.source_kind, &row.source_key, &row.path).as_deref(),
                    &row.title,
                ),
            })
            .collect())
    }

    fn save(&self, model: &str, vectors: &[(ActivityId, Embedding)]) -> Result<()> {
        let encoded: Vec<(i64, Vec<u8>)> = vectors.iter().map(|(id, vector)| ((*id).into(), encode(vector))).collect();
        let mut conn = self.db.write();
        conn.transaction(|conn| db::activity_embeddings::ActivityEmbeddingsRepo::upsert_many(conn, model, &encoded))?;
        Ok(())
    }

    fn get_vectors(&self, model: &str, activity_ids: &[ActivityId]) -> Result<HashMap<ActivityId, Embedding>> {
        let mut conn = self.db.read()?;
        let mut out = HashMap::with_capacity(activity_ids.len());
        for chunk in raw(activity_ids).chunks(LOOKUP_CHUNK) {
            for (id, bytes) in db::activity_embeddings::ActivityEmbeddingsRepo::get_many(&mut *conn, model, chunk)? {
                out.insert(id.into(), decode(&bytes)?);
            }
        }
        Ok(out)
    }

    fn get_candidates(&self, model: &str) -> Result<Vec<Candidate>> {
        let mut conn = self.db.read()?;
        db::activity_embeddings::ActivityEmbeddingsRepo::get_categorized(&mut *conn, model)?
            .into_iter()
            .map(|row| {
                Ok(Candidate {
                    activity_id: row.activity_id.into(),
                    activity: RelatedActivity {
                        app_name: row.source_name,
                        url: location(row.source_kind, &row.source_key, &row.path),
                        title: row.title,
                        category: row.category_id.into(),
                    },
                    vector: decode(&row.vector)?,
                })
            })
            .collect()
    }
}

/// Activities whose text the embedding model fails on, by model, so that one bad text cannot stop
/// every later pass. Kept in memory only: they are tried again after a restart.
#[derive(Default)]
pub struct EmbedSkips(Mutex<HashMap<String, HashSet<ActivityId>>>);

impl EmbedSkips {
    fn get(&self, model: &str) -> HashSet<ActivityId> {
        lock(&self.0).get(model).cloned().unwrap_or_default()
    }

    fn add(&self, model: &str, id: ActivityId) {
        lock(&self.0).entry(model.to_string()).or_default().insert(id);
    }
}

/// Embeds every activity that has no vector from `model` yet, a batch at a time, and returns how
/// many it did. Safe to run alongside itself: a vector made twice is just replaced.
///
/// A batch that fails is tried again one activity at a time; the ones that still fail while
/// others succeed are left out from then on. When none succeeds the model itself is the
/// problem, and that is the error. `cancelled` is checked between batches.
pub fn embed_missing(
    embeddings: &dyn EmbeddingSvc,
    embedder: &dyn Embedder,
    model: &str,
    skips: &EmbedSkips,
    cancelled: &dyn Fn() -> bool,
) -> Result<usize> {
    let mut total = 0;
    loop {
        if cancelled() {
            return Ok(total);
        }
        let skipped = skips.get(model);
        let batch: Vec<ActivityToEmbed> = embeddings
            .get_missing(model, EMBED_BATCH + skipped.len())?
            .into_iter()
            .filter(|activity| !skipped.contains(&activity.id))
            .take(EMBED_BATCH)
            .collect();
        if batch.is_empty() {
            return Ok(total);
        }
        let vectors = match embed_batch(embedder, model, &batch) {
            Ok(vectors) => vectors,
            Err(err) => {
                warn!(error = format!("{err:#}"), %model, "embedding a batch failed; retrying one activity at a time");
                embed_one_by_one(embedder, model, &batch, skips).map_err(|_| err)?
            }
        };
        embeddings.save(model, &vectors)?;
        total += vectors.len();
    }
}

fn embed_one_by_one(
    embedder: &dyn Embedder,
    model: &str,
    batch: &[ActivityToEmbed],
    skips: &EmbedSkips,
) -> Result<Vec<(ActivityId, Embedding)>> {
    let mut vectors = Vec::with_capacity(batch.len());
    let mut failed = Vec::new();
    for activity in batch {
        match embed_batch(embedder, model, std::slice::from_ref(activity)) {
            Ok(vector) => vectors.extend(vector),
            Err(err) => failed.push((activity.id, err)),
        }
    }
    if vectors.is_empty() {
        bail!("no activity of the batch could be embedded");
    }
    for (id, err) in failed {
        warn!(activity = %id, error = format!("{err:#}"), %model, "skipping an activity the embedding model fails on");
        skips.add(model, id);
    }
    Ok(vectors)
}

fn embed_batch(embedder: &dyn Embedder, model: &str, batch: &[ActivityToEmbed]) -> Result<Vec<(ActivityId, Embedding)>> {
    let texts: Vec<String> = batch.iter().map(|activity| activity.text.clone()).collect();
    let vectors = embedder.embed(model, &texts)?;
    if vectors.len() != texts.len() {
        bail!("the embedding model returned {} vectors for {} texts", vectors.len(), texts.len());
    }
    Ok(batch.iter().map(|activity| activity.id).zip(vectors).collect())
}

/// `domain/path` of the page an activity is on, `None` for an app.
fn location(kind: KeyKind, key: &str, path: &str) -> Option<String> {
    (kind == KeyKind::Domain)
        .then(|| url_location(&source_url(key, path)))
        .flatten()
}

fn encode(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn decode(bytes: &[u8]) -> Result<Embedding> {
    let (floats, rest) = bytes.as_chunks::<4>();
    if !rest.is_empty() {
        bail!("a stored embedding is {} bytes long, not a whole number of floats", bytes.len());
    }
    Ok(floats.iter().map(|float| f32::from_le_bytes(*float)).collect())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use anyhow::anyhow;

    use super::*;

    struct Store {
        missing: Mutex<Vec<ActivityToEmbed>>,
    }

    impl Store {
        fn new(texts: &[&str]) -> Self {
            let missing = texts
                .iter()
                .enumerate()
                .map(|(id, text)| ActivityToEmbed {
                    id: ActivityId(id as i64),
                    text: text.to_string(),
                })
                .collect();
            Self {
                missing: Mutex::new(missing),
            }
        }
    }

    impl EmbeddingSvc for Store {
        fn get_missing(&self, _model: &str, limit: usize) -> Result<Vec<ActivityToEmbed>> {
            Ok(lock(&self.missing).iter().take(limit).cloned().collect())
        }

        fn save(&self, _model: &str, vectors: &[(ActivityId, Embedding)]) -> Result<()> {
            lock(&self.missing).retain(|activity| vectors.iter().all(|(id, _)| *id != activity.id));
            Ok(())
        }

        fn get_vectors(&self, _model: &str, _ids: &[ActivityId]) -> Result<HashMap<ActivityId, Embedding>> {
            unreachable!()
        }

        fn get_candidates(&self, _model: &str) -> Result<Vec<Candidate>> {
            unreachable!()
        }
    }

    /// Fails on any text containing "bad", or on everything when `broken`.
    struct Model {
        broken: bool,
    }

    impl Embedder for Model {
        fn embed(&self, _model: &str, texts: &[String]) -> Result<Vec<Embedding>> {
            if self.broken || texts.iter().any(|text| text.contains("bad")) {
                return Err(anyhow!("cannot embed"));
            }
            Ok(texts.iter().map(|_| vec![1.0]).collect())
        }
    }

    #[test]
    fn a_text_the_model_fails_on_is_skipped_from_then_on() {
        let store = Store::new(&["one", "bad", "two"]);
        let skips = EmbedSkips::default();
        let model = Model { broken: false };
        assert_eq!(embed_missing(&store, &model, "m", &skips, &|| false).unwrap(), 2);
        assert_eq!(skips.get("m"), HashSet::from([ActivityId(1)]));
        assert_eq!(embed_missing(&store, &model, "m", &skips, &|| false).unwrap(), 0);
        assert!(skips.get("other").is_empty());
    }

    #[test]
    fn a_broken_model_is_an_error_and_skips_nothing() {
        let store = Store::new(&["one", "two"]);
        let skips = EmbedSkips::default();
        assert!(embed_missing(&store, &Model { broken: true }, "m", &skips, &|| false).is_err());
        assert!(skips.get("m").is_empty());
    }

    #[test]
    fn a_cancelled_pass_stops_between_batches() {
        let store = Store::new(&["one"; EMBED_BATCH + 1]);
        let skips = EmbedSkips::default();
        let checks = Cell::new(0);
        let cancelled = || {
            checks.set(checks.get() + 1);
            checks.get() > 1
        };
        let done = embed_missing(&store, &Model { broken: false }, "m", &skips, &cancelled).unwrap();
        assert_eq!(done, EMBED_BATCH);
        assert_eq!(lock(&store.missing).len(), 1);
    }
}
