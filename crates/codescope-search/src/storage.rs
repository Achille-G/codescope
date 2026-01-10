//! SQLite storage for metadata

use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// SQLite-based metadata storage
pub struct Storage {
    conn: Connection,
}

impl Storage {
    /// Open or create a storage database
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Open an in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS files (
                file_id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                lang TEXT,
                file_hash BLOB NOT NULL,
                size_bytes INTEGER,
                indexed_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chunks (
                chunk_id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
                symbol TEXT,
                kind TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                content_hash BLOB NOT NULL,
                content TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tombstones (
                chunk_id INTEGER PRIMARY KEY,
                deleted_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY,
                value BLOB
            );

            CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file_id);
            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_chunks_symbol ON chunks(symbol);
            "#,
        )?;
        Ok(())
    }

    /// Insert or update a file record
    pub fn upsert_file(
        &self,
        path: &str,
        lang: Option<&str>,
        file_hash: &[u8],
        size_bytes: i64,
    ) -> Result<i64> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            r#"
            INSERT INTO files (path, lang, file_hash, size_bytes, indexed_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(path) DO UPDATE SET
                lang = excluded.lang,
                file_hash = excluded.file_hash,
                size_bytes = excluded.size_bytes,
                indexed_at = excluded.indexed_at
            "#,
            params![path, lang, file_hash, size_bytes, now],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get file ID by path
    pub fn get_file_id(&self, path: &str) -> Result<Option<i64>> {
        let id = self
            .conn
            .query_row(
                "SELECT file_id FROM files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id)
    }

    /// Get file hash by path
    pub fn get_file_hash(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let hash = self
            .conn
            .query_row(
                "SELECT file_hash FROM files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(hash)
    }

    /// Delete a file and its chunks
    pub fn delete_file(&self, path: &str) -> Result<()> {
        // Get file_id first
        if let Some(file_id) = self.get_file_id(path)? {
            // Mark chunks as tombstones
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            self.conn.execute(
                r#"
                INSERT INTO tombstones (chunk_id, deleted_at)
                SELECT chunk_id, ?1 FROM chunks WHERE file_id = ?2
                "#,
                params![now, file_id],
            )?;

            // Delete file (cascades to chunks)
            self.conn
                .execute("DELETE FROM files WHERE file_id = ?1", params![file_id])?;
        }
        Ok(())
    }

    /// Insert a chunk
    pub fn insert_chunk(
        &self,
        file_id: i64,
        symbol: Option<&str>,
        kind: &str,
        start_line: u32,
        end_line: u32,
        content_hash: &[u8],
        content: &str,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO chunks (file_id, symbol, kind, start_line, end_line, content_hash, content)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                file_id,
                symbol,
                kind,
                start_line,
                end_line,
                content_hash,
                content
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Delete chunks for a file
    pub fn delete_chunks_for_file(&self, file_id: i64) -> Result<Vec<i64>> {
        // Get chunk IDs before deleting
        let mut stmt = self
            .conn
            .prepare("SELECT chunk_id FROM chunks WHERE file_id = ?1")?;
        let chunk_ids: Vec<i64> = stmt
            .query_map(params![file_id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Mark as tombstones
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        for &chunk_id in &chunk_ids {
            self.conn.execute(
                "INSERT OR IGNORE INTO tombstones (chunk_id, deleted_at) VALUES (?1, ?2)",
                params![chunk_id, now],
            )?;
        }

        // Delete chunks
        self.conn
            .execute("DELETE FROM chunks WHERE file_id = ?1", params![file_id])?;

        Ok(chunk_ids)
    }

    /// Get chunk by ID
    pub fn get_chunk(&self, chunk_id: i64) -> Result<Option<ChunkRecord>> {
        let record = self
            .conn
            .query_row(
                r#"
                SELECT c.chunk_id, c.file_id, f.path, c.symbol, c.kind,
                       c.start_line, c.end_line, c.content
                FROM chunks c
                JOIN files f ON c.file_id = f.file_id
                WHERE c.chunk_id = ?1
                "#,
                params![chunk_id],
                |row| {
                    Ok(ChunkRecord {
                        chunk_id: row.get(0)?,
                        file_id: row.get(1)?,
                        file_path: row.get(2)?,
                        symbol: row.get(3)?,
                        kind: row.get(4)?,
                        start_line: row.get(5)?,
                        end_line: row.get(6)?,
                        content: row.get(7)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    /// Get all chunk IDs (excluding tombstones)
    pub fn get_all_chunk_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT chunk_id FROM chunks
            WHERE chunk_id NOT IN (SELECT chunk_id FROM tombstones)
            "#,
        )?;
        let ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Get tombstone chunk IDs
    pub fn get_tombstones(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT chunk_id FROM tombstones")?;
        let ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Clear tombstones (after HNSW compaction)
    pub fn clear_tombstones(&self) -> Result<()> {
        self.conn.execute("DELETE FROM tombstones", [])?;
        Ok(())
    }

    /// Get/set key-value pairs
    pub fn get_kv(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let value = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value)
    }

    pub fn set_kv(&self, key: &str, value: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> Result<StorageStats> {
        let file_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        let chunk_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        let tombstone_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM tombstones", [], |row| row.get(0))?;

        Ok(StorageStats {
            file_count: file_count as usize,
            chunk_count: chunk_count as usize,
            tombstone_count: tombstone_count as usize,
        })
    }

    /// Begin a transaction
    pub fn begin_transaction(&self) -> Result<()> {
        self.conn.execute("BEGIN TRANSACTION", [])?;
        Ok(())
    }

    /// Commit transaction
    pub fn commit(&self) -> Result<()> {
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    /// Rollback transaction
    pub fn rollback(&self) -> Result<()> {
        self.conn.execute("ROLLBACK", [])?;
        Ok(())
    }
}

/// A chunk record from the database
#[derive(Debug, Clone)]
pub struct ChunkRecord {
    pub chunk_id: i64,
    pub file_id: i64,
    pub file_path: String,
    pub symbol: Option<String>,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub file_count: usize,
    pub chunk_count: usize,
    pub tombstone_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_basic() {
        let storage = Storage::open_memory().unwrap();

        // Insert a file
        let file_id = storage
            .upsert_file("src/main.rs", Some("rust"), b"hash123", 1000)
            .unwrap();
        assert!(file_id > 0);

        // Insert chunks
        let chunk_id = storage
            .insert_chunk(
                file_id,
                Some("main"),
                "function",
                1,
                10,
                b"chunkhash",
                "fn main() {}",
            )
            .unwrap();
        assert!(chunk_id > 0);

        // Get chunk
        let chunk = storage.get_chunk(chunk_id).unwrap().unwrap();
        assert_eq!(chunk.symbol, Some("main".to_string()));
        assert_eq!(chunk.file_path, "src/main.rs");

        // Stats
        let stats = storage.stats().unwrap();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.chunk_count, 1);
    }

    #[test]
    fn test_tombstones() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("test.rs", Some("rust"), b"hash", 100)
            .unwrap();
        storage
            .insert_chunk(file_id, None, "block", 1, 10, b"h", "code")
            .unwrap();

        let deleted = storage.delete_chunks_for_file(file_id).unwrap();
        assert_eq!(deleted.len(), 1);

        let tombstones = storage.get_tombstones().unwrap();
        assert_eq!(tombstones.len(), 1);
    }
}
