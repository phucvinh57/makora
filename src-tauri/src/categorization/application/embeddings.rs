//! The embedding of every activity, kept in the database so that finding the activities most like
//! another is a read, not a model run. Vectors are made once, as activities are saved
//! ([`super::indexing`]), and what is still missing is caught up before an AI run reads them.

use std::collections::HashMap;

use anyhow::{bail, Result};
use diesel::Connection;

use crate::categorization::domain::retrieval::{activity_text, Candidate, RelatedActivity};
use crate::categorization::ports::{Embedder, Embedding};
use crate::db::{self, Db};
use crate::shared::ids::{raw, ActivityId};
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
                    location(&row.source_kind, &row.source_key, &row.path).as_deref(),
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
                        url: location(&row.source_kind, &row.source_key, &row.path),
                        title: row.title,
                        category: row.category_id.into(),
                    },
                    vector: decode(&row.vector)?,
                })
            })
            .collect()
    }
}

/// Embeds every activity that has no vector from `model` yet, a batch at a time, and returns how
/// many it did. Safe to run alongside itself: a vector made twice is just replaced.
pub fn embed_missing(embeddings: &dyn EmbeddingSvc, embedder: &dyn Embedder, model: &str) -> Result<usize> {
    let mut total = 0;
    loop {
        let batch = embeddings.get_missing(model, EMBED_BATCH)?;
        if batch.is_empty() {
            return Ok(total);
        }
        let texts: Vec<String> = batch.iter().map(|activity| activity.text.clone()).collect();
        let vectors = embedder.embed(model, &texts)?;
        if vectors.len() != texts.len() {
            bail!("the embedding model returned {} vectors for {} texts", vectors.len(), texts.len());
        }
        let vectors: Vec<(ActivityId, Embedding)> = batch.iter().map(|activity| activity.id).zip(vectors).collect();
        embeddings.save(model, &vectors)?;
        total += batch.len();
    }
}

/// `domain/path` of the page an activity is on, `None` for an app.
fn location(kind: &str, key: &str, path: &str) -> Option<String> {
    (KeyKind::from_db(kind) == KeyKind::Domain)
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
