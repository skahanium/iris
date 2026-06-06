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
const MIGRATION_009_UP: &str = include_str!("../../migrations/009_ai_runtime.sql");
const MIGRATION_009_DOWN: &str = include_str!("../../migrations/009_ai_runtime.down.sql");
const MIGRATION_010_UP: &str = include_str!("../../migrations/010_knowledge_index.sql");
const MIGRATION_010_DOWN: &str = include_str!("../../migrations/010_knowledge_index.down.sql");
const MIGRATION_011_UP: &str = include_str!("../../migrations/011_eval_results.sql");
const MIGRATION_011_DOWN: &str = include_str!("../../migrations/011_eval_results.down.sql");
const MIGRATION_012_UP: &str = include_str!("../../migrations/012_session_title.sql");
const MIGRATION_012_DOWN: &str = include_str!("../../migrations/012_session_title.down.sql");
const MIGRATION_013_UP: &str = include_str!("../../migrations/013_ai_trace_checkpoint.sql");
const MIGRATION_013_DOWN: &str = include_str!("../../migrations/013_ai_trace_checkpoint.down.sql");
const MIGRATION_014_UP: &str = include_str!("../../migrations/014_web_page_cache.sql");
const MIGRATION_014_DOWN: &str = include_str!("../../migrations/014_web_page_cache.down.sql");
const MIGRATION_015_UP: &str = include_str!("../../migrations/015_search_cache.sql");
const MIGRATION_015_DOWN: &str = include_str!("../../migrations/015_search_cache.down.sql");
const MIGRATION_016_UP: &str = include_str!("../../migrations/016_cas_refs.sql");
const MIGRATION_016_DOWN: &str = include_str!("../../migrations/016_cas_refs.down.sql");
const MIGRATION_017_UP: &str = include_str!("../../migrations/017_rename_cascade.sql");
const MIGRATION_017_DOWN: &str = include_str!("../../migrations/017_rename_cascade.down.sql");
const MIGRATION_018_UP: &str = include_str!("../../migrations/018_skill_install_sources.sql");
const MIGRATION_018_DOWN: &str =
    include_str!("../../migrations/018_skill_install_sources.down.sql");
const MIGRATION_019_UP: &str = include_str!("../../migrations/019_skill_activation_index.sql");
const MIGRATION_019_DOWN: &str =
    include_str!("../../migrations/019_skill_activation_index.down.sql");
const MIGRATION_020_UP: &str = include_str!("../../migrations/020_tool_audit.sql");
const MIGRATION_020_DOWN: &str = include_str!("../../migrations/020_tool_audit.down.sql");
const MIGRATION_021_UP: &str = include_str!("../../migrations/021_skill_lifecycle_metadata.sql");
const MIGRATION_021_DOWN: &str =
    include_str!("../../migrations/021_skill_lifecycle_metadata.down.sql");
const MIGRATION_022_UP: &str = include_str!("../../migrations/022_session_expiry.sql");
const MIGRATION_022_DOWN: &str = include_str!("../../migrations/022_session_expiry.down.sql");
const MIGRATION_023_UP: &str = include_str!("../../migrations/023_file_lock.sql");
const MIGRATION_023_DOWN: &str = include_str!("../../migrations/023_file_lock.down.sql");

fn is_applied(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _migrations WHERE name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

fn apply_migration(conn: &Connection, name: &str, sql: &str, best_effort: bool) -> AppResult<()> {
    if is_applied(conn, name) {
        return Ok(());
    }
    conn.execute_batch("BEGIN")?;
    let exec_result = conn.execute_batch(sql);
    match exec_result {
        Ok(()) => {
            conn.execute(
                "INSERT INTO _migrations (name, applied_at) VALUES (?1, datetime('now'))",
                [name],
            )?;
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            if best_effort {
                tracing::warn!("best-effort migration '{name}' failed (skipped): {e}");
                Ok(())
            } else {
                Err(AppError::msg(format!("migration '{name}' failed: {e}")))
            }
        }
    }
}

/// Apply core schema migrations idempotently.
pub fn migrate_up(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL
        );",
    )?;

    apply_migration(conn, "001_core", MIGRATION_UP, false)?;
    apply_migration(conn, "002_vec", MIGRATION_002_UP, true)?;
    apply_migration(conn, "003_versions", MIGRATION_003_UP, false)?;
    apply_migration(conn, "004_files_dedupe", MIGRATION_004_UP, false)?;
    apply_migration(
        conn,
        "005_drop_iris_metadata_files",
        MIGRATION_005_UP,
        false,
    )?;
    apply_migration(conn, "006_versions_kind", MIGRATION_006_UP, false)?;
    apply_migration(conn, "007_recycle_bin", MIGRATION_007_UP, false)?;
    apply_migration(conn, "008_chunks_char_count", MIGRATION_008_UP, false)?;
    apply_migration(conn, "009_ai_runtime", MIGRATION_009_UP, false)?;
    apply_migration(conn, "010_knowledge_index", MIGRATION_010_UP, true)?;
    apply_migration(conn, "011_eval_results", MIGRATION_011_UP, true)?;
    apply_migration(conn, "012_session_title", MIGRATION_012_UP, true)?;
    apply_migration(conn, "013_ai_trace_checkpoint", MIGRATION_013_UP, true)?;
    apply_migration(conn, "014_web_page_cache", MIGRATION_014_UP, true)?;
    apply_migration(conn, "015_search_cache", MIGRATION_015_UP, true)?;
    apply_migration(conn, "016_cas_refs", MIGRATION_016_UP, false)?;
    apply_migration(conn, "017_rename_cascade", MIGRATION_017_UP, false)?;
    apply_migration(conn, "018_skill_install_sources", MIGRATION_018_UP, false)?;
    apply_migration(conn, "019_skill_activation_index", MIGRATION_019_UP, false)?;
    apply_migration(conn, "020_tool_audit", MIGRATION_020_UP, false)?;
    apply_migration(conn, "021_skill_lifecycle_metadata", MIGRATION_021_UP, false)?;
    apply_migration(conn, "022_session_expiry", MIGRATION_022_UP, false)?;
    apply_migration(conn, "023_file_lock", MIGRATION_023_UP, false)?;

    Ok(())
}

