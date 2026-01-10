//! Change detection for incremental indexing

use crate::walker::FileEntry;
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, trace};
use xxhash_rust::xxh3::xxh3_64;

/// Detected changes in the file system
#[derive(Debug, Default)]
pub struct Changes {
    /// New files that don't exist in the index
    pub added: Vec<PathBuf>,
    /// Files that have been modified since last index
    pub modified: Vec<PathBuf>,
    /// Files that were indexed but no longer exist
    pub deleted: Vec<PathBuf>,
}

impl Changes {
    /// Check if there are any changes
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    /// Total number of changes
    pub fn total(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }

    /// Files that need to be (re)indexed
    pub fn files_to_index(&self) -> impl Iterator<Item = &PathBuf> {
        self.added.iter().chain(self.modified.iter())
    }
}

/// Stored file state for change detection
#[derive(Debug, Clone)]
pub struct FileState {
    /// Relative path from project root
    pub path: String,
    /// XXH3 hash of file content
    pub content_hash: u64,
    /// File modification time (Unix timestamp)
    pub mtime: i64,
    /// File size in bytes
    pub size: u64,
}

/// Change detector that tracks file states in SQLite
pub struct ChangeDetector {
    conn: Connection,
    project_root: PathBuf,
}

impl ChangeDetector {
    /// Open or create the change detector database
    pub fn open(db_path: &Path, project_root: PathBuf) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let detector = Self { conn, project_root };
        detector.init_schema()?;
        Ok(detector)
    }

    /// Open an in-memory database (for testing)
    pub fn open_memory(project_root: PathBuf) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let detector = Self { conn, project_root };
        detector.init_schema()?;
        Ok(detector)
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS file_states (
                path TEXT PRIMARY KEY,
                content_hash INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                size INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_file_states_mtime ON file_states(mtime);
            "#,
        )?;
        Ok(())
    }

    /// Detect changes between current files and stored state
    pub fn detect_changes(&self, files: &[FileEntry]) -> Result<Changes> {
        let mut changes = Changes::default();

        // Get all currently tracked paths
        let tracked_paths = self.get_all_paths()?;
        let mut seen_paths: HashSet<String> = HashSet::new();

        for file in files {
            let rel_path = self.relative_path(&file.path);
            seen_paths.insert(rel_path.clone());

            match self.get_file_state(&rel_path)? {
                None => {
                    // New file
                    trace!("New file: {}", rel_path);
                    changes.added.push(file.path.clone());
                }
                Some(stored) => {
                    // Check if modified using mtime optimization
                    let current_mtime = get_mtime(&file.path).unwrap_or(0);

                    if current_mtime != stored.mtime || file.size != stored.size {
                        // mtime or size changed, verify with hash
                        let current_hash = self.compute_hash(&file.path)?;

                        if current_hash != stored.content_hash {
                            trace!("Modified file: {}", rel_path);
                            changes.modified.push(file.path.clone());
                        }
                    }
                }
            }
        }

        // Find deleted files
        for tracked_path in tracked_paths {
            if !seen_paths.contains(&tracked_path) {
                trace!("Deleted file: {}", tracked_path);
                changes.deleted.push(self.project_root.join(&tracked_path));
            }
        }

        debug!(
            "Detected changes: {} added, {} modified, {} deleted",
            changes.added.len(),
            changes.modified.len(),
            changes.deleted.len()
        );

        Ok(changes)
    }

    /// Update the stored state for a file
    pub fn update_file_state(&self, path: &Path) -> Result<()> {
        let rel_path = self.relative_path(path);
        let hash = self.compute_hash(path)?;
        let mtime = get_mtime(path).unwrap_or(0);
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        self.conn.execute(
            r#"
            INSERT INTO file_states (path, content_hash, mtime, size)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(path) DO UPDATE SET
                content_hash = excluded.content_hash,
                mtime = excluded.mtime,
                size = excluded.size
            "#,
            params![rel_path, hash as i64, mtime, size as i64],
        )?;

        Ok(())
    }

    /// Remove a file from tracking
    pub fn remove_file(&self, path: &Path) -> Result<()> {
        let rel_path = self.relative_path(path);
        self.conn
            .execute("DELETE FROM file_states WHERE path = ?1", params![rel_path])?;
        Ok(())
    }

    /// Clear all tracked files
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM file_states", [])?;
        Ok(())
    }

    /// Get the number of tracked files
    pub fn file_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM file_states", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get file state by path
    fn get_file_state(&self, rel_path: &str) -> Result<Option<FileState>> {
        let state = self
            .conn
            .query_row(
                "SELECT path, content_hash, mtime, size FROM file_states WHERE path = ?1",
                params![rel_path],
                |row| {
                    Ok(FileState {
                        path: row.get(0)?,
                        content_hash: row.get::<_, i64>(1)? as u64,
                        mtime: row.get(2)?,
                        size: row.get::<_, i64>(3)? as u64,
                    })
                },
            )
            .optional()?;
        Ok(state)
    }

    /// Get all tracked paths
    fn get_all_paths(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM file_states")?;
        let paths: HashSet<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// Compute XXH3 hash of file content
    fn compute_hash(&self, path: &Path) -> Result<u64> {
        let content = fs::read(path)?;
        Ok(xxh3_64(&content))
    }

    /// Convert absolute path to relative path
    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/") // Normalize path separators
    }
}

