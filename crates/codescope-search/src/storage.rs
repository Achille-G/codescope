//! SQLite storage for metadata
use crate::Result;
use parking_lot::{Condvar, Mutex};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

static NEXT_MEMORY_DB_ID: AtomicUsize = AtomicUsize::new(0);

/// SQLite connection pool for metadata storage.
pub struct StoragePool {
    inner: Arc<StoragePoolInner>,
}

struct StoragePoolInner {
    path: StoragePath,
    max_size: usize,
    open: AtomicUsize,
    idle: Mutex<Vec<Connection>>,
    available: Condvar,
}

#[derive(Clone)]
enum StoragePath {
    File(PathBuf),
    Memory { uri: String },
}

impl StoragePool {
    /// Open a pooled storage database.
    pub fn open(path: &Path, max_size: usize) -> Result<Self> {
        let inner = Arc::new(StoragePoolInner {
            path: StoragePath::File(path.to_path_buf()),
            max_size: max_size.max(1),
            open: AtomicUsize::new(0),
            idle: Mutex::new(Vec::new()),
            available: Condvar::new(),
        });

        let pool = Self { inner };
        let conn = pool.open_connection()?;
        Storage::from_connection(conn)?.return_to_pool(&pool.inner);
        Ok(pool)
    }

    /// Open a pooled in-memory database (shared cache).
    pub fn open_memory(max_size: usize) -> Result<Self> {
        let id = NEXT_MEMORY_DB_ID.fetch_add(1, Ordering::Relaxed);
        let inner = Arc::new(StoragePoolInner {
            path: StoragePath::Memory {
                uri: format!("file:codescope_mem_{id}?mode=memory&cache=shared"),
            },
            max_size: max_size.max(1),
            open: AtomicUsize::new(0),
            idle: Mutex::new(Vec::new()),
            available: Condvar::new(),
        });

        let pool = Self { inner };
        let conn = pool.open_connection()?;
        Storage::from_connection(conn)?.return_to_pool(&pool.inner);
        Ok(pool)
    }

    /// Get a storage connection from the pool.
    pub fn get(&self) -> Result<PooledStorage> {
        let mut idle = self.inner.idle.lock();

        loop {
            if let Some(conn) = idle.pop() {
                return Ok(PooledStorage {
                    storage: Some(Storage { conn }),
                    inner: Arc::clone(&self.inner),
                });
            }

            let open = self.inner.open.load(Ordering::Relaxed);
            if open < self.inner.max_size {
                self.inner.open.fetch_add(1, Ordering::Relaxed);
                drop(idle);

                match self.open_connection().and_then(Storage::from_connection) {
                    Ok(storage) => {
                        return Ok(PooledStorage {
                            storage: Some(storage),
                            inner: Arc::clone(&self.inner),
                        })
                    }
                    Err(err) => {
                        self.inner.open.fetch_sub(1, Ordering::Relaxed);
                        return Err(err);
                    }
                }
            }

            self.inner.available.wait(&mut idle);
        }
    }

    fn open_connection(&self) -> Result<Connection> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;

        let conn = match &self.inner.path {
            StoragePath::File(path) => Connection::open_with_flags(path, flags)?,
            StoragePath::Memory { uri } => Connection::open_with_flags(
                uri,
                flags | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_SHARED_CACHE,
            )?,
        };

        conn.busy_timeout(Duration::from_millis(250))?;
        Ok(conn)
    }
}

/// A pooled storage connection that returns to the pool on drop.
pub struct PooledStorage {
    storage: Option<Storage>,
    inner: Arc<StoragePoolInner>,
}

impl Deref for PooledStorage {
    type Target = Storage;

    fn deref(&self) -> &Self::Target {
        self.storage.as_ref().expect("pooled storage missing")
    }
}

impl DerefMut for PooledStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.storage.as_mut().expect("pooled storage missing")
    }
}

impl Drop for PooledStorage {
    fn drop(&mut self) {
        if let Some(storage) = self.storage.take() {
            storage.return_to_pool(&self.inner);
        }
    }
}

/// SQLite-based metadata storage
pub struct Storage {
    conn: Connection,
}

impl Storage {
    /// Open or create a storage database
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// Open an in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    fn return_to_pool(self, pool: &StoragePoolInner) {
        let mut idle = pool.idle.lock();
        idle.push(self.conn);
        pool.available.notify_one();
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

