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
            assert!(tables.contains(&"file_tags".into()), "missing file_tags table");
            assert!(tables.contains(&"chunks".into()), "missing chunks table");
            assert!(tables.contains(&"chunk_embeddings".into()), "missing chunk_embeddings table");
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
            assert_eq!(mode.to_lowercase(), "wal");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn with_conn_returns_result() {
        let db = Database::open_in_memory().unwrap();
        let result: AppResult<i64> = db.with_conn(|conn| {
            Ok(conn.query_row("SELECT 42", [], |row| row.get(0))?)
        });
        assert_eq!(result.unwrap(), 42);
    }
}
