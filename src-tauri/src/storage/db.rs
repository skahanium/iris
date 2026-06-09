use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

#[cfg(feature = "sqlite-vec")]
use std::sync::Once;

use rusqlite::Connection;

use super::migrate::migrate_up;
use crate::error::{AppError, AppResult};

static VECTOR_INDEX_READY: AtomicBool = AtomicBool::new(false);
static IN_MEMORY_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "sqlite-vec")]
static SQLITE_VEC_REGISTER: Once = Once::new();

const READ_POOL_SIZE: usize = 8;
const WRITE_POOL_SIZE: usize = 2;

/// Whether sqlite-vec vec0 tables are available for this process.
pub fn vector_index_ready() -> bool {
    VECTOR_INDEX_READY.load(Ordering::Relaxed)
}

/// SQLite connection pool with WAL and performance PRAGMAs from ARCHITECTURE.md.
///
/// Uses a pool of write connections and a pool of read connections to allow
/// concurrent read and write operations while writes are in progress (WAL mode).
///
/// Multiple write connections reduce lock contention at the Rust level: a short
/// IPC write (editor auto-save) won't be blocked behind a long-running write
/// (re-index, batch embedding). SQLite's WAL + `busy_timeout` handles the
/// file-level serialization transparently.
pub struct Database {
    write_pool: Vec<Mutex<Connection>>,
    write_idx: AtomicUsize,
    read_pool: Vec<Mutex<Connection>>,
    read_idx: AtomicUsize,
    #[allow(dead_code)]
    path: Option<PathBuf>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let primary = Connection::open(path)?;
        Self::apply_pragmas(&primary)?;
        #[cfg(feature = "sqlite-vec")]
        let vec_ready = Self::try_load_sqlite_vec(&primary, true);
        #[cfg(not(feature = "sqlite-vec"))]
        let vec_ready = false;
        VECTOR_INDEX_READY.store(vec_ready, Ordering::Relaxed);
        migrate_up(&primary)?;

        let mut write_pool = Vec::with_capacity(WRITE_POOL_SIZE);
        write_pool.push(Mutex::new(primary));
        for _ in 1..WRITE_POOL_SIZE {
            let wc = Connection::open(path)?;
            Self::apply_pragmas(&wc)?;
            #[cfg(feature = "sqlite-vec")]
            let _ = Self::try_load_sqlite_vec(&wc, true);
            write_pool.push(Mutex::new(wc));
        }

