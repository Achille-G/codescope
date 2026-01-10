//! ONNX Runtime embedder implementation.
//!
//! This supports sentence-transformer-style models exported to ONNX (e.g. MiniLM).
//! If you don't have model artifacts yet, `OnnxEmbedder::mock` can be used for
//! deterministic placeholder embeddings.

use crate::embedder::normalize;
use crate::{EmbedTokenizer, Embedder, EmbedderConfig, Error, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// ONNX Runtime based embedder
///
/// This is the primary embedding implementation for codescope.
/// It loads a sentence-transformer compatible ONNX model and tokenizer.
pub struct OnnxEmbedder {
    dimensions: usize,
    max_seq_len: usize,
    model_id: String,
    session: Option<parking_lot::Mutex<Session>>,
    tokenizer: Option<EmbedTokenizer>,
    output_name: Option<String>,
}

impl OnnxEmbedder {
    /// Load an embedder from model and tokenizer paths
    pub fn load(model_path: &Path, tokenizer_path: &Path, config: &EmbedderConfig) -> Result<Self> {
        if !model_path.exists() {
            return Err(Error::ModelNotFound(model_path.display().to_string()));
        }
        if !tokenizer_path.exists() {
            return Err(Error::ModelNotFound(tokenizer_path.display().to_string()));
        }

        let model_id = model_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("mock-model")
            .to_string();

        if !config.provider.is_available() {
            return Err(Error::UnsupportedProvider(config.provider.to_string()));
        }
        match config.provider {
            crate::ExecutionProvider::Cpu => {}
            _ => {
                // Keep the API GPU-ready, but V1 is CPU-only.
                return Err(Error::UnsupportedProvider(config.provider.to_string()));
            }
        }

        let tokenizer = EmbedTokenizer::from_file(tokenizer_path, config.max_seq_len)?;

        // NOTE: applications may choose to configure the global ORT environment with `ort::init()`;
        // we avoid doing that here because `codescope-embed` is a library.
        let mut builder = Session::builder()?;
        if let Some(num_threads) = config.num_threads {
            builder = builder.with_intra_threads(num_threads)?;
        }
        builder = builder.with_memory_pattern(false)?;

        let session = builder.commit_from_file(model_path)?;

        let (output_name, dimensions) = pick_output(&session).unwrap_or((None, 384));

        Ok(Self {
            dimensions,
            max_seq_len: config.max_seq_len,
            model_id,
            session: Some(parking_lot::Mutex::new(session)),
            tokenizer: Some(tokenizer),
            output_name,
        })
    }

    /// Create a mock embedder for testing
    pub fn mock(dimensions: usize) -> Self {
        Self {
            dimensions,
            max_seq_len: 512,
            model_id: "mock".to_string(),
            session: None,
            tokenizer: None,
            output_name: None,
        }
    }
}

impl Embedder for OnnxEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if self.session.is_none() {
            return embed_mock(texts, self.dimensions);
        }

        let tokenizer = self
            .tokenizer
            .as_ref()
            .ok_or_else(|| Error::ModelLoad("Tokenizer not initialized".to_string()))?;

        let mut session = self
            .session
            .as_ref()
            .ok_or_else(|| Error::ModelLoad("Session not initialized".to_string()))?
            .lock();

        let encoding = tokenizer.encode_batch(texts)?;
        if encoding.is_empty() {
            return Ok(vec![]);
        }

        let [batch_size, seq_len] = encoding.shape();
        let shape = [batch_size, seq_len];

        let input_ids_name = find_input_name(session.inputs(), "input_ids")
            .ok_or_else(|| Error::Inference("Model missing input_ids".to_string()))?
            .to_string();
        let attention_mask_name = find_input_name(session.inputs(), "attention_mask")
            .ok_or_else(|| Error::Inference("Model missing attention_mask".to_string()))?
            .to_string();
        let token_type_name =
            find_input_name(session.inputs(), "token_type_ids").map(|s| s.to_string());

        let mut inputs: Vec<(&str, Tensor<i64>)> = Vec::with_capacity(3);
        inputs.push((
            input_ids_name.as_str(),
            Tensor::from_array((shape, encoding.input_ids))?,
        ));
        inputs.push((
            attention_mask_name.as_str(),
            Tensor::from_array((shape, encoding.attention_mask.clone()))?,
        ));

        if let Some(name) = token_type_name.as_deref() {
            let token_type_ids = encoding
                .token_type_ids
                .unwrap_or_else(|| vec![0; batch_size * seq_len]);
            inputs.push((name, Tensor::from_array((shape, token_type_ids))?));
        }

        let outputs = session.run(inputs)?;
        let output = pick_output_value(&outputs, self.output_name.as_deref());

        let (out_shape, out_data) = output.try_extract_tensor::<f32>()?;
        let embeddings = if out_shape.len() == 2 {
            let batch = out_shape[0] as usize;
            let dim = out_shape[1] as usize;
            if dim != self.dimensions {
                return Err(Error::Inference(format!(
                    "Model output dimension {dim} does not match expected {}",
                    self.dimensions
                )));
            }
            if batch != batch_size {
                return Err(Error::Inference(format!(
                    "Model output batch {batch} does not match input batch {batch_size}"
                )));
            }

            out_data
                .chunks_exact(dim)
                .map(|row| row.to_vec())
                .collect::<Vec<_>>()
        } else if out_shape.len() == 3 {
            // Mean pooling over token embeddings using the attention mask.
            let batch = out_shape[0] as usize;
            let seq = out_shape[1] as usize;
            let dim = out_shape[2] as usize;
            if dim != self.dimensions {
                return Err(Error::Inference(format!(
                    "Model output dimension {dim} does not match expected {}",
                    self.dimensions
                )));
            }
            if batch != batch_size || seq != seq_len {
                return Err(Error::Inference(format!(
                    "Model output shape [{batch}, {seq}] does not match input [{batch_size}, {seq_len}]"
                )));
            }
            mean_pool_embeddings(out_data, &encoding.attention_mask, batch, seq, dim)
        } else {
            return Err(Error::Inference(format!(
                "Unsupported output rank {} (expected 2 or 3)",
                out_shape.len()
            )));
        };

        Ok(embeddings
            .into_iter()
            .map(|mut emb| {
                normalize(&mut emb);
                emb
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn max_seq_len(&self) -> usize {
        self.max_seq_len
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

fn embed_mock(texts: &[&str], dimensions: usize) -> Result<Vec<Vec<f32>>> {
    tracing::debug!("Mock embedding {} texts", texts.len());
    Ok(texts
        .iter()
        .map(|text| {
            let mut embedding = vec![0.0f32; dimensions];
            let bytes = text.as_bytes();
            for (i, &b) in bytes.iter().take(dimensions).enumerate() {
                embedding[i] = (b as f32 / 255.0) - 0.5;
            }
            normalize(&mut embedding);
            embedding
        })
        .collect())
}

fn find_input_name<'a>(inputs: &'a [ort::value::Outlet], key: &str) -> Option<&'a str> {
    inputs
        .iter()
        .map(|o| o.name())
        .find(|name| *name == key)
        .or_else(|| {
            inputs
                .iter()
                .map(|o| o.name())
                .find(|name| name.contains(key))
        })
}

fn pick_output(session: &Session) -> Option<(Option<String>, usize)> {
    let mut best_name: Option<String> = None;

    for candidate in ["sentence_embedding", "embedding", "embeddings"] {
        if let Some(out) = session
            .outputs()
            .iter()
            .find(|o| o.name().contains(candidate))
        {
            best_name = Some(out.name().to_string());
            break;
        }
    }

    let chosen = if let Some(name) = best_name.as_deref() {
        session.outputs().iter().find(|o| o.name() == name)
    } else {
        session.outputs().first()
    }?;

    let dims = match chosen.dtype() {
        ort::value::ValueType::Tensor { shape, .. } if shape.len() == 2 && shape[1] > 0 => {
            shape[1] as usize
        }
        ort::value::ValueType::Tensor { shape, .. } if shape.len() == 3 && shape[2] > 0 => {
            shape[2] as usize
        }
        _ => 384,
    };

    Some((best_name, dims))
}

fn pick_output_value<'o, 'r>(
    outputs: &'o ort::session::SessionOutputs<'r>,
    preferred: Option<&str>,
) -> &'o ort::value::DynValue {
    if let Some(name) = preferred {
        if let Some(v) = outputs.get(name) {
            return v;
        }
    }

    for candidate in ["sentence_embedding", "embedding", "embeddings"] {
        if let Some((name, _)) = outputs
            .keys()
            .find(|name| name.contains(candidate))
            .map(|name| (name, ()))
        {
            return outputs.get(name).expect("key exists");
        }
    }

    &outputs[0]
}

fn mean_pool_embeddings(
    last_hidden: &[f32],
    attention_mask: &[i64],
    batch: usize,
    seq: usize,
    dim: usize,
) -> Vec<Vec<f32>> {
    let mut pooled = vec![0.0f32; batch * dim];
    let mut counts = vec![0.0f32; batch];

    for b in 0..batch {
        for t in 0..seq {
            let mask = attention_mask[b * seq + t];
            if mask == 0 {
                continue;
            }
            counts[b] += 1.0;
            let base = (b * seq + t) * dim;
            for d in 0..dim {
                pooled[b * dim + d] += last_hidden[base + d];
            }
        }
    }

    for b in 0..batch {
        if counts[b] > 0.0 {
            for d in 0..dim {
                pooled[b * dim + d] /= counts[b];
            }
        }
    }

    pooled.chunks_exact(dim).map(|row| row.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_embedder() {
        let embedder = OnnxEmbedder::mock(384);
        let texts = vec!["hello world", "goodbye"];
        let embeddings = embedder.embed(&texts).unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);
        assert_eq!(embeddings[1].len(), 384);

        // Check normalization
        let norm: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_deterministic_embedding() {
        let embedder = OnnxEmbedder::mock(384);
        let e1 = embedder.embed(&["test"]).unwrap();
        let e2 = embedder.embed(&["test"]).unwrap();

        // Same input should give same output
        assert_eq!(e1[0], e2[0]);
    }

    #[test]
    fn test_onnx_embedder_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OnnxEmbedder>();
    }
}
