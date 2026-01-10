//! Project management for codescope
//!
//! Handles .codescope/ directory structure and initialization

use crate::{Config, Error, Profile, Result};
use std::path::{Path, PathBuf};

const CODESCOPE_DIR: &str = ".codescope";
const CONFIG_FILE: &str = "config.toml";
const META_DB: &str = "meta.sqlite";
const HNSW_INDEX: &str = "hnsw.index";
const TANTIVY_DIR: &str = "tantivy";
const LOCK_FILE: &str = ".lock";

/// Represents an initialized codescope project
#[derive(Debug)]
pub struct Project {
    /// Root directory of the project (where .codescope/ lives)
    root: PathBuf,
    /// Loaded configuration
    config: Config,
}

impl Project {
    /// Initialize a new codescope project in the given directory
    pub fn init(root: &Path, profile: Profile, force: bool) -> Result<Self> {
        let codescope_dir = root.join(CODESCOPE_DIR);

        if codescope_dir.exists() && !force {
            return Err(Error::AlreadyInitialized(root.to_path_buf()));
        }

        // Create directory structure
        std::fs::create_dir_all(&codescope_dir)?;
        std::fs::create_dir_all(codescope_dir.join(TANTIVY_DIR))?;

        // Create config
        let mut config = Config::default();
        config.profile = profile;
        config.save(&codescope_dir.join(CONFIG_FILE))?;

        // Create empty SQLite database
        Self::init_database(&codescope_dir.join(META_DB))?;

        tracing::info!("Initialized codescope project at {}", root.display());

        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }

    /// Open an existing codescope project
    pub fn open(root: &Path) -> Result<Self> {
        let codescope_dir = root.join(CODESCOPE_DIR);

        if !codescope_dir.exists() {
            return Err(Error::NotInitialized);
        }

        let config = Config::load(&codescope_dir.join(CONFIG_FILE))?;

        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }

    /// Find the project root by walking up from the given path
    pub fn find(start: &Path) -> Result<Self> {
        let mut current = start.to_path_buf();

        loop {
            if current.join(CODESCOPE_DIR).exists() {
                return Self::open(&current);
            }

            if !current.pop() {
                return Err(Error::NotInitialized);
            }
        }
    }

    /// Get the project root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the .codescope directory path
    pub fn codescope_dir(&self) -> PathBuf {
        self.root.join(CODESCOPE_DIR)
    }

    /// Get the config
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get mutable config reference
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Get path to SQLite metadata database
    pub fn meta_db_path(&self) -> PathBuf {
        self.codescope_dir().join(META_DB)
    }

    /// Get path to HNSW index file
    pub fn hnsw_index_path(&self) -> PathBuf {
        self.codescope_dir().join(HNSW_INDEX)
    }

    /// Get path to Tantivy index directory
    pub fn tantivy_dir(&self) -> PathBuf {
        self.codescope_dir().join(TANTIVY_DIR)
    }

    /// Get path to lock file
    pub fn lock_file_path(&self) -> PathBuf {
        self.codescope_dir().join(LOCK_FILE)
    }

    /// Save the current configuration
    pub fn save_config(&self) -> Result<()> {
        self.config.save(&self.codescope_dir().join(CONFIG_FILE))
    }

    /// Clean all index data (keeps config)
    pub fn clean(&self) -> Result<()> {
        let codescope_dir = self.codescope_dir();

        // Remove database
        let db_path = codescope_dir.join(META_DB);
        if db_path.exists() {
            std::fs::remove_file(&db_path)?;
        }

        // Remove HNSW index
        let hnsw_path = codescope_dir.join(HNSW_INDEX);
        if hnsw_path.exists() {
            std::fs::remove_file(&hnsw_path)?;
        }

        // Remove Tantivy directory
        let tantivy_path = codescope_dir.join(TANTIVY_DIR);
        if tantivy_path.exists() {
            std::fs::remove_dir_all(&tantivy_path)?;
            std::fs::create_dir_all(&tantivy_path)?;
        }

        // Recreate empty database
        Self::init_database(&db_path)?;

        tracing::info!("Cleaned codescope index");
        Ok(())
    }

    /// Initialize the SQLite database schema
    fn init_database(path: &Path) -> Result<()> {
        // Initialize schema via the storage layer.
        // Dropping the connection is enough; schema changes are persisted.
        let _storage = codescope_search::Storage::open(path)?;
        Ok(())
    }
}

/// Get the global codescope directory (~/.codescope/)
pub fn global_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codescope"))
}

/// Get the models directory (~/.codescope/models/)
pub fn models_dir() -> Option<PathBuf> {
    global_dir().map(|d| d.join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_project_init() {
        let temp = TempDir::new().unwrap();
        let project = Project::init(temp.path(), Profile::Default, false).unwrap();

        assert!(project.codescope_dir().exists());
        assert!(project.codescope_dir().join(CONFIG_FILE).exists());
        assert!(project.tantivy_dir().exists());
    }

    #[test]
    fn test_project_open() {
        let temp = TempDir::new().unwrap();
        Project::init(temp.path(), Profile::Light, false).unwrap();

        let project = Project::open(temp.path()).unwrap();
        assert_eq!(project.config().profile, Profile::Light);
    }

    #[test]
    fn test_project_find() {
        let temp = TempDir::new().unwrap();
        let subdir = temp.path().join("src").join("nested");
        std::fs::create_dir_all(&subdir).unwrap();

        Project::init(temp.path(), Profile::Default, false).unwrap();

        let project = Project::find(&subdir).unwrap();
        assert_eq!(project.root(), temp.path());
    }

    #[test]
    fn test_project_not_initialized() {
        let temp = TempDir::new().unwrap();
        let result = Project::open(temp.path());
        assert!(matches!(result, Err(Error::NotInitialized)));
    }
}
