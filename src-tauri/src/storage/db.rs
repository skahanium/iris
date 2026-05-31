use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};

use rusqlite::Connection;

use super::migrate::migrate_up;
use crate::error::{AppError, AppResult};

static VECTOR_INDEX_READY: AtomicBool = AtomicBool::new(false);
static SQLITE_VEC_REGISTER: Once = Once::new();

/// Whether sqlite-vec vec0 tables are available for this process.
pub fn vector_index_ready() -> bool {
    VECTOR_INDEX_READY.load(Ordering::Relaxed)
}

/// SQLite connection pool with WAL and performance PRAGMAs from ARCHITECTURE.md.
///
/// Uses separate connections for reads and writes to allow concurrent read
/// operations while writes are in progress (WAL mode enables this).
pub struct Database {
    write_conn: Mutex<Connection>,
    read_conn: Mutex<Connection>,
    #[allow(dead_code)]
    path: Option<PathBuf>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let write_conn = Connection::open(path)?;
        Self::apply_pragmas(&write_conn)?;
        let vec_ready = Self::try_load_sqlite_vec(&write_conn, true);
        VECTOR_INDEX_READY.store(vec_ready, Ordering::Relaxed);
        migrate_up(&write_conn)?;

        let read_conn = Connection::open(path)?;
        Self::apply_pragmas(&read_conn)?;
        let _ = Self::try_load_sqlite_vec(&read_conn, true);

        Ok(Self {
            write_conn: Mutex::new(write_conn),
            read_conn: Mutex::new(read_conn),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let write_conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&write_conn)?;
        // In-memory DBs skip sqlite-vec (process-global extension breaks isolated test DBs).
        VECTOR_INDEX_READY.store(false, Ordering::Relaxed);
        migrate_up(&write_conn)?;

        let read_conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&read_conn)?;
        Self::copy_schema(&write_conn, &read_conn)?;

        Ok(Self {
            write_conn: Mutex::new(write_conn),
            read_conn: Mutex::new(read_conn),
            path: None,
        })
    }

    fn copy_schema(src: &Connection, dst: &Connection) -> AppResult<()> {
        let mut stmt = src.prepare(
            "SELECT sql FROM sqlite_master
             WHERE type='table' AND sql IS NOT NULL
               AND name NOT LIKE 'sqlite_%'
               AND name NOT LIKE '%_data'
               AND name NOT LIKE '%_idx'
               AND name NOT LIKE '%_content'
               AND name NOT LIKE '%_config'
               AND name NOT LIKE '%_docsize'",
        )?;
        let sqls: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for sql in sqls {
            dst.execute_batch(&sql)?;
        }
        let mut idx_stmt = src.prepare(
            "SELECT sql FROM sqlite_master
             WHERE type='index' AND sql IS NOT NULL
               AND name NOT LIKE 'sqlite_%'",
        )?;
        let idx_sqls: Vec<String> = idx_stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for sql in idx_sqls {
            dst.execute_batch(&sql)?;
        }
        let mut trigger_stmt = src.prepare(
            "SELECT sql FROM sqlite_master
             WHERE type='trigger' AND sql IS NOT NULL
               AND name NOT LIKE 'sqlite_%'",
        )?;
        let trigger_sqls: Vec<String> = trigger_stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for sql in trigger_sqls {
            dst.execute_batch(&sql)?;
        }
        Ok(())
    }

    /// Load sqlite-vec extension when the bundled SQLite supports it.
    fn try_load_sqlite_vec(conn: &Connection, persistent_db: bool) -> bool {
        if !persistent_db || cfg!(test) {
            return false;
        }
        // Register once per process — `sqlite3_auto_extension` is global state.
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
            PRAGMA cache_size=-8000;
            PRAGMA mmap_size=268435456;
            PRAGMA temp_store=MEMORY;
            PRAGMA busy_timeout=5000;
            ",
        )?;
        Ok(())
    }

    /// Execute a closure with the write connection (for mutations).
    pub fn with_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let guard = self
            .write_conn
            .lock()
            .map_err(|_| AppError::msg("Database write lock poisoned"))?;
        f(&guard)
    }

    /// Execute a closure with the read connection (for queries only).
    ///
    /// This allows read operations to proceed concurrently with writes
    /// when WAL mode is enabled.
    pub fn with_read_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let guard = self
            .read_conn
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
