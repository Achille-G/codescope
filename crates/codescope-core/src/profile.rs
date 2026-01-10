//! Resource profiles for different machine capabilities

use serde::{Deserialize, Serialize};

/// Resource profile controlling threading, batching, and memory usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Conservative settings for ~8GB RAM machines
    Light,

    /// Balanced configuration for typical dev machines
    #[default]
    Default,

    /// Aggressive settings for 16+ core machines
    Heavy,
}

impl Profile {
    /// Number of threads for parallel operations
    pub fn thread_count(&self) -> usize {
        let cpus = num_cpus();
        match self {
            Profile::Light => (cpus / 4).max(1),
            Profile::Default => (cpus / 2).max(1),
            Profile::Heavy => cpus,
        }
    }

    /// Batch size for embedding operations
    pub fn embed_batch_size(&self) -> usize {
        match self {
            Profile::Light => 16,
            Profile::Default => 32,
            Profile::Heavy => 64,
        }
    }

    /// Number of candidates for ANN search
    pub fn ann_top_k(&self) -> usize {
        match self {
            Profile::Light => 50,
            Profile::Default => 100,
            Profile::Heavy => 200,
        }
    }

    /// File walker channel buffer size
    pub fn walker_buffer_size(&self) -> usize {
        match self {
            Profile::Light => 50,
            Profile::Default => 100,
            Profile::Heavy => 200,
        }
    }

    /// HNSW ef_construction parameter
    pub fn hnsw_ef_construction(&self) -> usize {
        match self {
            Profile::Light => 100,
            Profile::Default => 200,
            Profile::Heavy => 400,
        }
    }

    /// HNSW M parameter (max connections per node)
    pub fn hnsw_m(&self) -> usize {
        match self {
            Profile::Light => 16,
            Profile::Default => 32,
            Profile::Heavy => 48,
        }
    }
}

impl std::str::FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "light" => Ok(Profile::Light),
            "default" => Ok(Profile::Default),
            "heavy" => Ok(Profile::Heavy),
            _ => Err(format!(
                "Unknown profile: {}. Use light, default, or heavy.",
                s
            )),
        }
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Profile::Light => write!(f, "light"),
            Profile::Default => write!(f, "default"),
            Profile::Heavy => write!(f, "heavy"),
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_parse() {
        assert_eq!("light".parse::<Profile>().unwrap(), Profile::Light);
        assert_eq!("default".parse::<Profile>().unwrap(), Profile::Default);
        assert_eq!("heavy".parse::<Profile>().unwrap(), Profile::Heavy);
        assert_eq!("LIGHT".parse::<Profile>().unwrap(), Profile::Light);
    }

    #[test]
    fn test_profile_thread_count() {
        let light = Profile::Light;
        let heavy = Profile::Heavy;
        assert!(light.thread_count() <= heavy.thread_count());
    }
}