        let mut read_pool = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let rc = Connection::open(path)?;
            Self::apply_pragmas(&rc)?;
            #[cfg(feature = "sqlite-vec")]
            let _ = Self::try_load_sqlite_vec(&rc, true);
            read_pool.push(Mutex::new(rc));
        }

        Ok(Self {
            write_pool,
            write_idx: AtomicUsize::new(0),
            read_pool,
            read_idx: AtomicUsize::new(0),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let db_id = IN_MEMORY_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_uri = format!("file:memdb_{db_id}?mode=memory&cache=shared");

        let primary = Connection::open(&db_uri)?;
        Self::apply_pragmas(&primary)?;
        VECTOR_INDEX_READY.store(false, Ordering::Relaxed);
        migrate_up(&primary)?;

        let mut write_pool = Vec::with_capacity(WRITE_POOL_SIZE);
        write_pool.push(Mutex::new(primary));
        for _ in 1..WRITE_POOL_SIZE {
            let wc = Connection::open(&db_uri)?;
            Self::apply_pragmas(&wc)?;
            write_pool.push(Mutex::new(wc));
        }

        let mut read_pool = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let rc = Connection::open(&db_uri)?;
            Self::apply_pragmas(&rc)?;
            read_pool.push(Mutex::new(rc));
        }

        Ok(Self {
            write_pool,
            write_idx: AtomicUsize::new(0),
            read_pool,
            read_idx: AtomicUsize::new(0),
            path: None,
        })
    }

    #[cfg(feature = "sqlite-vec")]
    /// Load sqlite-vec extension when the bundled SQLite supports it.
    fn try_load_sqlite_vec(conn: &Connection, persistent_db: bool) -> bool {
        if !persistent_db || cfg!(test) {
            return false;
        }
        // Register once per process; `sqlite3_auto_extension` is global state.
        SQLITE_VEC_REGISTER.call_once(|| {
            // SAFETY: sqlite-vec documents registration via `sqlite3_auto_extension` (see sqlite-vec
            // crate tests). No safe alternative exists for static extension init with rusqlite bundled.
            #[allow(clippy::missing_transmute_annotations)]
            unsafe {
                rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                    sqlite_vec::sqlite3_vec_init as *const (),
                )));
            }
        });
        match conn.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0)) {
            Ok(version) => {
                tracing::info!(%version, "sqlite-vec extension loaded");
                true
            }
            Err(e) => {
                tracing::warn!("sqlite-vec extension not available: {e}");
                false
            }
        }
    }

    pub fn vector_index_ready(&self) -> bool {
        vector_index_ready()
    }

    fn apply_pragmas(conn: &Connection) -> AppResult<()> {
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=-32000;
            PRAGMA mmap_size=268435456;
            PRAGMA temp_store=MEMORY;
            PRAGMA busy_timeout=5000;
            PRAGMA foreign_keys=ON;
            ",
        )?;
        Ok(())
    }

    /// Perform a WAL checkpoint to prevent unbounded WAL growth.
    ///
    /// Calls `PRAGMA wal_checkpoint(TRUNCATE)` which moves all WAL frames
    /// into the main database file and truncates the WAL to zero bytes.
    pub fn wal_checkpoint(&self) -> AppResult<()> {
        self.with_conn(|conn| {
            let (busy, log, checkpointed): (i64, i64, i64) =
                conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            if log > 0 || checkpointed > 0 {
                tracing::info!(busy, log, checkpointed, "WAL checkpoint complete");
            }
            Ok(())
        })
    }

    /// Passive WAL checkpoint - safe to call after large writes.
    pub fn wal_checkpoint_passive(&self) -> AppResult<()> {
        self.with_conn(|conn| {
            let _ = conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()));
            Ok(())
        })
    }

    /// Run `PRAGMA optimize` to update query planner statistics.
    pub fn optimize(&self) -> AppResult<()> {
        self.with_conn(|conn| {
            conn.execute_batch("PRAGMA optimize; PRAGMA analysis_limit=1000;")?;
            Ok(())
        })
    }

    /// Execute a closure with a write connection from the pool (for mutations).
    ///
    /// Round-robins across `WRITE_POOL_SIZE` connections so that a short write
    /// (editor auto-save, version snapshot) is not forced to wait behind a long
    /// write (full re-index, batch embedding store).
    pub fn with_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let idx = self.write_idx.fetch_add(1, Ordering::Relaxed) % self.write_pool.len();
        let guard = self.write_pool[idx]
            .lock()
            .map_err(|_| AppError::msg("Database write lock poisoned"))?;
        f(&guard)
    }

    /// Execute a closure with a read connection from the pool (for queries only).
    ///
    /// Round-robin across `READ_POOL_SIZE` connections to allow concurrent reads
    /// when WAL mode is enabled.
    pub fn with_read_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let idx = self.read_idx.fetch_add(1, Ordering::Relaxed) % self.read_pool.len();
        let guard = self.read_pool[idx]
            .lock()
            .map_err(|_| AppError::msg("Database read lock poisoned"))?;
        f(&guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_core_tables() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .flatten()
                .collect();
            assert!(tables.contains(&"files".into()), "missing files table");
            assert!(tables.contains(&"tags".into()), "missing tags table");
            assert!(
                tables.contains(&"file_tags".into()),
                "missing file_tags table"
            );
            assert!(tables.contains(&"chunks".into()), "missing chunks table");
            assert!(
                tables.contains(&"chunk_embeddings".into()),
                "missing chunk_embeddings table"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn wal_mode_enabled() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            // In-memory databases ignore WAL and always return "memory"
            assert!(
                mode.to_lowercase() == "wal" || mode.to_lowercase() == "memory",
                "expected wal or memory, got {mode}"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn with_conn_returns_result() {
        let db = Database::open_in_memory().unwrap();
        let result: AppResult<i64> =
            db.with_conn(|conn| Ok(conn.query_row("SELECT 42", [], |row| row.get(0))?));
        assert_eq!(result.unwrap(), 42);
    }
}
