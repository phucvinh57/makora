//! Sentence embeddings from a GGUF embedding model (`all-minilm`, `nomic-embed-text`, ...),
//! through the same llama.cpp that runs the LLM and the same model cache and downloads.
//! Embedding models are tiny next to a chat model: tens of megabytes, a few milliseconds a text
//! on a CPU.

use std::num::NonZeroU32;

use anyhow::{anyhow, bail, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::token::LlamaToken;

use crate::categorization::ports::{Embedder, Embedding};

use super::llm::{backend, ModelCache};

/// The embedding model a fresh install uses; a small model in the Ollama library.
pub const DEFAULT_MODEL: &str = "all-minilm";

/// Longest text embedded, in tokens; also the batch size, which an embedding model must be able
/// to take a whole text in. Longer texts are cut.
const MAX_TOKENS: u32 = 512;
/// Texts embedded by one decode.
const SEQUENCES_PER_BATCH: u32 = 32;

pub struct LlamaEmbedder {
    cache: ModelCache,
}

impl LlamaEmbedder {
    pub fn new(cache: ModelCache) -> Self {
        Self { cache }
    }
}

impl Embedder for LlamaEmbedder {
    fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Embedding>> {
        embed_all(&self.cache, model, texts)
    }
}

fn embed_all(cache: &ModelCache, name: &str, texts: &[String]) -> Result<Vec<Embedding>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let backend = backend()?;
    let model = cache.acquire(name)?;
    let result = run(backend, &model, texts);
    drop(model);
    cache.touch(name);
    result
}

fn run(
    backend: &llama_cpp_2::llama_backend::LlamaBackend,
    model: &LlamaModel,
    texts: &[String],
) -> Result<Vec<Embedding>> {
    let n_ctx = model.n_ctx_train().clamp(1, MAX_TOKENS);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 8) as i32;
    let mut ctx = model
        .new_context(
            backend,
            LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(n_ctx * SEQUENCES_PER_BATCH))
                .with_n_batch(n_ctx * SEQUENCES_PER_BATCH)
                .with_n_ubatch(n_ctx * SEQUENCES_PER_BATCH)
                .with_n_seq_max(SEQUENCES_PER_BATCH)
                .with_embeddings(true)
                .with_n_threads(threads)
                .with_n_threads_batch(threads),
        )
        .map_err(|err| anyhow!("creating embedding context: {err}"))?;
    let capacity = (n_ctx * SEQUENCES_PER_BATCH) as usize;
    let mut batch = LlamaBatch::new(capacity, SEQUENCES_PER_BATCH as i32);

    let tokenized: Vec<Vec<LlamaToken>> = texts
        .iter()
        .map(|text| {
            let mut tokens = model.str_to_token(text, AddBos::Always)?;
            tokens.truncate(n_ctx as usize);
            Ok(tokens)
        })
        .collect::<Result<_>>()?;

    let mut out: Vec<Embedding> = Vec::with_capacity(texts.len());
    let mut start = 0;
    while start < tokenized.len() {
        // As many whole texts as one decode takes.
        let mut end = start;
        let mut used = 0;
        while end < tokenized.len()
            && end - start < SEQUENCES_PER_BATCH as usize
            && used + tokenized[end].len() <= capacity
        {
            used += tokenized[end].len();
            end += 1;
        }
        batch.clear();
        for (seq, tokens) in tokenized[start..end].iter().enumerate() {
            batch.add_sequence(tokens, seq as i32, true)?;
        }
        ctx.clear_kv_cache();
        ctx.decode(&mut batch)
            .map_err(|err| anyhow!("embedding texts: {err}"))?;
        for seq in 0..end - start {
            let vector = ctx
                .embeddings_seq_ith(seq as i32)
                .map_err(|err| anyhow!("reading embedding: {err}"))?;
            out.push(normalized(vector)?);
        }
        start = end;
    }
    Ok(out)
}

fn normalized(vector: &[f32]) -> Result<Embedding> {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        bail!("the model produced an empty embedding; is it an embedding model?");
    }
    Ok(vector.iter().map(|x| x / norm).collect())
}
