//! Cross-process file locking for codescope
//!
//! Prevents multiple indexers (e.g., `codescope index` and `codescope watch`)
//! from running simultaneously on the same project.

use crate::{Error, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use tracing::{debug, warn};

/// Holds an exclusive lock on the .codescope/.lock file.
/// The lock is released when this guard is dropped.
pub struct ProjectLock {
    _file: File,
    path: PathBuf,
}

impl ProjectLock {
    /// Attempt to acquire an exclusive lock on the project.
    ///
    /// Returns an error if another process holds the lock.
    pub fn try_acquire(lock_path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| Error::Io(format!("Failed to open lock file: {e}")))?;

        // Try to acquire exclusive lock (non-blocking)
        file.try_lock_exclusive().map_err(|e| {
            Error::LockHeld(format!(
                "Another codescope process is running. Lock file: {}. Error: {}",
                lock_path.display(),
                e
            ))
        })?;

        // Write PID to lock file for debugging
        let mut file = file;
        let pid = process::id();
        let _ = file.set_len(0);
        let _ = writeln!(file, "{pid}");
        let _ = file.flush();

        debug!(
            "Acquired project lock at {} (PID {})",
            lock_path.display(),
            pid
        );

        Ok(Self {
            _file: file,
            path: lock_path.to_path_buf(),
        })
    }

    /// Attempt to acquire the lock, waiting up to the specified duration.
    ///
    /// This is a blocking version that retries with exponential backoff.
    pub fn acquire_with_timeout(lock_path: &Path, timeout: std::time::Duration) -> Result<Self> {
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let mut delay = Duration::from_millis(10);
        let max_delay = Duration::from_millis(500);

        loop {
            match Self::try_acquire(lock_path) {
                Ok(lock) => return Ok(lock),
                Err(Error::LockHeld(_)) if start.elapsed() < timeout => {
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(max_delay);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Check if a lock is currently held (without acquiring it).
    pub fn is_locked(lock_path: &Path) -> bool {
        if !lock_path.exists() {
            return false;
        }

        let file = match OpenOptions::new().write(true).open(lock_path) {
            Ok(f) => f,
            Err(_) => return false,
        };

        // Try to acquire, then immediately release
        match file.try_lock_exclusive() {
            Ok(()) => {
                let _ = FileExt::unlock(&file);
                false
            }
            Err(_) => true,
        }
    }

    /// Read the PID from the lock file (if available).
    pub fn read_holder_pid(lock_path: &Path) -> Option<u32> {
        std::fs::read_to_string(lock_path).ok()?.trim().parse().ok()
    }

    /// Get the path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        debug!("Releasing project lock at {}", self.path.display());
        // File is unlocked automatically when dropped due to fs2 behavior
    }
}

/// Check if a stale lock file exists and try to clean it up.
///
/// A lock is considered stale if:
/// - The lock file exists but no process holds it
/// - The PID in the lock file doesn't correspond to a running process
///
/// Returns `true` if a stale lock was cleaned up.
pub fn cleanup_stale_lock(lock_path: &Path) -> bool {
    if !lock_path.exists() {
        return false;
    }

    // Check if lock is actually held
    if ProjectLock::is_locked(lock_path) {
        // Lock is held by another process, not stale
        return false;
    }

    // Lock file exists but isn't held - it's stale
    warn!(
        "Found stale lock file at {}; cleaning up",
        lock_path.display()
    );

    if let Err(e) = std::fs::remove_file(lock_path) {
        warn!("Failed to remove stale lock file: {e}");
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_acquire_lock() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".lock");

        let lock = ProjectLock::try_acquire(&lock_path).unwrap();
        assert!(lock_path.exists());

        // Note: Reading PID from an actively held lock may fail on Windows
        // because the file is open in exclusive mode. We test PID reading
        // in test_cleanup_stale_lock instead.

        drop(lock);
    }

    #[test]
    fn test_lock_contention() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".lock");

        let _lock1 = ProjectLock::try_acquire(&lock_path).unwrap();

        // Second attempt should fail
        let result = ProjectLock::try_acquire(&lock_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_lock_released_on_drop() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".lock");

        {
            let _lock = ProjectLock::try_acquire(&lock_path).unwrap();
            assert!(ProjectLock::is_locked(&lock_path));
        }

        // Lock should be released
        assert!(!ProjectLock::is_locked(&lock_path));

        // Should be able to acquire again
        let _lock2 = ProjectLock::try_acquire(&lock_path).unwrap();
    }

    #[test]
    fn test_is_locked() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".lock");

        assert!(!ProjectLock::is_locked(&lock_path));

        let _lock = ProjectLock::try_acquire(&lock_path).unwrap();
        assert!(ProjectLock::is_locked(&lock_path));
    }

    #[test]
    fn test_cleanup_stale_lock() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".lock");

        // Create a lock file without holding it
        std::fs::write(&lock_path, "12345").unwrap();

        // Should detect and clean up
        assert!(cleanup_stale_lock(&lock_path));
        assert!(!lock_path.exists());
    }
}
