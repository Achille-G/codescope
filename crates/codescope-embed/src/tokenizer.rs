//! Tokenizer abstraction for embedding models.
//!
//! This wraps the `tokenizers` crate with a small, model-agnostic API that
//! produces the tensors most transformer ONNX models expect.

use crate::{Error, Result};
use std::path::Path;
use tokenizers::tokenizer::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};
use tokenizers::utils::truncation::TruncationStrategy;

/// Batched tokenizer output for transformer models.
#[derive(Debug, Clone)]
pub struct BatchEncoding {
    /// Flattened `[batch, seq]` input ids.
    pub input_ids: Vec<i64>,
    /// Flattened `[batch, seq]` attention mask (1 = real token, 0 = padding).
    pub attention_mask: Vec<i64>,
    /// Flattened `[batch, seq]` token type ids (segment ids), if provided by the tokenizer.
    pub token_type_ids: Option<Vec<i64>>,
    /// Batch size.
    pub batch_size: usize,
    /// Sequence length after padding/truncation.
    pub seq_len: usize,
}

impl BatchEncoding {
    pub fn shape(&self) -> [usize; 2] {
        [self.batch_size, self.seq_len]
    }

    pub fn len(&self) -> usize {
        self.batch_size
    }

    pub fn is_empty(&self) -> bool {
        self.batch_size == 0
    }
}

/// Tokenizer wrapper configured for embedding inference.
#[derive(Clone)]
pub struct EmbedTokenizer {
    inner: Tokenizer,
    max_seq_len: usize,
}

impl std::fmt::Debug for EmbedTokenizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbedTokenizer")
            .field("max_seq_len", &self.max_seq_len)
            .finish_non_exhaustive()
    }
}

impl EmbedTokenizer {
    /// Load a tokenizer from a `tokenizer.json` file and configure truncation + padding.
    pub fn from_file(path: &Path, max_seq_len: usize) -> Result<Self> {
        let mut inner =
            Tokenizer::from_file(path).map_err(|e| Error::Tokenizer(e.to_string()))?;

        inner
            .with_truncation(Some(TruncationParams {
                max_length: max_seq_len,
                strategy: TruncationStrategy::LongestFirst,
                ..Default::default()
            }))
            .map_err(|e| Error::Tokenizer(e.to_string()))?;

        // Use fixed-length padding so ONNX models with static shapes work reliably.
        inner.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(max_seq_len),
            ..Default::default()
        }));

        Ok(Self { inner, max_seq_len })
    }

    pub fn max_seq_len(&self) -> usize {
        self.max_seq_len
    }

    /// Encode a batch of texts into `[batch, seq]` ids/masks.
    pub fn encode_batch(&self, texts: &[&str]) -> Result<BatchEncoding> {
        if texts.is_empty() {
            return Ok(BatchEncoding {
                input_ids: vec![],
                attention_mask: vec![],
                token_type_ids: Some(vec![]),
                batch_size: 0,
                seq_len: self.max_seq_len,
            });
        }

        let encodings = self
            .inner
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| Error::Tokenizer(e.to_string()))?;

        let batch_size = encodings.len();
        let seq_len = encodings
            .first()
            .map(|e| e.get_ids().len())
            .unwrap_or(self.max_seq_len);

        let mut input_ids = Vec::with_capacity(batch_size * seq_len);
        let mut attention_mask = Vec::with_capacity(batch_size * seq_len);
        let mut token_type_ids = Vec::with_capacity(batch_size * seq_len);

        for encoding in &encodings {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let types = encoding.get_type_ids();

            if ids.len() != seq_len || mask.len() != seq_len || types.len() != seq_len {
                return Err(Error::Tokenizer(
                    "Tokenizer produced inconsistent sequence lengths in a batch".to_string(),
                ));
            }

            input_ids.extend(ids.iter().map(|x| *x as i64));
            attention_mask.extend(mask.iter().map(|x| *x as i64));
            token_type_ids.extend(types.iter().map(|x| *x as i64));
        }

        let token_type_ids = if token_type_ids.is_empty() {
            None
        } else {
            Some(token_type_ids)
        };

        Ok(BatchEncoding {
            input_ids,
            attention_mask,
            token_type_ids,
            batch_size,
            seq_len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    fn test_tokenizer(max_seq_len: usize) -> EmbedTokenizer {
        let vocab: HashMap<String, u32> = [
            ("<unk>", 0),
            ("hello", 1),
            ("world", 2),
            ("goodbye", 3),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();

        let model = WordLevel::builder()
            .vocab(vocab)
            .unk_token("<unk>".to_string())
            .build()
            .unwrap();

        let mut inner = Tokenizer::new(model);
        inner.with_pre_tokenizer(Some(Whitespace::default()));
        inner
            .with_truncation(Some(TruncationParams {
                max_length: max_seq_len,
                strategy: TruncationStrategy::LongestFirst,
                ..Default::default()
            }))
            .unwrap();
        inner.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(max_seq_len),
            ..Default::default()
        }));

        EmbedTokenizer { inner, max_seq_len }
    }

    #[test]
    fn test_encode_batch_fixed_padding() {
        let tok = test_tokenizer(8);
        let enc = tok.encode_batch(&["hello world", "goodbye"]).unwrap();
        assert_eq!(enc.shape(), [2, 8]);
        assert_eq!(enc.input_ids.len(), 16);
        assert_eq!(enc.attention_mask.len(), 16);

        let mask_sum: i64 = enc.attention_mask.iter().sum();
        assert!(mask_sum > 0);
    }

    #[test]
    fn test_encode_batch_truncates() {
        let tok = test_tokenizer(4);
        let enc = tok.encode_batch(&["hello world hello world"]).unwrap();
        assert_eq!(enc.shape(), [1, 4]);
        assert_eq!(enc.attention_mask.iter().sum::<i64>(), 4);
    }
}

