//! Classification infrastructure, behind the categorization
//! [`Embedder`](crate::categorization::ports::Embedder) and
//! [`Classifier`](crate::categorization::ports::Classifier) ports: a small embedding model that
//! lets the activities most like a new one be found, a local LLM that labels it given those, and
//! the downloads of both.

mod choices;
pub mod embedding;
pub mod llm;
pub mod model_download;
