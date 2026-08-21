use anyhow::{Context, Result};
use model2vec_rs::model::StaticModel;

const MODEL_ID: &str = "minishlab/potion-base-8M@bf8b056651a2c21b8d2565580b8569da283cab23";

/// The small interface the classifier needs from an embedding backend.
///
/// Keeping this separate from `model2vec-rs` makes it possible to try a larger local
/// sentence model later without changing the classifier or its SQLite cache.
pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

pub struct Model2Vec {
    model: StaticModel,
    model_id: String,
    dim: usize,
}

impl Model2Vec {
    pub fn new() -> Result<Self> {
        let model = StaticModel::from_bytes(
            include_bytes!("../../resources/potion-base-8M/tokenizer.json").as_ref(),
            include_bytes!("../../resources/potion-base-8M/model.safetensors").as_ref(),
            include_bytes!("../../resources/potion-base-8M/config.json").as_ref(),
            Some(true),
        )
        .context("loading embedded potion-base-8M model")?;
        let dim = model.encode_single("makora").len();
        if dim == 0 {
            anyhow::bail!("embedded potion-base-8M model returned an empty vector");
        }
        Ok(Self {
            model,
            model_id: MODEL_ID.to_string(),
            dim,
        })
    }
}

impl Embedder for Model2Vec {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let vectors = self.model.encode(texts);
        if vectors.len() != texts.len()
            || vectors.iter().any(|vector| {
                vector.len() != self.dim || vector.iter().any(|value| !value.is_finite())
            })
        {
            anyhow::bail!("embedding backend returned vectors with an unexpected shape");
        }
        Ok(vectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_model_returns_normalized_vectors() {
        let model = Model2Vec::new().expect("embedded model should load");
        let vectors = model
            .embed(&["github software repository".to_string()])
            .unwrap();
        assert_eq!(vectors.len(), 1);
        assert_eq!(vectors[0].len(), model.dim());
        let norm = vectors[0]
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }
}
