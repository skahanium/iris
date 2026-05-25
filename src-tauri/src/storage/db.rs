use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use super::migrate::migrate_up;
use crate::error::AppResult;

/// SQLite connection with WAL and performance PRAGMAs from ARCHITECTURE.md.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;
        migrate_up(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&conn)?;
        migrate_up(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
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

    pub fn with_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let guard = self
            .conn
            .lock()
            .map_err(|_| crate::error::AppError::msg("Database lock poisoned"))?;
        f(&guard)
    }
}