/// Get file modification time as Unix timestamp
fn get_mtime(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_env() -> (TempDir, ChangeDetector) {
        let dir = TempDir::new().unwrap();
        let detector = ChangeDetector::open_memory(dir.path().to_path_buf()).unwrap();
        (dir, detector)
    }

    #[test]
    fn test_detect_new_files() {
        let (dir, detector) = create_test_env();

        // Create a file
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "fn main() {}").unwrap();

        let files = vec![FileEntry::new(file_path.clone(), 12)];
        let changes = detector.detect_changes(&files).unwrap();

        assert_eq!(changes.added.len(), 1);
        assert_eq!(changes.modified.len(), 0);
        assert_eq!(changes.deleted.len(), 0);
    }

    #[test]
    fn test_detect_modified_files() {
        let (dir, detector) = create_test_env();

        // Create and track a file
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "fn main() {}").unwrap();
        detector.update_file_state(&file_path).unwrap();

        // Verify no changes
        let files = vec![FileEntry::new(file_path.clone(), 12)];
        let changes = detector.detect_changes(&files).unwrap();
        assert!(changes.is_empty());

        // Modify the file
        sleep(Duration::from_millis(10)); // Ensure mtime changes
        fs::write(&file_path, "fn main() { println!(\"hello\"); }").unwrap();

        let files = vec![FileEntry::new(
            file_path.clone(),
            fs::metadata(&file_path).unwrap().len(),
        )];
        let changes = detector.detect_changes(&files).unwrap();

        assert_eq!(changes.added.len(), 0);
        assert_eq!(changes.modified.len(), 1);
        assert_eq!(changes.deleted.len(), 0);
    }

    #[test]
    fn test_detect_deleted_files() {
        let (dir, detector) = create_test_env();

        // Create and track a file
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "fn main() {}").unwrap();
        detector.update_file_state(&file_path).unwrap();

        // Delete the file
        fs::remove_file(&file_path).unwrap();

        // Detect changes with empty file list
        let changes = detector.detect_changes(&[]).unwrap();

        assert_eq!(changes.added.len(), 0);
        assert_eq!(changes.modified.len(), 0);
        assert_eq!(changes.deleted.len(), 1);
    }

    #[test]
    fn test_no_changes_when_unchanged() {
        let (dir, detector) = create_test_env();

        // Create and track a file
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, "fn main() {}").unwrap();
        detector.update_file_state(&file_path).unwrap();

        // Check for changes without modifying
        let files = vec![FileEntry::new(file_path, 12)];
        let changes = detector.detect_changes(&files).unwrap();

        assert!(changes.is_empty());
    }

    #[test]
    fn test_file_count() {
        let (dir, detector) = create_test_env();

        assert_eq!(detector.file_count().unwrap(), 0);

        let file1 = dir.path().join("a.rs");
        let file2 = dir.path().join("b.rs");
        fs::write(&file1, "a").unwrap();
        fs::write(&file2, "b").unwrap();

        detector.update_file_state(&file1).unwrap();
        detector.update_file_state(&file2).unwrap();

        assert_eq!(detector.file_count().unwrap(), 2);

        detector.remove_file(&file1).unwrap();
        assert_eq!(detector.file_count().unwrap(), 1);
    }
}