            CREATE TABLE IF NOT EXISTS imports (
                import_id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
                source TEXT NOT NULL,
                symbol TEXT,
                alias TEXT,
                is_default INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS call_sites (
                call_id INTEGER PRIMARY KEY,
                caller_chunk_id INTEGER NOT NULL REFERENCES chunks(chunk_id) ON DELETE CASCADE,
                callee_name TEXT NOT NULL,
                resolved_chunk_id INTEGER REFERENCES chunks(chunk_id) ON DELETE SET NULL,
                line INTEGER NOT NULL,
                column INTEGER,
                is_method INTEGER DEFAULT 0,
                receiver TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file_id);
            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_chunks_symbol ON chunks(symbol);
            CREATE INDEX IF NOT EXISTS idx_imports_file ON imports(file_id);
            CREATE INDEX IF NOT EXISTS idx_imports_symbol ON imports(symbol);
            CREATE INDEX IF NOT EXISTS idx_calls_caller ON call_sites(caller_chunk_id);
            CREATE INDEX IF NOT EXISTS idx_calls_callee ON call_sites(callee_name);
            CREATE INDEX IF NOT EXISTS idx_calls_resolved ON call_sites(resolved_chunk_id);
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

        self.get_file_id(path)?
            .ok_or_else(|| crate::Error::Storage(format!("Missing file_id after upsert: {path}")))
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

    /// Delete a file and return the chunk IDs that were removed.
    ///
    /// This is useful for keeping secondary indexes (BM25/HNSW) in sync.
    pub fn delete_file_returning_chunk_ids(&self, path: &str) -> Result<Vec<i64>> {
        let Some(file_id) = self.get_file_id(path)? else {
            return Ok(Vec::new());
        };

        let chunk_ids = self.delete_chunks_for_file(file_id)?;
        self.conn
            .execute("DELETE FROM files WHERE file_id = ?1", params![file_id])?;
        Ok(chunk_ids)
    }

    /// Insert a chunk
    #[allow(clippy::too_many_arguments)]
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
            .prepare_cached("SELECT chunk_id FROM chunks WHERE file_id = ?1")?;
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

    /// Delete imports for a file.
    pub fn delete_imports_for_file(&self, file_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM imports WHERE file_id = ?1", params![file_id])?;
        Ok(())
    }

    /// Insert an import binding for a file.
    pub fn insert_import(
        &self,
        file_id: i64,
        source: &str,
        symbol: Option<&str>,
        alias: Option<&str>,
        is_default: bool,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO imports (file_id, source, symbol, alias, is_default)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                file_id,
                source,
                symbol,
                alias,
                if is_default { 1 } else { 0 }
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert a call site for a caller chunk.
    pub fn insert_call_site(
        &self,
        caller_chunk_id: i64,
        callee_name: &str,
        line: u32,
        column: Option<u32>,
        is_method: bool,
        receiver: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO call_sites (caller_chunk_id, callee_name, line, column, is_method, receiver)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                caller_chunk_id,
                callee_name,
                i64::from(line),
                column.map(i64::from),
                if is_method { 1 } else { 0 },
                receiver,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Resolve call sites for all chunks in a file (best-effort).
    pub fn resolve_call_sites(&self, file_id: i64) -> Result<usize> {
        let (file_path, lang): (String, Option<String>) = self.conn.query_row(
            "SELECT path, lang FROM files WHERE file_id = ?1",
            params![file_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let lang = lang.unwrap_or_default();

        let imports = self.get_imports_for_file(file_id)?;
        let import_by_local = build_import_lookup(&imports);

        self.transaction(|conn| {
            // Reset to allow re-resolution after re-indexing.
            conn.execute(
                r#"
                UPDATE call_sites
                SET resolved_chunk_id = NULL
                WHERE caller_chunk_id IN (SELECT chunk_id FROM chunks WHERE file_id = ?1)
                "#,
                params![file_id],
            )?;

            let mut stmt = conn.prepare_cached(
                r#"
                SELECT cs.call_id, cs.callee_name, cs.is_method, cs.receiver
                FROM call_sites cs
                JOIN chunks c ON cs.caller_chunk_id = c.chunk_id
                WHERE c.file_id = ?1
                "#,
            )?;

            let rows = stmt.query_map(params![file_id], |row| {
                Ok(CallSiteRow {
                    call_id: row.get(0)?,
                    callee_name: row.get(1)?,
                    is_method: row.get::<_, i64>(2)? != 0,
                    receiver: row.get(3)?,
                })
            })?;

            let mut resolved = 0usize;
            for row in rows {
                let row = row?;
                let mut resolved_chunk_id =
                    find_unique_chunk_in_file(conn, file_id, &row.callee_name)?;

                if resolved_chunk_id.is_none() {
                    resolved_chunk_id =
                        resolve_call_via_imports(conn, &file_path, &lang, &row, &import_by_local)?;
                }

                if resolved_chunk_id.is_none() {
                    resolved_chunk_id = find_unique_chunk_global(conn, &row.callee_name)?;
                }

                if let Some(chunk_id) = resolved_chunk_id {
                    conn.execute(
                        "UPDATE call_sites SET resolved_chunk_id = ?1 WHERE call_id = ?2",
                        params![chunk_id, row.call_id],
                    )?;
                    resolved += 1;
                }
            }

            Ok(resolved)
        })
    }

    /// Get caller locations for a symbol (best-effort).
    pub fn get_callers(&self, symbol: &str) -> Result<Vec<CallerInfo>> {
        let candidate_ids = self.get_chunk_ids_by_symbol(symbol)?;

        let mut out = Vec::new();
        if !candidate_ids.is_empty() {
            out.extend(self.get_callers_by_resolved_ids(&candidate_ids)?);
        } else {
            out.extend(self.get_callers_by_callee_name(symbol)?);
        }

        Ok(out)
    }

    /// Get caller locations for resolved callee chunk IDs.
    pub fn get_callers_for_resolved_chunk_ids(&self, chunk_ids: &[i64]) -> Result<Vec<CallerInfo>> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.get_callers_by_resolved_ids(chunk_ids)
    }

    /// Get callee locations for a caller chunk.
    pub fn get_callees(&self, chunk_id: i64) -> Result<Vec<CalleeInfo>> {
        let (caller_file_id, caller_file_path, caller_lang): (i64, String, Option<String>) =
            self.conn.query_row(
                r#"
                SELECT c.file_id, f.path, f.lang
                FROM chunks c
                JOIN files f ON c.file_id = f.file_id
                WHERE c.chunk_id = ?1
                "#,
                params![chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        let mut caller_lang = caller_lang.unwrap_or_default();
        if caller_lang.is_empty() {
            let lower = caller_file_path.to_ascii_lowercase();
            caller_lang = infer_lang_from_path(&lower).unwrap_or_default().to_string();
        }

        let imports = self.get_imports_for_file(caller_file_id)?;
        let import_kinds = if imports.is_empty() {
            None
        } else {
            Some(build_import_kind_lookup(
                &self.conn,
                &caller_file_path,
                &caller_lang,
                &imports,
            )?)
        };

        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT cs.callee_name, cs.line, cs.column, cs.is_method, cs.receiver,
                   cs.resolved_chunk_id,
                   rc.symbol, rf.path, rc.start_line
            FROM call_sites cs
            LEFT JOIN chunks rc ON cs.resolved_chunk_id = rc.chunk_id
            LEFT JOIN files rf ON rc.file_id = rf.file_id
            WHERE cs.caller_chunk_id = ?1
             ORDER BY cs.line ASC, cs.column ASC
             "#,
        )?;

        let rows = stmt.query_map(params![chunk_id], |row| {
            let callee_name: String = row.get(0)?;
            let is_method = row.get::<_, i64>(3)? != 0;
            let receiver: Option<String> = row.get(4)?;
            let resolved_chunk_id: Option<i64> = row.get(5)?;

            let target_kind = if resolved_chunk_id.is_some() {
                CallTargetKind::Project
            } else {
                classify_unresolved_call_target(
                    &caller_lang,
                    &callee_name,
                    is_method,
                    receiver.as_deref(),
                    import_kinds.as_ref(),
                    &imports,
                )
            };

            Ok(CalleeInfo {
                callee_name,
                call_line: to_u32(row.get::<_, i64>(1)?),
                call_column: row.get::<_, Option<i64>>(2)?.map(to_u32),
                is_method,
                receiver,
                target_kind,
                resolved_chunk_id,
                resolved_symbol: row.get(6)?,
                resolved_file: row.get(7)?,
                resolved_line: row.get::<_, Option<i64>>(8)?.map(to_u32),
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Find chunk records matching a symbol.
    pub fn find_chunks_by_symbol(&self, symbol: &str) -> Result<Vec<ChunkRecord>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT c.chunk_id, c.file_id, f.path, c.symbol, c.kind,
                   c.start_line, c.end_line, c.content
            FROM chunks c
            JOIN files f ON c.file_id = f.file_id
            WHERE c.symbol = ?1
            ORDER BY f.path ASC, c.start_line ASC
            "#,
        )?;

        let rows = stmt.query_map(params![symbol], |row| {
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
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Find chunk records matching a symbol in a specific file.
    pub fn find_chunks_by_symbol_in_file(
        &self,
        symbol: &str,
        file_path: &str,
    ) -> Result<Vec<ChunkRecord>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT c.chunk_id, c.file_id, f.path, c.symbol, c.kind,
                   c.start_line, c.end_line, c.content
            FROM chunks c
            JOIN files f ON c.file_id = f.file_id
            WHERE c.symbol = ?1 AND f.path = ?2
            ORDER BY c.start_line ASC
            "#,
        )?;

        let rows = stmt.query_map(params![symbol, file_path], |row| {
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
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Get all file IDs currently present in the database.
    pub fn get_all_file_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT file_id FROM files ORDER BY file_id")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    fn get_imports_for_file(&self, file_id: i64) -> Result<Vec<ImportRecord>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT source, symbol, alias, is_default
            FROM imports
            WHERE file_id = ?1
            "#,
        )?;

        let rows = stmt.query_map(params![file_id], |row| {
            Ok(ImportRecord {
                source: row.get(0)?,
                symbol: row.get(1)?,
                alias: row.get(2)?,
                is_default: row.get::<_, i64>(3)? != 0,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_chunk_ids_by_symbol(&self, symbol: &str) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT chunk_id FROM chunks WHERE symbol = ?1")?;
        let ids = stmt
            .query_map(params![symbol], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    fn get_callers_by_resolved_ids(&self, chunk_ids: &[i64]) -> Result<Vec<CallerInfo>> {
        let mut placeholders = String::new();
        for (idx, _) in chunk_ids.iter().enumerate() {
            if idx > 0 {
                placeholders.push(',');
            }
            placeholders.push('?');
        }

        let sql = format!(
            r#"
            SELECT cs.caller_chunk_id, c.symbol, f.path, cs.line, cs.column, cs.callee_name, cs.is_method, cs.receiver
            FROM call_sites cs
            JOIN chunks c ON cs.caller_chunk_id = c.chunk_id
            JOIN files f ON c.file_id = f.file_id
            WHERE cs.resolved_chunk_id IN ({placeholders})
            ORDER BY f.path ASC, cs.line ASC, cs.column ASC
            "#,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let params = rusqlite::params_from_iter(chunk_ids.iter().copied());
        let rows = stmt.query_map(params, |row| {
            Ok(CallerInfo {
                caller_chunk_id: row.get(0)?,
                caller_symbol: row.get(1)?,
                caller_file: row.get(2)?,
                call_line: to_u32(row.get::<_, i64>(3)?),
                call_column: row.get::<_, Option<i64>>(4)?.map(to_u32),
                callee_name: row.get(5)?,
                is_method: row.get::<_, i64>(6)? != 0,
                receiver: row.get(7)?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_callers_by_callee_name(&self, symbol: &str) -> Result<Vec<CallerInfo>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT cs.caller_chunk_id, c.symbol, f.path, cs.line, cs.column, cs.callee_name, cs.is_method, cs.receiver
            FROM call_sites cs
            JOIN chunks c ON cs.caller_chunk_id = c.chunk_id
            JOIN files f ON c.file_id = f.file_id
            WHERE cs.callee_name = ?1
            ORDER BY f.path ASC, cs.line ASC, cs.column ASC
            "#,
        )?;

        let rows = stmt.query_map(params![symbol], |row| {
            Ok(CallerInfo {
                caller_chunk_id: row.get(0)?,
                caller_symbol: row.get(1)?,
                caller_file: row.get(2)?,
                call_line: to_u32(row.get::<_, i64>(3)?),
                call_column: row.get::<_, Option<i64>>(4)?.map(to_u32),
                callee_name: row.get(5)?,
                is_method: row.get::<_, i64>(6)? != 0,
                receiver: row.get(7)?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
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
        let mut stmt = self.conn.prepare_cached(
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
        let mut stmt = self
            .conn
            .prepare_cached("SELECT chunk_id FROM tombstones")?;
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

    /// Run a closure inside a SQLite transaction.
    ///
    /// Rolls back automatically if the closure returns an error.
    /// Nesting-safe: when a transaction is already active on this connection,
    /// the closure runs within it (commit/rollback stay owned by the outer call).
    pub fn transaction<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        if !self.conn.is_autocommit() {
            return f(&self.conn);
        }

        self.conn.execute_batch("BEGIN TRANSACTION")?;

        match f(&self.conn) {
            Ok(result) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(result)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct CallSiteRow {
    call_id: i64,
    callee_name: String,
    is_method: bool,
    receiver: Option<String>,
}

fn infer_lang_from_path(lower_path: &str) -> Option<&'static str> {
    let ext = lower_path.rsplit_once('.').map(|(_, ext)| ext)?;
    match ext {
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("jsx"),
        "py" | "pyi" => Some("python"),
        "rs" => Some("rust"),
        "java" => Some("java"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        _ => None,
    }
}

fn build_import_lookup(imports: &[ImportRecord]) -> HashMap<String, Vec<ImportRecord>> {
    let mut map: HashMap<String, Vec<ImportRecord>> = HashMap::new();

    for import in imports {
        if let Some(local) = import.alias.as_deref().or(import.symbol.as_deref()) {
            map.entry(local.to_string())
                .or_default()
                .push(import.clone());
        }
    }

    map
}

fn build_import_kind_lookup(
    conn: &Connection,
    caller_file_path: &str,
    caller_lang: &str,
    imports: &[ImportRecord],
) -> Result<HashMap<String, CallTargetKind>> {
    let import_by_local = build_import_lookup(imports);

    match caller_lang {
        "python" => build_python_import_kind_lookup(conn, caller_file_path, &import_by_local),
        "typescript" | "javascript" | "tsx" | "jsx" => {
            Ok(build_js_import_kind_lookup(&import_by_local))
        }
        "go" => Ok(build_go_import_kind_lookup(&import_by_local)),
        "java" => Ok(build_java_import_kind_lookup(&import_by_local)),
        "rust" => Ok(build_rust_import_kind_lookup(&import_by_local)),
        _ => Ok(HashMap::new()),
    }
}

fn build_python_import_kind_lookup(
    conn: &Connection,
    caller_file_path: &str,
    import_by_local: &HashMap<String, Vec<ImportRecord>>,
) -> Result<HashMap<String, CallTargetKind>> {
    let mut out: HashMap<String, CallTargetKind> = HashMap::new();

    for (local, imports) in import_by_local {
        let mut any_project = false;
        let mut any_stdlib = false;
        let mut any_external = false;

        for import in imports {
            if import.source.is_empty() {
                continue;
            }

            if import.source.starts_with('.') {
                any_project = true;
                continue;
            }

            let candidate_paths =
                resolve_import_source_to_paths(caller_file_path, "python", &import.source);
            let mut is_project = false;
            for path in candidate_paths {
                if find_file_id_by_path(conn, &path)?.is_some() {
                    is_project = true;
                    break;
                }
            }

            if is_project {
                any_project = true;
                continue;
            }

            let root = import.source.split('.').next().unwrap_or_default().trim();
            if is_python_stdlib_root(root) {
                any_stdlib = true;
            } else {
                any_external = true;
            }
        }

        let kind = if any_project {
            CallTargetKind::Project
        } else if any_stdlib {
            CallTargetKind::Stdlib
        } else if any_external {
            CallTargetKind::External
        } else {
            CallTargetKind::Unresolved
        };

        out.insert(local.clone(), kind);
    }

    Ok(out)
}

fn build_js_import_kind_lookup(
    import_by_local: &HashMap<String, Vec<ImportRecord>>,
) -> HashMap<String, CallTargetKind> {
    let mut out: HashMap<String, CallTargetKind> = HashMap::new();

    for (local, imports) in import_by_local {
        let mut any_project = false;
        let mut any_stdlib = false;
        let mut any_external = false;

        for import in imports {
            if import.source.is_empty() {
                continue;
            }

            if is_js_relative_import_source(&import.source) {
                any_project = true;
                continue;
            }

            let source = normalize_node_import_source(&import.source);
            if is_node_stdlib_module(source) {
                any_stdlib = true;
            } else {
                any_external = true;
            }
        }

        let kind = if any_project {
            CallTargetKind::Project
        } else if any_stdlib {
            CallTargetKind::Stdlib
        } else if any_external {
            CallTargetKind::External
        } else {
            CallTargetKind::Unresolved
        };

        out.insert(local.clone(), kind);
    }

    out
}

fn build_go_import_kind_lookup(
    import_by_local: &HashMap<String, Vec<ImportRecord>>,
) -> HashMap<String, CallTargetKind> {
    let mut out: HashMap<String, CallTargetKind> = HashMap::new();

    for (local, imports) in import_by_local {
        let mut any_stdlib = false;
        let mut any_external = false;

        for import in imports {
            if import.source.is_empty() {
                continue;
            }

            if import.source == "C" {
                any_external = true;
                continue;
            }

            let root = import.source.split('/').next().unwrap_or_default().trim();
            if root.contains('.') {
                any_external = true;
            } else {
                any_stdlib = true;
            }
        }

        let kind = if any_stdlib {
            CallTargetKind::Stdlib
        } else if any_external {
            CallTargetKind::External
        } else {
            CallTargetKind::Unresolved
        };

        out.insert(local.clone(), kind);
    }

    out
}

fn build_java_import_kind_lookup(
    import_by_local: &HashMap<String, Vec<ImportRecord>>,
) -> HashMap<String, CallTargetKind> {
    let mut out: HashMap<String, CallTargetKind> = HashMap::new();

    for (local, imports) in import_by_local {
        let mut any_stdlib = false;
        let mut any_external = false;

        for import in imports {
            if import.source.is_empty() {
                continue;
            }

            if is_java_stdlib_package(&import.source) {
                any_stdlib = true;
            } else {
                any_external = true;
            }
        }

        let kind = if any_stdlib {
            CallTargetKind::Stdlib
        } else if any_external {
            CallTargetKind::External
        } else {
            CallTargetKind::Unresolved
        };

        out.insert(local.clone(), kind);
    }

    out
}

fn build_rust_import_kind_lookup(
    import_by_local: &HashMap<String, Vec<ImportRecord>>,
) -> HashMap<String, CallTargetKind> {
    let mut out: HashMap<String, CallTargetKind> = HashMap::new();

    for (local, imports) in import_by_local {
        let mut any_project = false;
        let mut any_stdlib = false;
        let mut any_external = false;

        for import in imports {
            if import.source.is_empty() {
                continue;
            }

            let root = import.source.split("::").next().unwrap_or_default().trim();
            match root {
                "crate" | "self" | "super" => any_project = true,
                "std" | "core" | "alloc" => any_stdlib = true,
                _ => any_external = true,
            }
        }

        let kind = if any_project {
            CallTargetKind::Project
        } else if any_stdlib {
            CallTargetKind::Stdlib
        } else if any_external {
            CallTargetKind::External
        } else {
            CallTargetKind::Unresolved
        };

        out.insert(local.clone(), kind);
    }

    out
}

fn classify_unresolved_call_target(
    caller_lang: &str,
    callee_name: &str,
    is_method: bool,
    receiver: Option<&str>,
    import_kinds: Option<&HashMap<String, CallTargetKind>>,
    imports: &[ImportRecord],
) -> CallTargetKind {
    if let Some(import_kinds) = import_kinds {
        if let Some(kind) = import_kind_for_call(import_kinds, callee_name, receiver) {
            return kind;
        }
    }

    match caller_lang {
        "python" => {
            let receiver_is_builtins = receiver
                .and_then(leading_ident)
                .is_some_and(|ident| ident == "builtins");
            if is_python_builtin(callee_name) && (!is_method || receiver_is_builtins) {
                CallTargetKind::Builtin
            } else {
                CallTargetKind::Unresolved
            }
        }
        "typescript" | "javascript" | "tsx" | "jsx" => {
            if receiver
                .and_then(leading_ident)
                .is_some_and(is_js_builtin_global_object)
                || (!is_method && is_js_builtin_global_function(callee_name))
            {
                CallTargetKind::Builtin
            } else {
                CallTargetKind::Unresolved
            }
        }
        "go" => {
            if !is_method && is_go_builtin(callee_name) {
                CallTargetKind::Builtin
            } else {
                CallTargetKind::Unresolved
            }
        }
        "java" => {
            if receiver.is_some_and(is_java_stdlib_receiver) {
                CallTargetKind::Stdlib
            } else {
                CallTargetKind::Unresolved
            }
        }
        "rust" => {
            if receiver.is_some_and(is_rust_stdlib_receiver)
                || (!is_method && is_rust_prelude_function(callee_name))
            {
                CallTargetKind::Stdlib
            } else {
                CallTargetKind::Unresolved
            }
        }
        "c" => {
            if is_c_stdlib_function(callee_name) {
                CallTargetKind::Stdlib
            } else {
                CallTargetKind::Unresolved
            }
        }
        "cpp" => {
            if receiver.is_some_and(is_cpp_std_receiver) || is_c_stdlib_function(callee_name) {
                CallTargetKind::Stdlib
            } else {
                // When the file includes standard headers, treat common unqualified calls as
                // `std` (best-effort; avoids `sort (unresolved)` noise for C++).
                let has_std_header = imports.iter().any(|i| is_c_stdlib_header(&i.source));
                if has_std_header && is_cpp_stdlib_unqualified_function(callee_name) {
                    CallTargetKind::Stdlib
                } else {
                    CallTargetKind::Unresolved
                }
            }
        }
        _ => CallTargetKind::Unresolved,
    }
}

fn import_kind_for_call(
    import_kinds: &HashMap<String, CallTargetKind>,
    callee_name: &str,
    receiver: Option<&str>,
) -> Option<CallTargetKind> {
    if let Some(receiver) = receiver {
        let receiver = receiver.trim();
        if let Some(kind) = import_kinds.get(receiver) {
            return Some(*kind);
        }
        if let Some(leading) = leading_ident(receiver) {
            if let Some(kind) = import_kinds.get(leading) {
                return Some(*kind);
            }
        }
    }

    import_kinds.get(callee_name).copied()
}

fn is_js_relative_import_source(source: &str) -> bool {
    let source = source.trim();
    source.starts_with("./") || source.starts_with("../") || source.starts_with('/')
}

fn normalize_node_import_source(source: &str) -> &str {
    source.strip_prefix("node:").unwrap_or(source)
}

fn is_node_stdlib_module(source: &str) -> bool {
    matches!(
        source,
        "assert"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "dns/promises"
            | "domain"
            | "events"
            | "fs"
            | "fs/promises"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "path/posix"
            | "path/win32"
            | "perf_hooks"
            | "punycode"
            | "querystring"
            | "readline"
            | "readline/promises"
            | "repl"
            | "stream"
            | "stream/consumers"
            | "stream/promises"
            | "stream/web"
            | "string_decoder"
            | "timers"
            | "timers/promises"
            | "tls"
            | "tty"
            | "url"
            | "util"
            | "util/types"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

fn is_js_builtin_global_object(name: &str) -> bool {
    matches!(
        name,
        "console"
            | "Math"
            | "JSON"
            | "Number"
            | "String"
            | "Object"
            | "Array"
            | "Promise"
            | "Date"
            | "RegExp"
            | "Map"
            | "Set"
            | "WeakMap"
            | "WeakSet"
            | "Symbol"
            | "BigInt"
            | "Reflect"
            | "Intl"
            | "URL"
            | "URLSearchParams"
            | "TextEncoder"
            | "TextDecoder"
            | "Buffer"
            | "process"
            | "document"
            | "window"
            | "globalThis"
    )
}

fn is_js_builtin_global_function(name: &str) -> bool {
    matches!(
        name,
        "setTimeout"
            | "clearTimeout"
            | "setInterval"
            | "clearInterval"
            | "setImmediate"
            | "clearImmediate"
            | "queueMicrotask"
            | "fetch"
            | "atob"
            | "btoa"
            | "requestAnimationFrame"
            | "cancelAnimationFrame"
            | "parseInt"
            | "parseFloat"
            | "isNaN"
            | "isFinite"
            | "encodeURI"
            | "encodeURIComponent"
            | "decodeURI"
            | "decodeURIComponent"
    )
}

fn is_go_builtin(name: &str) -> bool {
    matches!(
        name,
        "append"
            | "cap"
            | "clear"
            | "close"
            | "complex"
            | "copy"
            | "delete"
            | "imag"
            | "len"
            | "make"
            | "max"
            | "min"
            | "new"
            | "panic"
            | "print"
            | "println"
            | "real"
            | "recover"
    )
}

fn is_java_stdlib_package(package: &str) -> bool {
    let package = package.trim();
    package == "java"
        || package.starts_with("java.")
        || package == "javax"
        || package.starts_with("javax.")
        || package == "jdk"
        || package.starts_with("jdk.")
        || package == "sun"
        || package.starts_with("sun.")
        || package == "com.sun"
        || package.starts_with("com.sun.")
        || package == "javafx"
        || package.starts_with("javafx.")
        || package == "org.w3c"
        || package.starts_with("org.w3c.")
        || package == "org.xml"
        || package.starts_with("org.xml.")
}

fn is_java_lang_class(name: &str) -> bool {
    matches!(
        name,
        "System"
            | "String"
            | "Math"
            | "Integer"
            | "Long"
            | "Double"
            | "Float"
            | "Boolean"
            | "Character"
            | "Object"
            | "Class"
            | "Thread"
            | "Runtime"
            | "Process"
            | "ProcessBuilder"
            | "Exception"
            | "RuntimeException"
            | "Error"
            | "Throwable"
            | "Enum"
            | "StringBuilder"
            | "StringBuffer"
    )
}

fn is_java_stdlib_receiver(receiver: &str) -> bool {
    let receiver = receiver.trim();
    if is_java_stdlib_package(receiver) {
        return true;
    }

    let leading = leading_ident(receiver).unwrap_or_default();
    is_java_lang_class(leading)
}

fn is_rust_stdlib_receiver(receiver: &str) -> bool {
    let receiver = receiver.trim();
    let root = receiver.split("::").next().unwrap_or_default().trim();
    matches!(root, "std" | "core" | "alloc")
}

fn is_rust_prelude_function(name: &str) -> bool {
    matches!(name, "drop")
}

fn is_cpp_std_receiver(receiver: &str) -> bool {
    let receiver = receiver.trim();
    let root = receiver.split("::").next().unwrap_or_default().trim();
    root == "std"
}

fn is_c_stdlib_header(header: &str) -> bool {
    let header = header.trim();
    matches!(
        header,
        "assert.h"
            | "ctype.h"
            | "errno.h"
            | "float.h"
            | "limits.h"
            | "locale.h"
            | "math.h"
            | "setjmp.h"
            | "signal.h"
            | "stdarg.h"
            | "stddef.h"
            | "stdint.h"
            | "stdio.h"
            | "stdlib.h"
            | "string.h"
            | "time.h"
            | "wchar.h"
            | "wctype.h"
            | "cstdio"
            | "cstdlib"
            | "cstring"
            | "cmath"
            | "cassert"
            | "iostream"
            | "vector"
            | "string"
            | "map"
            | "unordered_map"
            | "set"
            | "unordered_set"
            | "memory"
            | "utility"
            | "algorithm"
            | "functional"
            | "thread"
            | "mutex"
            | "chrono"
            | "bits/stdc++.h"
    )
}

fn is_cpp_stdlib_unqualified_function(name: &str) -> bool {
    matches!(
        name,
        "sort"
            | "swap"
            | "move"
            | "forward"
            | "make_pair"
            | "make_tuple"
            | "make_shared"
            | "make_unique"
            | "to_string"
            | "stoi"
            | "stol"
            | "stoll"
            | "stof"
            | "stod"
            | "stold"
    )
}

fn is_c_stdlib_function(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "abort"
            | "exit"
            | "perror"
            | "printf"
            | "fprintf"
            | "sprintf"
            | "snprintf"
            | "scanf"
            | "fscanf"
            | "sscanf"
            | "puts"
            | "fputs"
            | "putchar"
            | "getchar"
            | "fopen"
            | "fclose"
            | "fread"
            | "fwrite"
            | "fflush"
            | "fseek"
            | "ftell"
            | "fgets"
            | "malloc"
            | "calloc"
            | "realloc"
            | "free"
            | "memcpy"
            | "memmove"
            | "memset"
            | "memcmp"
            | "strlen"
            | "strcpy"
            | "strncpy"
            | "strcat"
            | "strncat"
            | "strcmp"
            | "strncmp"
            | "strchr"
            | "strrchr"
            | "strstr"
            | "strtok"
            | "tolower"
            | "toupper"
            | "isalnum"
            | "isalpha"
            | "isdigit"
            | "isspace"
    )
}

fn is_python_builtin(name: &str) -> bool {
    matches!(
        name,
        "__build_class__"
            | "__debug__"
            | "__doc__"
            | "__import__"
            | "__loader__"
            | "__name__"
            | "__package__"
            | "__spec__"
            | "abs"
            | "aiter"
            | "all"
            | "anext"
            | "any"
            | "ascii"
            | "bin"
            | "bool"
            | "breakpoint"
            | "bytearray"
            | "bytes"
            | "callable"
            | "chr"
            | "classmethod"
            | "compile"
            | "complex"
            | "delattr"
            | "dict"
            | "dir"
            | "divmod"
            | "enumerate"
            | "eval"
            | "exec"
            | "filter"
            | "float"
            | "format"
            | "frozenset"
            | "getattr"
            | "globals"
            | "hasattr"
            | "hash"
            | "help"
            | "hex"
            | "id"
            | "input"
            | "int"
            | "isinstance"
            | "issubclass"
            | "iter"
            | "len"
            | "list"
            | "locals"
            | "map"
            | "max"
            | "memoryview"
            | "min"
            | "next"
            | "object"
            | "oct"
            | "open"
            | "ord"
            | "pow"
            | "print"
            | "property"
            | "range"
            | "repr"
            | "reversed"
            | "round"
            | "set"
            | "setattr"
            | "slice"
            | "sorted"
            | "staticmethod"
            | "str"
            | "sum"
            | "super"
            | "tuple"
            | "type"
            | "vars"
            | "zip"
            | "ArithmeticError"
            | "AssertionError"
            | "AttributeError"
            | "BaseException"
            | "BaseExceptionGroup"
            | "BlockingIOError"
            | "BrokenPipeError"
            | "BufferError"
            | "BytesWarning"
            | "ChildProcessError"
            | "ConnectionAbortedError"
            | "ConnectionError"
            | "ConnectionRefusedError"
            | "ConnectionResetError"
            | "DeprecationWarning"
            | "EOFError"
            | "EncodingWarning"
            | "EnvironmentError"
            | "Exception"
            | "ExceptionGroup"
            | "FileExistsError"
            | "FileNotFoundError"
            | "FloatingPointError"
            | "FutureWarning"
            | "GeneratorExit"
            | "IOError"
            | "ImportError"
            | "ImportWarning"
            | "IndentationError"
            | "IndexError"
            | "InterruptedError"
            | "IsADirectoryError"
            | "KeyError"
            | "KeyboardInterrupt"
            | "LookupError"
            | "MemoryError"
            | "ModuleNotFoundError"
            | "NameError"
            | "NotADirectoryError"
            | "NotImplemented"
            | "NotImplementedError"
            | "OSError"
            | "OverflowError"
            | "PendingDeprecationWarning"
            | "PermissionError"
            | "ProcessLookupError"
            | "RecursionError"
            | "ReferenceError"
            | "ResourceWarning"
            | "RuntimeError"
            | "RuntimeWarning"
            | "StopAsyncIteration"
            | "StopIteration"
            | "SyntaxError"
            | "SyntaxWarning"
            | "SystemError"
            | "SystemExit"
            | "TabError"
            | "TimeoutError"
            | "TypeError"
            | "UnboundLocalError"
            | "UnicodeDecodeError"
            | "UnicodeEncodeError"
            | "UnicodeError"
            | "UnicodeTranslateError"
            | "UnicodeWarning"
            | "UserWarning"
            | "ValueError"
            | "Warning"
            | "ZeroDivisionError"
    )
}

fn is_python_stdlib_root(module: &str) -> bool {
    // Common top-level stdlib modules (best-effort).
    matches!(
        module,
        "__future__"
            | "_abc"
            | "_ast"
            | "_bisect"
            | "_blake2"
            | "_bz2"
            | "_codecs"
            | "_collections"
            | "_collections_abc"
            | "_compat_pickle"
            | "_compression"
            | "_contextvars"
            | "_csv"
            | "_ctypes"
            | "_curses"
            | "_datetime"
            | "_decimal"
            | "_elementtree"
            | "_functools"
            | "_hashlib"
            | "_heapq"
            | "_imp"
            | "_io"
            | "_json"
            | "_locale"
            | "_lzma"
            | "_markupbase"
            | "_md5"
            | "_msi"
            | "_multibytecodec"
            | "_multiprocessing"
            | "_opcode"
            | "_operator"
            | "_osx_support"
            | "_pickle"
            | "_posixshmem"
            | "_py_abc"
            | "_pydecimal"
            | "_pyio"
            | "_queue"
            | "_random"
            | "_sha1"
            | "_sha256"
            | "_sha3"
            | "_sha512"
            | "_signal"
            | "_socket"
            | "_sqlite3"
            | "_ssl"
            | "_stat"
            | "_statistics"
            | "_string"
            | "_struct"
            | "_thread"
            | "_threading_local"
            | "_tkinter"
            | "_tracemalloc"
            | "_typing"
            | "_uuid"
            | "_warnings"
            | "_weakref"
            | "_weakrefset"
            | "abc"
            | "aifc"
            | "antigravity"
            | "argparse"
            | "array"
            | "ast"
            | "asynchat"
            | "asyncio"
            | "asyncore"
            | "atexit"
            | "audioop"
            | "base64"
            | "bdb"
            | "binascii"
            | "bisect"
            | "builtins"
            | "bz2"
            | "calendar"
            | "cgi"
            | "cgitb"
            | "chunk"
            | "cmath"
            | "cmd"
            | "code"
            | "codecs"
            | "codeop"
            | "collections"
            | "colorsys"
            | "compileall"
            | "concurrent"
            | "configparser"
            | "contextlib"
            | "contextvars"
            | "copy"
            | "copyreg"
            | "cProfile"
            | "crypt"
            | "csv"
            | "ctypes"
            | "curses"
            | "dataclasses"
            | "datetime"
            | "dbm"
            | "decimal"
            | "difflib"
            | "dis"
            | "distutils"
            | "doctest"
            | "email"
            | "encodings"
            | "ensurepip"
            | "enum"
            | "errno"
            | "faulthandler"
            | "fcntl"
            | "filecmp"
            | "fileinput"
            | "fnmatch"
            | "fractions"
            | "ftplib"
            | "functools"
            | "gc"
            | "getopt"
            | "getpass"
            | "gettext"
            | "glob"
            | "graphlib"
            | "gzip"
            | "hashlib"
            | "heapq"
            | "hmac"
            | "html"
            | "http"
            | "imaplib"
            | "imghdr"
            | "imp"
            | "importlib"
            | "inspect"
            | "io"
            | "ipaddress"
            | "itertools"
            | "json"
            | "keyword"
            | "lib2to3"
            | "linecache"
            | "locale"
            | "logging"
            | "lzma"
            | "mailbox"
            | "mailcap"
            | "marshal"
            | "math"
            | "mimetypes"
            | "mmap"
            | "modulefinder"
            | "msilib"
            | "msvcrt"
            | "multiprocessing"
            | "netrc"
            | "nis"
            | "nntplib"
            | "numbers"
            | "operator"
            | "optparse"
            | "os"
            | "pathlib"
            | "pdb"
            | "pickle"
            | "pickletools"
            | "pipes"
            | "pkgutil"
            | "platform"
            | "plistlib"
            | "poplib"
            | "posix"
            | "pprint"
            | "profile"
            | "pstats"
            | "pty"
            | "pwd"
            | "py_compile"
            | "pyclbr"
            | "pydoc"
            | "queue"
            | "quopri"
            | "random"
            | "re"
            | "readline"
            | "reprlib"
            | "resource"
            | "rlcompleter"
            | "runpy"
            | "sched"
            | "secrets"
            | "select"
            | "selectors"
            | "shelve"
            | "shlex"
            | "shutil"
            | "signal"
            | "site"
            | "smtpd"
            | "smtplib"
            | "sndhdr"
            | "socket"
            | "socketserver"
            | "sqlite3"
            | "ssl"
            | "stat"
            | "statistics"
            | "string"
            | "stringprep"
            | "struct"
            | "subprocess"
            | "sunau"
            | "symtable"
            | "sys"
            | "sysconfig"
            | "tabnanny"
            | "tarfile"
            | "telnetlib"
            | "tempfile"
            | "textwrap"
            | "threading"
            | "time"
            | "timeit"
            | "tkinter"
            | "token"
            | "tokenize"
            | "trace"
            | "traceback"
            | "tracemalloc"
            | "tty"
            | "turtle"
            | "types"
            | "typing"
            | "unicodedata"
            | "unittest"
            | "urllib"
            | "uuid"
            | "venv"
            | "warnings"
            | "wave"
            | "weakref"
            | "webbrowser"
            | "winreg"
            | "winsound"
            | "wsgiref"
            | "xml"
            | "xmlrpc"
            | "zipapp"
            | "zipfile"
            | "zipimport"
            | "zlib"
    )
}

fn resolve_call_via_imports(
    conn: &Connection,
    caller_file_path: &str,
    caller_lang: &str,
    call: &CallSiteRow,
    import_by_local: &HashMap<String, Vec<ImportRecord>>,
) -> Result<Option<i64>> {
    let lookup_key = if call.is_method {
        call.receiver
            .as_deref()
            .and_then(|r| import_by_local.get(r.trim()).map(|_| r.trim().to_string()))
            .or_else(|| {
                call.receiver
                    .as_deref()
                    .and_then(leading_ident)
                    .map(|s| s.to_string())
            })
    } else {
        Some(call.callee_name.clone())
    };

    let Some(lookup_key) = lookup_key else {
        return Ok(None);
    };

    let Some(imports) = import_by_local.get(&lookup_key) else {
        return Ok(None);
    };

    for import in imports {
        let desired_symbol = if call.is_method {
            Some(call.callee_name.as_str())
        } else if let Some(sym) = import.symbol.as_deref() {
            if sym == "*" {
                None
            } else {
                Some(sym)
            }
        } else if import.is_default {
            Some(call.callee_name.as_str())
        } else {
            None
        };

        let Some(desired_symbol) = desired_symbol else {
            continue;
        };

        let candidate_paths =
            resolve_import_source_to_paths(caller_file_path, caller_lang, &import.source);
        for path in candidate_paths {
            let Some(import_file_id) = find_file_id_by_path(conn, &path)? else {
                continue;
            };

            if let Some(chunk_id) = find_unique_chunk_in_file(conn, import_file_id, desired_symbol)?
            {
                return Ok(Some(chunk_id));
            }

            // Default imports can map to a single exported function/class with a different name.
            if import.is_default && import.symbol.is_none() {
                if let Some(chunk_id) = find_unique_function_chunk_in_file(conn, import_file_id)? {
                    return Ok(Some(chunk_id));
                }
            }
        }

        // Fallback for languages where mapping `source` -> file is hard (Rust/Go/Java/C):
        // use the imported symbol name to resolve globally when it's unique.
        if !call.is_method {
            if let Some(chunk_id) = find_unique_chunk_global(conn, desired_symbol)? {
                return Ok(Some(chunk_id));
            }
        }
    }

    Ok(None)
}

fn resolve_import_source_to_paths(
    caller_file_path: &str,
    caller_lang: &str,
    source: &str,
) -> Vec<String> {
    match caller_lang {
        "typescript" | "javascript" | "tsx" | "jsx" => {
            resolve_js_import_paths(caller_file_path, source)
        }
        "python" => resolve_python_import_paths(source),
        _ => Vec::new(),
    }
}

fn resolve_js_import_paths(caller_file_path: &str, source: &str) -> Vec<String> {
    if source.is_empty() {
        return Vec::new();
    }

    let source = source.replace('\\', "/");
    let base = if source.starts_with("./") || source.starts_with("../") {
        let dir = dirname_posix(caller_file_path);
        join_posix(dir, &source)
    } else if source.starts_with('/') {
        normalize_posix_path(source.trim_start_matches('/'))
    } else {
        return Vec::new();
    };

    if has_extension(&base) {
        return vec![base];
    }

    let exts = ["ts", "tsx", "js", "jsx", "mjs", "cjs"];
    let mut paths = Vec::new();
    for ext in exts {
        paths.push(format!("{base}.{ext}"));
        paths.push(format!("{base}/index.{ext}"));
    }

    paths.sort();
    paths.dedup();
    paths
}

fn resolve_python_import_paths(source: &str) -> Vec<String> {
    if source.is_empty() || source.starts_with('.') {
        return Vec::new();
    }

    let base = source.replace('.', "/");
    vec![format!("{base}.py"), format!("{base}/__init__.py")]
}

fn find_file_id_by_path(conn: &Connection, path: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT file_id FROM files WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn find_unique_chunk_in_file(conn: &Connection, file_id: i64, symbol: &str) -> Result<Option<i64>> {
    let mut stmt = conn.prepare_cached(
        r#"
        SELECT chunk_id
        FROM chunks
        WHERE file_id = ?1 AND symbol = ?2
        LIMIT 2
        "#,
    )?;

    let ids: Vec<i64> = stmt
        .query_map(params![file_id, symbol], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((ids.len() == 1).then(|| ids[0]))
}

fn find_unique_function_chunk_in_file(conn: &Connection, file_id: i64) -> Result<Option<i64>> {
    let mut stmt = conn.prepare_cached(
        r#"
        SELECT chunk_id
        FROM chunks
        WHERE file_id = ?1 AND kind = 'function'
        LIMIT 2
        "#,
    )?;

    let ids: Vec<i64> = stmt
        .query_map(params![file_id], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((ids.len() == 1).then(|| ids[0]))
}

fn find_unique_chunk_global(conn: &Connection, symbol: &str) -> Result<Option<i64>> {
    let mut stmt = conn.prepare_cached(
        r#"
        SELECT chunk_id
        FROM chunks
        WHERE symbol = ?1
        LIMIT 2
        "#,
    )?;

    let ids: Vec<i64> = stmt
        .query_map(params![symbol], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok((ids.len() == 1).then(|| ids[0]))
}

fn dirname_posix(path: &str) -> &str {
    let idx = path.rfind('/').or_else(|| path.rfind('\\'));
    idx.map(|i| &path[..i]).unwrap_or("")
}

fn join_posix(base: &str, rel: &str) -> String {
    let base = base.replace('\\', "/").trim_end_matches('/').to_string();
    let rel = rel.replace('\\', "/");
    if base.is_empty() {
        normalize_posix_path(&rel)
    } else {
        normalize_posix_path(&format!("{base}/{rel}"))
    }
}

fn normalize_posix_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn has_extension(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.contains('.')
}

fn leading_ident(text: &str) -> Option<&str> {
    let text = text.trim();
    let mut chars = text.chars();
    let first = chars.next()?;
    if !is_ident_start(first) {
        return None;
    }
    let mut end = first.len_utf8();
    for c in chars {
        if is_ident_continue(c) {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    Some(&text[..end])
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn to_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(0)
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

/// Import binding information for a file.
#[derive(Debug, Clone)]
pub struct ImportRecord {
    pub source: String,
    pub symbol: Option<String>,
    pub alias: Option<String>,
    pub is_default: bool,
}

/// A "caller" entry for a given symbol.
#[derive(Debug, Clone)]
pub struct CallerInfo {
    pub caller_chunk_id: i64,
    pub caller_symbol: Option<String>,
    pub caller_file: String,
    pub call_line: u32,
    pub call_column: Option<u32>,
    pub callee_name: String,
    pub is_method: bool,
    pub receiver: Option<String>,
}

/// Best-effort classification for callee targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallTargetKind {
    /// Target appears to be in the indexed project (resolved or inferred via imports).
    Project,
    /// Language/runtime builtin (e.g., Python `print`).
    Builtin,
    /// Standard library module/symbol (best-effort).
    Stdlib,
    /// Third-party dependency module/symbol (best-effort).
    External,
    /// Unknown/unresolved target.
    Unresolved,
}

impl CallTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Builtin => "builtin",
            Self::Stdlib => "stdlib",
            Self::External => "external",
            Self::Unresolved => "unresolved",
        }
    }
}

impl std::fmt::Display for CallTargetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A "callee" entry for a given caller chunk.
#[derive(Debug, Clone)]
pub struct CalleeInfo {
    pub callee_name: String,
    pub call_line: u32,
    pub call_column: Option<u32>,
    pub is_method: bool,
    pub receiver: Option<String>,
    pub target_kind: CallTargetKind,
    pub resolved_chunk_id: Option<i64>,
    pub resolved_symbol: Option<String>,
    pub resolved_file: Option<String>,
    pub resolved_line: Option<u32>,
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
    use crate::Error;

    #[test]
    fn test_storage_pool_shared_state() {
        let pool = StoragePool::open_memory(2).unwrap();

        {
            let storage = pool.get().unwrap();
            storage.upsert_file("a.rs", Some("rust"), b"h1", 1).unwrap();
        }

        let storage = pool.get().unwrap();
        let stats = storage.stats().unwrap();
        assert_eq!(stats.file_count, 1);
    }

    #[test]
    fn test_storage_transaction_rollback() {
        let storage = Storage::open_memory().unwrap();
        let result: Result<()> = storage.transaction(|conn| {
            conn.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)",
                params!["k", b"v"],
            )?;
            Err(Error::Storage("boom".to_string()))
        });

        assert!(result.is_err());
        assert!(storage.get_kv("k").unwrap().is_none());
    }

    #[test]
    fn test_storage_transaction_nested() {
        let storage = Storage::open_memory().unwrap();

        // An outer transaction wrapping methods that use transactions
        // internally (e.g. delete_chunks_for_file) must not fail with
        // "cannot start a transaction within a transaction".
        let result: Result<()> = storage.transaction(|_| {
            let file_id = storage.upsert_file("a.rs", Some("rust"), b"h1", 1)?;
            storage.insert_chunk(file_id, Some("f"), "function", 1, 2, b"c1", "fn f() {}")?;
            storage.transaction(|_| {
                storage.insert_chunk(file_id, Some("g"), "function", 3, 4, b"c2", "fn g() {}")?;
                Ok(())
            })?;
            storage.delete_chunks_for_file(file_id)?;
            Ok(())
        });
        assert!(result.is_ok());

        // Inner failure rolls back the whole outer transaction.
        let result: Result<()> = storage.transaction(|_| {
            storage.upsert_file("b.rs", Some("rust"), b"h2", 1)?;
            storage.transaction(|_| Err(Error::Storage("boom".to_string())))
        });
        assert!(result.is_err());
        assert!(storage.get_file_id("b.rs").unwrap().is_none());
    }

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

    #[test]
    fn test_call_site_resolution_typescript_import() {
        let storage = Storage::open_memory().unwrap();

        let file_id_b = storage
            .upsert_file("src/b.ts", Some("typescript"), b"h_b", 10)
            .unwrap();
        let foo_chunk_id = storage
            .insert_chunk(
                file_id_b,
                Some("foo"),
                "function",
                1,
                1,
                b"c_b",
                "export function foo() {}",
            )
            .unwrap();

        let file_id_a = storage
            .upsert_file("src/a.ts", Some("typescript"), b"h_a", 10)
            .unwrap();
        storage.delete_imports_for_file(file_id_a).unwrap();
        storage
            .insert_import(file_id_a, "./b", Some("foo"), None, false)
            .unwrap();

        let caller_chunk_id = storage
            .insert_chunk(
                file_id_a,
                Some("caller"),
                "function",
                1,
                3,
                b"c_a",
                "import { foo } from './b';\nexport function caller() { foo(); }\n",
            )
            .unwrap();
        storage
            .insert_call_site(caller_chunk_id, "foo", 2, Some(1), false, None)
            .unwrap();

        let resolved = storage.resolve_call_sites(file_id_a).unwrap();
        assert_eq!(resolved, 1);

        let callees = storage.get_callees(caller_chunk_id).unwrap();
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].target_kind, CallTargetKind::Project);
        assert_eq!(callees[0].resolved_chunk_id, Some(foo_chunk_id));
        assert_eq!(callees[0].resolved_file.as_deref(), Some("src/b.ts"));

        let callers = storage.get_callers("foo").unwrap();
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].caller_file, "src/a.ts");
    }

    #[test]
    fn test_python_target_kind_builtin_and_stdlib() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("a.py", Some("python"), b"h_py", 10)
            .unwrap();
        let chunk_id = storage
            .insert_chunk(
                file_id,
                Some("caller"),
                "function",
                1,
                4,
                b"c_py",
                "import os\n\ndef caller():\n    print('hi')\n    os.path.join('a', 'b')\n",
            )
            .unwrap();

        storage
            .insert_import(file_id, "os", None, Some("os"), false)
            .unwrap();

        storage
            .insert_call_site(chunk_id, "print", 4, Some(5), false, None)
            .unwrap();
        storage
            .insert_call_site(chunk_id, "join", 5, Some(5), true, Some("os.path"))
            .unwrap();

        let callees = storage.get_callees(chunk_id).unwrap();
        assert_eq!(callees.len(), 2);
        assert_eq!(callees[0].callee_name, "print");
        assert_eq!(callees[0].target_kind, CallTargetKind::Builtin);
        assert_eq!(callees[1].callee_name, "join");
        assert_eq!(callees[1].target_kind, CallTargetKind::Stdlib);
    }

    #[test]
    fn test_typescript_target_kind_project_stdlib_external() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("src/a.ts", Some("typescript"), b"h_ts", 10)
            .unwrap();
        let chunk_id = storage
            .insert_chunk(file_id, Some("caller"), "function", 1, 5, b"c_ts", "...")
            .unwrap();

        // Namespace import from a Node stdlib module.
        storage
            .insert_import(file_id, "fs", Some("*"), Some("fs"), false)
            .unwrap();
        // Named import from an external package.
        storage
            .insert_import(file_id, "lodash", Some("uniq"), None, false)
            .unwrap();
        // Local (project) import.
        storage
            .insert_import(file_id, "./b", Some("localFn"), None, false)
            .unwrap();

        storage
            .insert_call_site(chunk_id, "readFile", 1, Some(1), true, Some("fs"))
            .unwrap();
        storage
            .insert_call_site(chunk_id, "uniq", 2, Some(1), false, None)
            .unwrap();
        storage
            .insert_call_site(chunk_id, "localFn", 3, Some(1), false, None)
            .unwrap();

        let callees = storage.get_callees(chunk_id).unwrap();
        let kinds: HashMap<_, _> = callees
            .into_iter()
            .map(|c| (c.callee_name, c.target_kind))
            .collect();
        assert_eq!(kinds.get("readFile"), Some(&CallTargetKind::Stdlib));
        assert_eq!(kinds.get("uniq"), Some(&CallTargetKind::External));
        assert_eq!(kinds.get("localFn"), Some(&CallTargetKind::Project));
    }

    #[test]
    fn test_go_target_kind_builtin_and_stdlib_import() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("a.go", Some("go"), b"h_go", 10)
            .unwrap();
        let chunk_id = storage
            .insert_chunk(file_id, Some("caller"), "function", 1, 5, b"c_go", "...")
            .unwrap();

        storage
            .insert_import(file_id, "fmt", None, Some("fmt"), false)
            .unwrap();

        storage
            .insert_call_site(chunk_id, "Println", 1, Some(1), true, Some("fmt"))
            .unwrap();
        storage
            .insert_call_site(chunk_id, "len", 2, Some(1), false, None)
            .unwrap();

        let callees = storage.get_callees(chunk_id).unwrap();
        let kinds: HashMap<_, _> = callees
            .into_iter()
            .map(|c| (c.callee_name, c.target_kind))
            .collect();
        assert_eq!(kinds.get("Println"), Some(&CallTargetKind::Stdlib));
        assert_eq!(kinds.get("len"), Some(&CallTargetKind::Builtin));
    }

    #[test]
    fn test_java_target_kind_stdlib_import_and_lang_class() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("a.java", Some("java"), b"h_java", 10)
            .unwrap();
        let chunk_id = storage
            .insert_chunk(file_id, Some("caller"), "method", 1, 10, b"c_java", "...")
            .unwrap();

        storage
            .insert_import(file_id, "java.util", Some("Collections"), None, false)
            .unwrap();

        storage
            .insert_call_site(chunk_id, "sort", 1, Some(1), true, Some("Collections"))
            .unwrap();
        storage
            .insert_call_site(chunk_id, "println", 2, Some(1), true, Some("System.out"))
            .unwrap();

        let callees = storage.get_callees(chunk_id).unwrap();
        let kinds: HashMap<_, _> = callees
            .into_iter()
            .map(|c| (c.callee_name, c.target_kind))
            .collect();
        assert_eq!(kinds.get("sort"), Some(&CallTargetKind::Stdlib));
        assert_eq!(kinds.get("println"), Some(&CallTargetKind::Stdlib));
    }

    #[test]
    fn test_rust_target_kind_stdlib_import_receiver() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("a.rs", Some("rust"), b"h_rs", 10)
            .unwrap();
        let chunk_id = storage
            .insert_chunk(file_id, Some("caller"), "function", 1, 5, b"c_rs", "...")
            .unwrap();

        // `use std::mem;`
        storage
            .insert_import(file_id, "std", Some("mem"), None, false)
            .unwrap();

        // `mem::drop(x)`
        storage
            .insert_call_site(chunk_id, "drop", 1, Some(1), false, Some("mem"))
            .unwrap();

        let callees = storage.get_callees(chunk_id).unwrap();
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].callee_name, "drop");
        assert_eq!(callees[0].target_kind, CallTargetKind::Stdlib);
    }

    #[test]
    fn test_cpp_target_kind_stdlib_header_unqualified_call() {
        let storage = Storage::open_memory().unwrap();

        let file_id = storage
            .upsert_file("a.cpp", Some("cpp"), b"h_cpp", 10)
            .unwrap();
        let chunk_id = storage
            .insert_chunk(file_id, Some("caller"), "function", 1, 5, b"c_cpp", "...")
            .unwrap();

        // `#include <algorithm>`
        storage
            .insert_import(file_id, "algorithm", None, None, false)
            .unwrap();

        // `sort(...)`
        storage
            .insert_call_site(chunk_id, "sort", 1, Some(1), false, None)
            .unwrap();

        let callees = storage.get_callees(chunk_id).unwrap();
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].callee_name, "sort");
        assert_eq!(callees[0].target_kind, CallTargetKind::Stdlib);
    }
}
