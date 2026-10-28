//! Classification infrastructure, behind the categorization
//! [`Embedder`](crate::categorization::ports::Embedder) and
//! [`Classifier`](crate::categorization::ports::Classifier) ports: a small embedding model that
//! lets the activities most like a new one be found, an LLM that labels it given those — run
//! locally or by a remote provider — and the downloads of the local models.

mod choices;
pub mod embedding;
mod gguf;
pub mod llm;
pub mod model_download;
mod prompt;
pub mod remote;
pub mod router;
pub mod secrets;
