//! Embedding pipeline integration for `codescope-core`.

use crate::{Project, Result};
use codescope_embed::{
    DownloadProgress, EmbedderConfig, EmbeddingPipeline, ExecutionProvider, ModelRegistry,
    OnnxEmbedder,
};
use std::path::PathBuf;

/// Resolved model artifact paths for the active project config.
#[derive(Debug, Clone)]
pub struct ResolvedEmbedding {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub config: EmbedderConfig,
}

/// Resolve the embedder config + artifact locations from project configuration.
pub fn resolve_embedding(project: &Project) -> Result<ResolvedEmbedding> {
    let config = project.config();

    let models_root =
        crate::project::models_dir().unwrap_or_else(|| project.codescope_dir().join("models"));

    let registry = ModelRegistry::new(models_root.clone());
    let model_info = registry.get(&config.embedding.model_id);

    let model_dir = match &config.embedding.model_path {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => models_root.join(path),
        None => registry.model_dir(&config.embedding.model_id),
    };

    let model_path = model_dir.join("model.onnx");
    let tokenizer_path = model_dir.join("tokenizer.json");

    let batch_size = config
        .embedding
        .batch_size
        .unwrap_or_else(|| config.profile.embed_batch_size());

    let max_seq_len = model_info.map(|m| m.max_seq_len).unwrap_or(256);

    let embedder_config = EmbedderConfig {
        model_path: model_path.clone(),
        tokenizer_path: tokenizer_path.clone(),
        provider: ExecutionProvider::Cpu,
        batch_size,
        num_threads: config.embedding.num_threads,
        max_seq_len,
    };

    Ok(ResolvedEmbedding {
        model_path,
        tokenizer_path,
        config: embedder_config,
    })
}

/// Build an embedding pipeline for the project using the configured model.
///
/// This does not download model artifacts. Ensure the ONNX model and tokenizer exist at:
/// - `<models_root>/<model_id>/model.onnx`
/// - `<models_root>/<model_id>/tokenizer.json`
pub fn build_embedding_pipeline(project: &Project) -> Result<EmbeddingPipeline> {
    let resolved = resolve_embedding(project)?;
    let embedder = OnnxEmbedder::load(
        &resolved.model_path,
        &resolved.tokenizer_path,
        &resolved.config,
    )?;
    Ok(EmbeddingPipeline::new(Box::new(embedder)).with_batch_size(resolved.config.batch_size))
}

/// Ensure the embedding model is downloaded.
///
/// Downloads the model files if they don't exist. Returns `true` if download was performed.
pub fn ensure_model_downloaded<F>(project: &Project, on_progress: Option<F>) -> Result<bool>
where
    F: FnMut(&str, DownloadProgress),
{
    let config = project.config();
    let models_root =
        crate::project::models_dir().unwrap_or_else(|| project.codescope_dir().join("models"));

    let registry = ModelRegistry::new(models_root);

    registry
        .ensure_model(&config.embedding.model_id, on_progress)
        .map_err(crate::Error::from)
}

/// Check if the embedding model is downloaded.
pub fn is_model_downloaded(project: &Project) -> bool {
    let config = project.config();
    let models_root =
        crate::project::models_dir().unwrap_or_else(|| project.codescope_dir().join("models"));

    let registry = ModelRegistry::new(models_root);
    registry.is_downloaded(&config.embedding.model_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_embedding_paths_defaults() {
        let temp = TempDir::new().unwrap();
        let project = Project::init(temp.path(), Profile::Default, false).unwrap();

        let resolved = resolve_embedding(&project).unwrap();
        assert!(resolved
            .model_path
            .ends_with(Path::new("paraphrase-multilingual-MiniLM-L12-v2").join("model.onnx")));
        assert!(resolved
            .tokenizer_path
            .ends_with(Path::new("paraphrase-multilingual-MiniLM-L12-v2").join("tokenizer.json")));
        assert_eq!(resolved.config.max_seq_len, 256);
    }
}
