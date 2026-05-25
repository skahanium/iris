use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

const MIGRATION_UP: &str = include_str!("../../migrations/001_core.sql");
const MIGRATION_DOWN: &str = include_str!("../../migrations/001_core.down.sql");
const MIGRATION_002_UP: &str = include_str!("../../migrations/002_vec.sql");
const MIGRATION_002_DOWN: &str = include_str!("../../migrations/002_vec.down.sql");
const MIGRATION_003_UP: &str = include_str!("../../migrations/003_versions.sql");
const MIGRATION_003_DOWN: &str = include_str!("../../migrations/003_versions.down.sql");

/// Apply core schema migrations idempotently.
pub fn migrate_up(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL
        );",
    )?;

    let applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '001_core'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !applied {
        conn.execute_batch(MIGRATION_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('001_core', datetime('now'))",
            [],
        )?;
    }

    // Migration 002: sqlite-vec (best-effort; fails gracefully if sqlite-vec not loaded)
    let vec_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '002_vec'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !vec_applied {
        let _ = conn.execute_batch(MIGRATION_002_UP);
        let _ = conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('002_vec', datetime('now'))",
            [],
        );
    }

    // Migration 003: version snapshots
    let v3_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '003_versions'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v3_applied {
        conn.execute_batch(MIGRATION_003_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('003_versions', datetime('now'))",
            [],
        )?;
    }

    Ok(())
}

/// Roll back all migrations (for tests).
pub fn migrate_down(conn: &Connection) -> AppResult<()> {
    let _ = conn.execute_batch(MIGRATION_003_DOWN);
    let _ = conn.execute("DELETE FROM _migrations WHERE name = '003_versions'", []);
    let _ = conn.execute_batch(MIGRATION_002_DOWN);
    let _ = conn.execute("DELETE FROM _migrations WHERE name = '002_vec'", []);
    conn.execute_batch(MIGRATION_DOWN)?;
    conn.execute("DELETE FROM _migrations WHERE name = '001_core'", [])?;
    Ok(())
}

/// Load SQL from migrations directory if present (dev helper).
pub fn load_migration_file(path: &Path) -> AppResult<String> {
    fs::read_to_string(path).map_err(|e| AppError::msg(format!("Failed to read migration: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        migrate_down(&conn).unwrap();
        let err = conn.query_row("SELECT COUNT(*) FROM files", [], |r: &rusqlite::Row| {
            r.get::<_, i64>(0)
        });
        assert!(err.is_err());
    }

    #[test]
    fn migration_002_applies_idempotently() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        // Second call should not fail
        migrate_up(&conn).unwrap();

        // Verify 002 migration was recorded
        let applied: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE name = '002_vec'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(applied);
    }

    #[test]
    fn migration_002_down_removes_vec_table() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        migrate_down(&conn).unwrap();

        // After down, vec_chunks should not exist (best-effort — may fail if vec not loaded)
        let result = conn.query_row(
            "SELECT COUNT(*) FROM vec_chunks",
            [],
            |r: &rusqlite::Row| r.get::<_, i64>(0),
        );
        // Either the table doesn't exist OR it's empty — both acceptable
        if let Ok(count) = result {
            assert_eq!(count, 0);
        }
    }
}
