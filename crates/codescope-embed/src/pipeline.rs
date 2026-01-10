//! Embedding pipeline utilities (batching, preprocessing, progress callbacks).

use crate::{preprocess::preprocess_batch, BoxedEmbedder, Error, Result};

/// Progress information emitted during embedding.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddingProgress {
    pub processed: usize,
    pub total: Option<usize>,
}

/// High-level embedding pipeline: preprocess -> batch -> inference.
pub struct EmbeddingPipeline {
    embedder: BoxedEmbedder,
    batch_size: usize,
    preprocess_max_chars: usize,
}

impl std::fmt::Debug for EmbeddingPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingPipeline")
            .field("batch_size", &self.batch_size)
            .field("preprocess_max_chars", &self.preprocess_max_chars)
            .finish_non_exhaustive()
    }
}

impl EmbeddingPipeline {
    pub fn new(embedder: BoxedEmbedder) -> Self {
        Self {
            embedder,
            batch_size: 32,
            preprocess_max_chars: 8 * 1024,
        }
    }

    pub fn model_id(&self) -> &str {
        self.embedder.model_id()
    }

    pub fn dimensions(&self) -> usize {
        self.embedder.dimensions()
    }

    pub fn max_seq_len(&self) -> usize {
        self.embedder.max_seq_len()
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    pub fn with_preprocess_max_chars(mut self, max_chars: usize) -> Self {
        self.preprocess_max_chars = max_chars;
        self
    }

    pub fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.embed_texts_with_progress(texts, None::<fn(EmbeddingProgress)>)
    }

    pub fn embed_texts_with_progress<F>(&self, texts: &[&str], mut on_progress: Option<F>) -> Result<Vec<Vec<f32>>>
    where
        F: FnMut(EmbeddingProgress),
    {
        let mut out = Vec::with_capacity(texts.len());
        self.embed_texts_streaming_with_progress(
            texts,
            &mut on_progress,
            |embedding| out.push(embedding),
        )?;
        Ok(out)
    }

    pub fn embed_texts_streaming_with_progress<F, C>(
        &self,
        texts: &[&str],
        on_progress: &mut Option<F>,
        mut consume: C,
    ) -> Result<usize>
    where
        F: FnMut(EmbeddingProgress),
        C: FnMut(Vec<f32>),
    {
        let total = Some(texts.len());
        let mut processed = 0usize;

        for batch in texts.chunks(self.batch_size) {
            let preprocessed = preprocess_batch(batch, self.preprocess_max_chars);
            let refs: Vec<&str> = preprocessed.iter().map(|s| s.as_str()).collect();

            let embeddings = self.embedder.embed(&refs)?;
            if embeddings.len() != batch.len() {
                return Err(Error::Inference(format!(
                    "Embedder returned {} embeddings for batch of size {}",
                    embeddings.len(),
                    batch.len()
                )));
            }

            for embedding in embeddings {
                consume(embedding);
            }

            processed += batch.len();
            if let Some(cb) = on_progress.as_mut() {
                cb(EmbeddingProgress { processed, total });
            }
        }

        Ok(processed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OnnxEmbedder;

    #[test]
    fn test_pipeline_batches_and_progress() {
        let embedder: BoxedEmbedder = Box::new(OnnxEmbedder::mock(8));
        let pipeline = EmbeddingPipeline::new(embedder).with_batch_size(2);

        let texts = ["a", "b", "c", "d", "e"];

        let mut calls = 0usize;
        let mut last_processed = 0usize;
        let embeddings = pipeline
            .embed_texts_with_progress(&texts, Some(|p: EmbeddingProgress| {
                calls += 1;
                last_processed = p.processed;
                assert_eq!(p.total, Some(texts.len()));
            }))
            .unwrap();

        assert_eq!(embeddings.len(), texts.len());
        assert!(calls >= 3); // 5 items @ batch size 2 => 3 batches
        assert_eq!(last_processed, texts.len());
    }

    #[test]
    fn test_streaming_does_not_require_output_vec() {
        let embedder: BoxedEmbedder = Box::new(OnnxEmbedder::mock(8));
        let pipeline = EmbeddingPipeline::new(embedder).with_batch_size(3);

        let texts = ["one", "two", "three", "four"];
        let mut count = 0usize;
        pipeline
            .embed_texts_streaming_with_progress(&texts, &mut None::<fn(EmbeddingProgress)>, |_| {
                count += 1;
            })
            .unwrap();
        assert_eq!(count, texts.len());
    }
}
