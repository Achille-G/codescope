use anyhow::{Context, Result};
use codescope_embed::{EmbeddingPipeline, Embedder, EmbedderConfig, ExecutionProvider, OnnxEmbedder};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let model = args
        .next()
        .map(PathBuf::from)
        .context("Usage: embed_smoke <path/to/model.onnx> <path/to/tokenizer.json>")?;
    let tokenizer = args
        .next()
        .map(PathBuf::from)
        .context("Usage: embed_smoke <path/to/model.onnx> <path/to/tokenizer.json>")?;

    let config = EmbedderConfig {
        model_path: model.clone(),
        tokenizer_path: tokenizer.clone(),
        provider: ExecutionProvider::Cpu,
        batch_size: 32,
        num_threads: None,
        max_seq_len: 256,
    };

    let embedder = OnnxEmbedder::load(&model, &tokenizer, &config)?;
    println!("model_id: {}", embedder.model_id());
    println!("dims: {}", embedder.dimensions());
    println!("max_seq_len: {}", embedder.max_seq_len());

    let pipeline = EmbeddingPipeline::new(Box::new(embedder)).with_batch_size(config.batch_size);
    let texts = [
        "fn add(a: i32, b: i32) -> i32 { a + b }",
        "How do I add two numbers in Rust?",
        "class UserService { getUser(id) { return db.find(id) } }",
    ];

    let mut ticks = 0usize;
    let embeddings = pipeline.embed_texts_with_progress(&texts, Some(|p: codescope_embed::EmbeddingProgress| {
        ticks += 1;
        eprintln!("embedded: {}/{}", p.processed, p.total.unwrap_or(0));
    }))?;

    println!("embedded {} texts ({} progress updates)", embeddings.len(), ticks);
    for (i, emb) in embeddings.iter().enumerate() {
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        println!("text[{i}] norm: {norm:.4}");
    }

    Ok(())
}
