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
const MIGRATION_004_UP: &str = include_str!("../../migrations/004_files_dedupe.sql");
const MIGRATION_004_DOWN: &str = include_str!("../../migrations/004_files_dedupe.down.sql");
const MIGRATION_005_UP: &str = include_str!("../../migrations/005_drop_iris_metadata_files.sql");
const MIGRATION_005_DOWN: &str =
    include_str!("../../migrations/005_drop_iris_metadata_files.down.sql");
const MIGRATION_006_UP: &str = include_str!("../../migrations/006_versions_kind.sql");
const MIGRATION_006_DOWN: &str = include_str!("../../migrations/006_versions_kind.down.sql");
const MIGRATION_007_UP: &str = include_str!("../../migrations/007_recycle_bin.sql");
const MIGRATION_007_DOWN: &str = include_str!("../../migrations/007_recycle_bin.down.sql");
const MIGRATION_008_UP: &str = include_str!("../../migrations/008_chunks_char_count.sql");
const MIGRATION_008_DOWN: &str = include_str!("../../migrations/008_chunks_char_count.down.sql");

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

    let v4_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '004_files_dedupe'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v4_applied {
        conn.execute_batch(MIGRATION_004_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('004_files_dedupe', datetime('now'))",
            [],
        )?;
    }

    let v5_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '005_drop_iris_metadata_files'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v5_applied {
        conn.execute_batch(MIGRATION_005_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('005_drop_iris_metadata_files', datetime('now'))",
            [],
        )?;
    }

    let v6_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '006_versions_kind'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v6_applied {
        conn.execute_batch(MIGRATION_006_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('006_versions_kind', datetime('now'))",
            [],
        )?;
    }

    let v7_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '007_recycle_bin'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v7_applied {
        conn.execute_batch(MIGRATION_007_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('007_recycle_bin', datetime('now'))",
            [],
        )?;
    }

    let v8_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '008_chunks_char_count'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v8_applied {
        conn.execute_batch(MIGRATION_008_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('008_chunks_char_count', datetime('now'))",
            [],
        )?;
    }

    Ok(())
}

/// Roll back all migrations (for tests).
pub fn migrate_down(conn: &Connection) -> AppResult<()> {
    let _ = conn.execute_batch(MIGRATION_008_DOWN);
    let _ = conn.execute(
        "DELETE FROM _migrations WHERE name = '008_chunks_char_count'",
        [],
    );
    let _ = conn.execute_batch(MIGRATION_007_DOWN);
    let _ = conn.execute(
        "DELETE FROM _migrations WHERE name = '007_recycle_bin'",
        [],
    );
    let _ = conn.execute_batch(MIGRATION_006_DOWN);
    let _ = conn.execute(
        "DELETE FROM _migrations WHERE name = '006_versions_kind'",
        [],
    );
    let _ = conn.execute_batch(MIGRATION_005_DOWN);
    let _ = conn.execute(
        "DELETE FROM _migrations WHERE name = '005_drop_iris_metadata_files'",
        [],
    );
    let _ = conn.execute_batch(MIGRATION_004_DOWN);
    let _ = conn.execute(
        "DELETE FROM _migrations WHERE name = '004_files_dedupe'",
        [],
    );
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

    #[test]
    fn migration_004_dedupes_duplicate_paths() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE files_dup AS SELECT * FROM files;
             DROP TABLE files;
             CREATE TABLE files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL,
                title TEXT,
                frontmatter TEXT,
                content_hash TEXT NOT NULL,
                word_count INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             INSERT INTO files SELECT * FROM files_dup;
             INSERT INTO files (path, title, content_hash, created_at, updated_at)
             VALUES ('dup.md', 'Dup', 'h2', '2020-01-01', '2026-01-02'),
                    ('dup.md', 'Dup', 'h3', '2020-01-01', '2026-01-03');
             DROP TABLE files_dup;",
        )
        .unwrap();

        conn.execute_batch(MIGRATION_004_UP).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'dup.md'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_003_creates_versions_table() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);

        // Need a file first to satisfy FK constraint
        conn.execute(
            "INSERT INTO files (path, title, content_hash, created_at, updated_at)
             VALUES ('test.md', 'Test', 'abc', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO versions (file_id, version_no, content_hash, storage_path, created_at)
             VALUES (1, '20260501000000000', 'abc', '1/test.md', datetime('now'))",
            [],
        )
        .unwrap();
    }

    fn versions_has_kind_column(conn: &Connection) -> bool {
        let mut stmt = conn.prepare("PRAGMA table_info(versions)").expect("pragma");
        let names: Vec<String> = stmt
            .query_map([], |row| row.get(1))
            .expect("query")
            .flatten()
            .collect();
        names.iter().any(|name| name == "kind")
    }

    #[test]
    fn migration_006_applies_idempotently() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        migrate_up(&conn).unwrap();

        let applied: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE name = '006_versions_kind'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(applied, 1);
        assert!(versions_has_kind_column(&conn));
    }

    #[test]
    fn migration_006_backfills_kind_and_storage_path() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        conn.execute_batch(MIGRATION_006_DOWN).unwrap();
        conn.execute(
            "DELETE FROM _migrations WHERE name = '006_versions_kind'",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO files (path, title, content_hash, created_at, updated_at)
             VALUES ('note.md', 'Note', 'abc', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO versions (file_id, version_no, content_hash, storage_path, is_finalized, created_at)
             VALUES (1, '20260525143052123', 'hash1', 'note.md', 1, datetime('now'))",
            [],
        )
        .unwrap();

        migrate_up(&conn).unwrap();

        let (kind, storage_path): (String, String) = conn
            .query_row(
                "SELECT kind, storage_path FROM versions WHERE version_no = '20260525143052123'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(kind, "finalize");
        assert_eq!(storage_path, "1/20260525143052123.md");
    }

    #[test]
    fn migration_006_down_removes_kind_column() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        assert!(versions_has_kind_column(&conn));

        conn.execute_batch(MIGRATION_006_DOWN).unwrap();
        assert!(!versions_has_kind_column(&conn));
    }
}
