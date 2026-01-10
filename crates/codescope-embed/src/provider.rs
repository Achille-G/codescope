//! Execution provider abstraction for CPU/GPU

use serde::{Deserialize, Serialize};

/// Execution provider for ONNX Runtime
///
/// This enum allows switching between CPU and GPU execution
/// without changing the embedding API.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ExecutionProvider {
    /// CPU execution (default, always available)
    #[default]
    Cpu,

    /// NVIDIA CUDA execution
    Cuda {
        /// GPU device ID
        #[serde(default)]
        device_id: u32,
    },

    /// Apple CoreML execution (macOS/iOS)
    CoreML,

    /// DirectML execution (Windows)
    DirectML {
        /// GPU device ID
        #[serde(default)]
        device_id: u32,
    },
}

impl ExecutionProvider {
    /// Create CPU provider
    pub fn cpu() -> Self {
        Self::Cpu
    }

    /// Create CUDA provider
    pub fn cuda(device_id: u32) -> Self {
        Self::Cuda { device_id }
    }

    /// Create CoreML provider
    pub fn coreml() -> Self {
        Self::CoreML
    }

    /// Create DirectML provider
    pub fn directml(device_id: u32) -> Self {
        Self::DirectML { device_id }
    }

    /// Get provider name for logging
    pub fn name(&self) -> &'static str {
        match self {
            ExecutionProvider::Cpu => "CPU",
            ExecutionProvider::Cuda { .. } => "CUDA",
            ExecutionProvider::CoreML => "CoreML",
            ExecutionProvider::DirectML { .. } => "DirectML",
        }
    }

    /// Check if this provider is available on the current system
    pub fn is_available(&self) -> bool {
        match self {
            ExecutionProvider::Cpu => true,
            // For now, only CPU is guaranteed available
            // GPU availability would need runtime checks
            _ => false,
        }
    }
}

impl std::fmt::Display for ExecutionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Configuration for embedding execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedderConfig {
    /// Path to the ONNX model file
    pub model_path: std::path::PathBuf,

    /// Path to the tokenizer.json file
    pub tokenizer_path: std::path::PathBuf,

    /// Execution provider
    #[serde(default)]
    pub provider: ExecutionProvider,

    /// Batch size for inference
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Number of threads for CPU execution
    #[serde(default)]
    pub num_threads: Option<usize>,

    /// Maximum sequence length
    #[serde(default = "default_max_seq_len")]
    pub max_seq_len: usize,
}

fn default_batch_size() -> usize {
    32
}

fn default_max_seq_len() -> usize {
    512
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::new(),
            tokenizer_path: std::path::PathBuf::new(),
            provider: ExecutionProvider::default(),
            batch_size: default_batch_size(),
            num_threads: None,
            max_seq_len: default_max_seq_len(),
        }
    }
}