fn rollback_migration(conn: &Connection, name: &str, sql: &str) {
    let _ = conn.execute_batch(sql);
    let _ = conn.execute("DELETE FROM _migrations WHERE name = ?1", [name]);
}

/// Roll back all migrations in strict reverse order (for tests).
pub fn migrate_down(conn: &Connection) -> AppResult<()> {
    rollback_migration(conn, "023_file_lock", MIGRATION_023_DOWN);
    rollback_migration(conn, "022_session_expiry", MIGRATION_022_DOWN);
    rollback_migration(conn, "021_skill_lifecycle_metadata", MIGRATION_021_DOWN);
    rollback_migration(conn, "020_tool_audit", MIGRATION_020_DOWN);
    rollback_migration(conn, "019_skill_activation_index", MIGRATION_019_DOWN);
    rollback_migration(conn, "018_skill_install_sources", MIGRATION_018_DOWN);
    rollback_migration(conn, "017_rename_cascade", MIGRATION_017_DOWN);
    rollback_migration(conn, "016_cas_refs", MIGRATION_016_DOWN);
    rollback_migration(conn, "015_search_cache", MIGRATION_015_DOWN);
    rollback_migration(conn, "014_web_page_cache", MIGRATION_014_DOWN);
    rollback_migration(conn, "013_ai_trace_checkpoint", MIGRATION_013_DOWN);
    rollback_migration(conn, "012_session_title", MIGRATION_012_DOWN);
    rollback_migration(conn, "011_eval_results", MIGRATION_011_DOWN);
    rollback_migration(conn, "010_knowledge_index", MIGRATION_010_DOWN);
    rollback_migration(conn, "009_ai_runtime", MIGRATION_009_DOWN);
    rollback_migration(conn, "008_chunks_char_count", MIGRATION_008_DOWN);
    rollback_migration(conn, "007_recycle_bin", MIGRATION_007_DOWN);
    rollback_migration(conn, "006_versions_kind", MIGRATION_006_DOWN);
    rollback_migration(conn, "005_drop_iris_metadata_files", MIGRATION_005_DOWN);
    rollback_migration(conn, "004_files_dedupe", MIGRATION_004_DOWN);
    rollback_migration(conn, "003_versions", MIGRATION_003_DOWN);
    rollback_migration(conn, "002_vec", MIGRATION_002_DOWN);
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

        // Migration 002 is best-effort (depends on sqlite-vec).
        // If sqlite-vec is not loaded, it should NOT be marked as applied.
        // If sqlite-vec IS loaded, it should be marked as applied.
        // Either way, migrate_up should succeed without error.
        let applied: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE name = '002_vec'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        // Verify the vec_chunks table exists iff the migration was recorded
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        assert_eq!(
            applied, table_exists,
            "migration record and table existence must be consistent"
        );
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
                genre TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_locked INTEGER NOT NULL DEFAULT 0
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

    #[test]
    fn migration_009_creates_ai_runtime_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has_sessions: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_sessions, "missing sessions table");

        let has_traces: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ai_traces'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_traces, "missing ai_traces table");

        let has_profile: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_profile'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_profile, "missing user_profile table");

        let has_deposits: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_deposits'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_deposits, "missing knowledge_deposits table");

        // Verify files extended columns exist
        let col_exists = |table: &str, col: &str| -> bool {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .expect("pragma");
            let names: Vec<String> = stmt
                .query_map([], |row| row.get(1))
                .expect("query")
                .flatten()
                .collect();
            names.iter().any(|n| n == col)
        };
        assert!(col_exists("files", "genre"), "missing files.genre");
        assert!(
            col_exists("chunks", "embedding_model"),
            "missing chunks.embedding_model"
        );
    }

    #[test]
    fn migration_009_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let has_sessions: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_sessions);

        let _ = conn.execute_batch(MIGRATION_009_DOWN);
        let _ = conn.execute("DELETE FROM _migrations WHERE name = '009_ai_runtime'", []);

        let still_has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(
            !still_has,
            "sessions should be dropped after down migration"
        );
    }

    #[test]
    fn migration_010_creates_knowledge_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        // Migration 010 is best-effort (depends on sqlite-vec for vec_anchors).
        // If sqlite-vec is not loaded, the entire migration fails and no tables are created.
        // Check if the migration was applied before asserting table existence.
        let applied: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE name = '010_knowledge_index'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if applied {
            for table in &[
                "semantic_anchors",
                "regulation_index",
                "genre_templates",
                "block_links",
            ] {
                let has: bool = conn
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                        ),
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|c| c > 0)
                    .unwrap();
                assert!(has, "missing {table}");
            }
        }
    }

    #[test]
    fn migration_010_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let applied: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM _migrations WHERE name = '010_knowledge_index'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if applied {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='semantic_anchors'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has);

            let _ = conn.execute_batch(MIGRATION_010_DOWN);
            let _ = conn.execute(
                "DELETE FROM _migrations WHERE name = '010_knowledge_index'",
                [],
            );

            let gone: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='semantic_anchors'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(!gone);
        }
    }

    #[test]
    fn migration_018_creates_skill_install_sources() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_install_sources'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has, "missing skill_install_sources table");

        // Verify can insert
        conn.execute(
            "INSERT INTO skill_install_sources (skill_name, scope, source_type, source_url)
             VALUES ('test-skill', 'Vault', 'url', 'https://example.com/SKILL.md')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM skill_install_sources WHERE skill_name = 'test-skill'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_018_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_install_sources'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has);

        let _ = conn.execute_batch(MIGRATION_018_DOWN);
        let _ = conn.execute(
            "DELETE FROM _migrations WHERE name = '018_skill_install_sources'",
            [],
        );

        let gone: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_install_sources'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!gone);
    }

    #[test]
    fn migration_019_creates_skill_activation_index() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_activation_index'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has, "missing skill_activation_index table");

        // Verify can insert
        conn.execute(
            "INSERT INTO skill_activation_index (skill_name, scope, description, keywords)
             VALUES ('test-skill', 'Vault', 'A test skill', 'test skill')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM skill_activation_index WHERE skill_name = 'test-skill'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_019_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_activation_index'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has);

        let _ = conn.execute_batch(MIGRATION_019_DOWN);
        let _ = conn.execute(
            "DELETE FROM _migrations WHERE name = '019_skill_activation_index'",
            [],
        );

        let gone: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_activation_index'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!gone);
    }

    #[test]
    fn migration_020_creates_tool_audit() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tool_audit'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has, "missing tool_audit table");

        // Verify can insert
        conn.execute(
            "INSERT INTO ai_traces (request_id, scene, status, created_at)
             VALUES ('req-1', 'knowledge_lookup', 'running', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tool_audit (request_id, harness_round, tool_name, arguments_summary, success)
             VALUES ('req-1', 1, 'read_note', 'path=test.md', 1)",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tool_audit WHERE request_id = 'req-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_020_tool_audit_references_ai_traces() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let target_table: String = conn
            .query_row("PRAGMA foreign_key_list(tool_audit)", [], |row| row.get(2))
            .unwrap();
        assert_eq!(target_table, "ai_traces");
    }

    #[test]
    fn migration_020_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tool_audit'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has);

        let _ = conn.execute_batch(MIGRATION_020_DOWN);
        let _ = conn.execute("DELETE FROM _migrations WHERE name = '020_tool_audit'", []);

        let gone: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tool_audit'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!gone);
    }

    #[test]
    fn migration_023_adds_is_locked_column_with_default() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        conn.execute(
            "INSERT INTO files (path, title, content_hash, created_at, updated_at) VALUES ('test.md', 'Test', 'h', '2020-01-01', '2020-01-01')",
            [],
        )
        .unwrap();

        let is_locked: i64 = conn
            .query_row(
                "SELECT is_locked FROM files WHERE path = 'test.md'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(is_locked, 0);
    }

    #[test]
    fn migration_023_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has_column: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('files') WHERE name = 'is_locked'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_column);

        rollback_migration(&conn, "023_file_lock", MIGRATION_023_DOWN);

        let gone: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('files') WHERE name = 'is_locked'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!gone);
    }
}
