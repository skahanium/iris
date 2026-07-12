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
const MIGRATION_024_UP: &str = include_str!("../../migrations/024_perf_indexes.sql");
const MIGRATION_024_DOWN: &str = include_str!("../../migrations/024_perf_indexes.down.sql");
const MIGRATION_025_UP: &str = include_str!("../../migrations/025_knowledge_scalar_backfill.sql");
const MIGRATION_025_DOWN: &str =
    include_str!("../../migrations/025_knowledge_scalar_backfill.down.sql");
const MIGRATION_026_UP: &str =
    include_str!("../../migrations/026_skill_closed_loop_diagnostics.sql");
const MIGRATION_026_DOWN: &str =
    include_str!("../../migrations/026_skill_closed_loop_diagnostics.down.sql");
const MIGRATION_027_UP: &str = include_str!("../../migrations/027_agent_permissions.sql");
const MIGRATION_027_DOWN: &str = include_str!("../../migrations/027_agent_permissions.down.sql");
const MIGRATION_028_UP: &str = include_str!("../../migrations/028_multimodal_messages.sql");
const MIGRATION_028_DOWN: &str = include_str!("../../migrations/028_multimodal_messages.down.sql");
const MIGRATION_029_UP: &str = include_str!("../../migrations/029_model_registry.sql");
const MIGRATION_029_DOWN: &str = include_str!("../../migrations/029_model_registry.down.sql");
const MIGRATION_030_UP: &str = include_str!("../../migrations/030_runtime_vault_scope.sql");
const MIGRATION_030_DOWN: &str = include_str!("../../migrations/030_runtime_vault_scope.down.sql");
const MIGRATION_031_UP: &str = include_str!("../../migrations/031_links_single_column_indexes.sql");
const MIGRATION_031_DOWN: &str =
    include_str!("../../migrations/031_links_single_column_indexes.down.sql");
const MIGRATION_032_UP: &str = include_str!("../../migrations/032_agent_tasks.sql");
const MIGRATION_032_DOWN: &str = include_str!("../../migrations/032_agent_tasks.down.sql");
const MIGRATION_033_UP: &str =
    include_str!("../../migrations/033_conversation_memory_deliberation.sql");
const MIGRATION_033_DOWN: &str =
    include_str!("../../migrations/033_conversation_memory_deliberation.down.sql");
const MIGRATION_034_UP: &str = include_str!("../../migrations/034_writing_research_state.sql");
const MIGRATION_034_DOWN: &str =
    include_str!("../../migrations/034_writing_research_state.down.sql");
const MIGRATION_035_UP: &str = include_str!("../../migrations/035_skill_trust_profiles.sql");
const MIGRATION_035_DOWN: &str = include_str!("../../migrations/035_skill_trust_profiles.down.sql");
const MIGRATION_036_UP: &str =
    include_str!("../../migrations/036_session_message_evidence_packets.sql");
const MIGRATION_036_DOWN: &str =
    include_str!("../../migrations/036_session_message_evidence_packets.down.sql");
const MIGRATION_037_UP: &str = include_str!("../../migrations/037_session_evidence.sql");
const MIGRATION_037_DOWN: &str = include_str!("../../migrations/037_session_evidence.down.sql");
const MIGRATION_038_UP: &str = include_str!("../../migrations/038_attachments.sql");
const MIGRATION_038_DOWN: &str = include_str!("../../migrations/038_attachments.down.sql");
const MIGRATION_039_UP: &str = include_str!("../../migrations/039_workspace_media.sql");
const MIGRATION_039_DOWN: &str = include_str!("../../migrations/039_workspace_media.down.sql");
const MIGRATION_040_UP: &str = include_str!("../../migrations/040_mcp_runtime_registry.sql");
const MIGRATION_040_DOWN: &str = include_str!("../../migrations/040_mcp_runtime_registry.down.sql");
const MIGRATION_041_UP: &str =
    include_str!("../../migrations/041_mcp_transport_https_contract.sql");
const MIGRATION_041_DOWN: &str =
    include_str!("../../migrations/041_mcp_transport_https_contract.down.sql");
const MIGRATION_042_UP: &str = include_str!("../../migrations/042_reign_in_ai_capabilities.sql");
const MIGRATION_042_DOWN: &str =
    include_str!("../../migrations/042_reign_in_ai_capabilities.down.sql");
const MIGRATION_043_UP: &str = include_str!("../../migrations/043_chunk_retrieval_metadata.sql");
const MIGRATION_043_DOWN: &str =
    include_str!("../../migrations/043_chunk_retrieval_metadata.down.sql");
const MIGRATION_044_UP: &str = include_str!("../../migrations/044_embedding_generation_v2.sql");
const MIGRATION_044_DOWN: &str =
    include_str!("../../migrations/044_embedding_generation_v2.down.sql");
const MIGRATION_045_UP: &str = include_str!("../../migrations/045_metadata_fts.sql");
const MIGRATION_045_DOWN: &str = include_str!("../../migrations/045_metadata_fts.down.sql");
const MIGRATION_046_UP: &str = include_str!("../../migrations/046_auxiliary_embeddings_v2.sql");
const MIGRATION_046_DOWN: &str =
    include_str!("../../migrations/046_auxiliary_embeddings_v2.down.sql");
const MIGRATION_047_UP: &str = include_str!("../../migrations/047_agent_run_foundation.sql");
const MIGRATION_047_DOWN: &str = include_str!("../../migrations/047_agent_run_foundation.down.sql");
const MIGRATION_048_UP: &str = include_str!("../../migrations/048_agent_run_confirmations.sql");
const MIGRATION_048_DOWN: &str =
    include_str!("../../migrations/048_agent_run_confirmations.down.sql");

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
    apply_migration(
        conn,
        "021_skill_lifecycle_metadata",
        MIGRATION_021_UP,
        false,
    )?;
    apply_migration(conn, "022_session_expiry", MIGRATION_022_UP, false)?;
    apply_migration(conn, "023_file_lock", MIGRATION_023_UP, false)?;
    apply_migration(conn, "024_perf_indexes", MIGRATION_024_UP, false)?;
    apply_migration(
        conn,
        "025_knowledge_scalar_backfill",
        MIGRATION_025_UP,
        false,
    )?;
    apply_migration(
        conn,
        "026_skill_closed_loop_diagnostics",
        MIGRATION_026_UP,
        false,
    )?;
    apply_migration(conn, "027_agent_permissions", MIGRATION_027_UP, false)?;
    apply_migration(conn, "028_multimodal_messages", MIGRATION_028_UP, false)?;
    apply_migration(conn, "029_model_registry", MIGRATION_029_UP, false)?;
    apply_migration(conn, "030_runtime_vault_scope", MIGRATION_030_UP, false)?;
    apply_migration(
        conn,
        "031_links_single_column_indexes",
        MIGRATION_031_UP,
        false,
    )?;
    apply_migration(conn, "032_agent_tasks", MIGRATION_032_UP, false)?;
    apply_migration(
        conn,
        "033_conversation_memory_deliberation",
        MIGRATION_033_UP,
        false,
    )?;
    apply_migration(conn, "034_writing_research_state", MIGRATION_034_UP, false)?;
    apply_migration(conn, "035_skill_trust_profiles", MIGRATION_035_UP, false)?;
    apply_migration(
        conn,
        "036_session_message_evidence_packets",
        MIGRATION_036_UP,
        false,
    )?;
    apply_migration(conn, "037_session_evidence", MIGRATION_037_UP, false)?;
    apply_migration(conn, "038_attachments", MIGRATION_038_UP, false)?;
    apply_migration(conn, "039_workspace_media", MIGRATION_039_UP, false)?;
    apply_migration(conn, "040_mcp_runtime_registry", MIGRATION_040_UP, false)?;
    apply_migration(
        conn,
        "041_mcp_transport_https_contract",
        MIGRATION_041_UP,
        false,
    )?;
    apply_migration(
        conn,
        "042_reign_in_ai_capabilities",
        MIGRATION_042_UP,
        false,
    )?;
    apply_migration(
        conn,
        "043_chunk_retrieval_metadata",
        MIGRATION_043_UP,
        false,
    )?;
    apply_migration(conn, "044_embedding_generation_v2", MIGRATION_044_UP, false)?;
    apply_migration(conn, "045_metadata_fts", MIGRATION_045_UP, false)?;
    apply_migration(conn, "046_auxiliary_embeddings_v2", MIGRATION_046_UP, false)?;
    apply_migration(conn, "047_agent_run_foundation", MIGRATION_047_UP, false)?;
    apply_migration(conn, "048_agent_run_confirmations", MIGRATION_048_UP, false)?;

    Ok(())
}

fn rollback_migration(conn: &Connection, name: &str, sql: &str) {
    let _ = conn.execute_batch(sql);
    let _ = conn.execute("DELETE FROM _migrations WHERE name = ?1", [name]);
}

/// Roll back all migrations in strict reverse order (for tests).
pub fn migrate_down(conn: &Connection) -> AppResult<()> {
    rollback_migration(conn, "048_agent_run_confirmations", MIGRATION_048_DOWN);
    rollback_migration(conn, "047_agent_run_foundation", MIGRATION_047_DOWN);
    rollback_migration(conn, "046_auxiliary_embeddings_v2", MIGRATION_046_DOWN);
    rollback_migration(conn, "045_metadata_fts", MIGRATION_045_DOWN);
    rollback_migration(conn, "044_embedding_generation_v2", MIGRATION_044_DOWN);
    rollback_migration(conn, "043_chunk_retrieval_metadata", MIGRATION_043_DOWN);
    rollback_migration(conn, "042_reign_in_ai_capabilities", MIGRATION_042_DOWN);
    rollback_migration(conn, "041_mcp_transport_https_contract", MIGRATION_041_DOWN);
    rollback_migration(conn, "040_mcp_runtime_registry", MIGRATION_040_DOWN);
    rollback_migration(conn, "039_workspace_media", MIGRATION_039_DOWN);
    rollback_migration(conn, "038_attachments", MIGRATION_038_DOWN);
    rollback_migration(conn, "037_session_evidence", MIGRATION_037_DOWN);
    rollback_migration(
        conn,
        "036_session_message_evidence_packets",
        MIGRATION_036_DOWN,
    );
    rollback_migration(conn, "035_skill_trust_profiles", MIGRATION_035_DOWN);
    rollback_migration(conn, "034_writing_research_state", MIGRATION_034_DOWN);
    rollback_migration(
        conn,
        "033_conversation_memory_deliberation",
        MIGRATION_033_DOWN,
    );
    rollback_migration(conn, "032_agent_tasks", MIGRATION_032_DOWN);
    rollback_migration(conn, "031_links_single_column_indexes", MIGRATION_031_DOWN);
    rollback_migration(conn, "030_runtime_vault_scope", MIGRATION_030_DOWN);
    rollback_migration(conn, "029_model_registry", MIGRATION_029_DOWN);
    rollback_migration(conn, "028_multimodal_messages", MIGRATION_028_DOWN);
    rollback_migration(conn, "027_agent_permissions", MIGRATION_027_DOWN);
    rollback_migration(
        conn,
        "026_skill_closed_loop_diagnostics",
        MIGRATION_026_DOWN,
    );
    rollback_migration(conn, "025_knowledge_scalar_backfill", MIGRATION_025_DOWN);
    rollback_migration(conn, "024_perf_indexes", MIGRATION_024_DOWN);
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

        // After down, vec_chunks should not exist (best-effort - may fail if vec not loaded)
        let result = conn.query_row(
            "SELECT COUNT(*) FROM vec_chunks",
            [],
            |r: &rusqlite::Row| r.get::<_, i64>(0),
        );
        // Either the table doesn't exist OR it's empty - both acceptable
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
    fn migration_024_creates_perf_indexes() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for index in [
            "idx_versions_file_kind_created",
            "idx_chunks_file_index",
            "idx_files_path_not_classified",
        ] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?1",
                    [index],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {index}");
        }
    }

    #[test]
    fn migration_025_creates_scalar_knowledge_tables_without_vec() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in [
            "semantic_anchors",
            "regulation_index",
            "genre_templates",
            "block_links",
        ] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {table}");
        }
    }

    #[test]
    fn migration_026_legacy_skill_closed_loop_tables_are_removed_by_reign_in() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in ["skill_diagnostics", "skill_storage"] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(!has, "{table} must be removed by reign-in migration");
        }
    }

    #[test]
    fn migration_026_roundtrip_final_state_has_no_legacy_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        assert!(!conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_diagnostics'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap());
    }

    #[test]
    fn migration_027_creates_agent_permission_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in ["agent_permission_grants", "agent_permission_audit"] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {table}");
        }

        conn.execute(
            "INSERT INTO ai_traces (request_id, scene, status, created_at)
             VALUES ('req-perm-1', 'drafting_assist', 'running', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_permission_grants
             (permission_name, decision, scope_kind, scope_value, risk_level, skill_id)
             VALUES ('vault.write.patch', 'allow_session', 'vault', 'current', 'medium', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_permission_audit
             (request_id, skill_id, tool_name, permission_name, decision, scope_summary, risk_level, result_status)
             VALUES ('req-perm-1', NULL, 'replace_selection', 'vault.write.patch', 'allow_once', 'path=notes/a.md', 'medium', 'pending')",
            [],
        )
        .unwrap();

        let summary: String = conn
            .query_row(
                "SELECT scope_summary FROM agent_permission_audit WHERE request_id = 'req-perm-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(summary, "path=notes/a.md");
    }

    #[test]
    fn migration_027_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        assert!(conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_permission_audit'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap());

        rollback_migration(&conn, "027_agent_permissions", MIGRATION_027_DOWN);

        let gone: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_permission_audit'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!gone);
    }

    #[test]
    fn migration_029_creates_model_registry() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='llm_model_registry'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_table, "missing llm_model_registry table");

        let has_provider_index: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_llm_model_registry_provider'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_provider_index, "missing provider index");

        conn.execute(
            "INSERT INTO llm_model_registry
             (provider_id, model_id, display_name, source, stale, first_seen_at, last_seen_at,
              last_refreshed_at, user_confirmed_capabilities)
             VALUES ('custom', 'model-a', 'Model A', 'provider_discovered', 0,
                     datetime('now'), datetime('now'), datetime('now'), '[]')",
            [],
        )
        .unwrap();

        let source: String = conn
            .query_row(
                "SELECT source FROM llm_model_registry WHERE provider_id = 'custom' AND model_id = 'model-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "provider_discovered");
    }

    #[test]
    fn migration_030_adds_vault_scope_columns_to_runtime_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in [
            "sessions",
            "session_messages",
            "ai_memories",
            "knowledge_deposits",
            "user_profile",
            "web_page_cache",
            "search_cache",
        ] {
            let has_column: bool = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = 'vault_id'"
                    ),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap();
            assert!(has_column, "missing vault_id on {table}");
        }
    }

    #[test]
    fn migration_036_adds_session_message_evidence_packets() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let has_column: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('session_messages') WHERE name = 'evidence_packets'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap();

        assert!(has_column, "missing evidence_packets on session_messages");
    }
    #[test]
    fn migration_037_creates_session_evidence_without_body_snapshots() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='session_evidence'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap();
        assert!(table_exists, "missing session_evidence table");

        let columns = conn
            .prepare("PRAGMA table_info(session_evidence)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();

        for required in [
            "session_id",
            "citation_index",
            "citation_label",
            "packet_key",
            "message_seq_first",
            "source_type",
            "title",
            "source_path",
            "source_span_start",
            "source_span_end",
            "heading_path",
            "content_hash",
            "retrieval_reason",
            "score",
            "confidence",
            "url",
            "normalized_url",
            "domain",
            "retrieved_at",
            "search_backend",
            "source_rank",
            "failure_reason",
            "retired_at",
        ] {
            assert!(
                columns.contains(&required.to_string()),
                "missing {required}"
            );
        }

        for forbidden in [
            "body",
            "content",
            "excerpt",
            "snapshot",
            "note_content",
            "page_body",
            "page_excerpt",
            "web_snapshot",
        ] {
            assert!(
                !columns.contains(&forbidden.to_string()),
                "session_evidence must not store {forbidden}"
            );
        }

        let foreign_keys = conn
            .prepare("PRAGMA foreign_key_list(session_evidence)")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert!(
            foreign_keys.iter().any(|(table, from, to, on_delete)| {
                table == "sessions"
                    && from == "session_id"
                    && to == "id"
                    && on_delete.eq_ignore_ascii_case("CASCADE")
            }),
            "session_evidence.session_id must cascade with sessions.id"
        );

        conn.execute(
            "INSERT INTO sessions (session_key, scene, created_at, updated_at)
             VALUES ('evidence-session', 'knowledge_lookup', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO session_evidence
             (session_id, citation_index, citation_label, packet_key, message_seq_first,
              source_type, title, source_path, content_hash, created_at)
             VALUES (?1, 1, '[C1]', 'local:path:hash', 2,
                     'local', 'Source', 'source.md', 'hash', datetime('now'))",
            [session_id],
        )
        .unwrap();

        let duplicate_label = conn.execute(
            "INSERT INTO session_evidence
             (session_id, citation_index, citation_label, packet_key, message_seq_first,
              source_type, title, created_at)
             VALUES (?1, 2, '[C1]', 'local:other:hash', 2,
                     'local', 'Other', datetime('now'))",
            [session_id],
        );
        assert!(
            duplicate_label.is_err(),
            "citation_label must be unique per session"
        );

        let duplicate_packet = conn.execute(
            "INSERT INTO session_evidence
             (session_id, citation_index, citation_label, packet_key, message_seq_first,
              source_type, title, created_at)
             VALUES (?1, 2, '[C2]', 'local:path:hash', 2,
                     'local', 'Duplicate', datetime('now'))",
            [session_id],
        );
        assert!(
            duplicate_packet.is_err(),
            "packet_key must be unique per session"
        );

        conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
            .unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            remaining, 0,
            "session_evidence must cascade on session delete"
        );
    }

    #[test]
    fn migration_038_creates_attachment_refs() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attachment_refs'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap();
        assert!(table_exists, "missing attachment_refs table");

        let columns = conn
            .prepare("PRAGMA table_info(attachment_refs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();

        for required in [
            "id",
            "source_path",
            "target_path",
            "ref_kind",
            "created_at",
            "updated_at",
        ] {
            assert!(
                columns.contains(&required.to_string()),
                "missing {required}"
            );
        }

        conn.execute(
            "INSERT INTO attachment_refs
             (source_path, target_path, ref_kind, created_at, updated_at)
             VALUES ('notes/a.md', 'media/image.png', 'embed', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        let duplicate = conn.execute(
            "INSERT INTO attachment_refs
             (source_path, target_path, ref_kind, created_at, updated_at)
             VALUES ('notes/a.md', 'media/image.png', 'embed', datetime('now'), datetime('now'))",
            [],
        );
        assert!(
            duplicate.is_err(),
            "attachment refs should be unique per source/target/kind"
        );
    }

    #[test]
    fn migration_043_adds_chunk_retrieval_metadata() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let columns = conn
            .prepare("PRAGMA table_info(chunks)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();

        for required in ["heading_path", "source_start", "source_end", "content_hash"] {
            assert!(
                columns.contains(&required.to_string()),
                "missing chunks.{required}"
            );
        }
    }

    #[test]
    fn migration_032_creates_agent_task_tables_with_session_lifecycle() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in ["agent_tasks", "agent_task_steps", "agent_task_events"] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {table}");
        }

        conn.execute(
            "INSERT INTO sessions (session_key, scene, created_at, updated_at)
             VALUES ('task-session', 'knowledge_lookup', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO agent_tasks
             (task_id, request_id, session_id, kind, status, user_goal_summary, budget_policy_json, created_at, updated_at)
             VALUES ('task-1', 'req-task-1', ?1, 'lightweight', 'running', 'short summary', '{}', datetime('now'), datetime('now'))",
            [session_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_task_steps
             (task_id, step_seq, kind, status, input_summary, output_summary, checkpoint_json, created_at, updated_at)
             VALUES ('task-1', 1, 'respond', 'completed', 'input summary', 'output summary',
                     '{\"summary\":\"safe\",\"packet_ids\":[]}', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_task_events
             (task_id, event_type, message, payload_json, created_at)
             VALUES ('task-1', 'status', 'started', '{}', datetime('now'))",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
            .unwrap();

        for table in ["agent_tasks", "agent_task_steps", "agent_task_events"] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} should cascade with its session task");
        }
    }

    #[test]
    fn migration_032_agent_task_checkpoint_is_summary_shaped() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let mut columns = conn
            .prepare("PRAGMA table_info(agent_task_steps)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        columns.sort();

        assert!(columns.contains(&"checkpoint_json".to_string()));
        assert!(columns.contains(&"input_summary".to_string()));
        assert!(columns.contains(&"output_summary".to_string()));
        assert!(!columns.contains(&"full_prompt".to_string()));
        assert!(!columns.contains(&"full_messages".to_string()));
        assert!(!columns.contains(&"note_content".to_string()));
    }

    #[test]
    fn migration_047_creates_unified_agent_run_schema_without_legacy_routing_fields() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in ["agent_runs", "agent_run_steps", "agent_run_events"] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap();
            assert!(exists, "missing {table}");
        }

        let run_columns = conn
            .prepare("PRAGMA table_info(agent_runs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        for required in [
            "run_id",
            "client_request_id",
            "session_id",
            "turn_id",
            "status",
            "state_version",
            "effect",
            "effort",
            "security_domain",
            "risk",
            "envelope_json",
            "goal_summary",
            "budget_policy_json",
            "provider_route_summary_json",
            "stage_metrics_json",
            "token_input",
            "token_output",
            "error_code",
            "safe_error_message",
            "created_at",
            "updated_at",
            "completed_at",
        ] {
            assert!(
                run_columns.contains(&required.to_string()),
                "missing {required}"
            );
        }
        for forbidden in ["scene", "note_path", "document_path", "checkpoint"] {
            assert!(
                !run_columns.contains(&forbidden.to_string()),
                "agent_runs must not contain legacy {forbidden}"
            );
        }

        conn.execute(
            "INSERT INTO sessions (session_key, scene, created_at, updated_at)
             VALUES ('run-schema-session', 'knowledge_lookup', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO agent_runs
             (run_id, client_request_id, session_id, turn_id, status, state_version,
              effect, effort, security_domain, risk, envelope_json, goal_summary,
              budget_policy_json, provider_route_summary_json, stage_metrics_json,
              token_input, token_output, created_at, updated_at)
             VALUES ('run-1', 'request-1', ?1, 'turn-1', 'accepted', 0,
                     'answer', 'direct', 'normal', 'read_only', '{}', 'summary',
                     '{}', '{}', '{}', 0, 0, datetime('now'), datetime('now'))",
            [session_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_run_events
             (run_id, event_seq, state_version, event_type, payload_json, created_at)
             VALUES ('run-1', 1, 0, 'accepted', '{}', datetime('now'))",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO agent_run_events
                 (run_id, event_seq, state_version, event_type, payload_json, created_at)
                 VALUES ('run-1', 1, 0, 'accepted', '{}', datetime('now'))",
                [],
            )
            .is_err());

        let evidence_columns = conn
            .prepare("PRAGMA table_info(session_evidence)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        for required in ["origin_run_id", "material_role", "stale", "bounded_excerpt"] {
            assert!(
                evidence_columns.contains(&required.to_string()),
                "missing session_evidence.{required}"
            );
        }

        conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
            .unwrap();
        let events: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_run_events", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(events, 0, "run events must cascade with their session");
        let foreign_key_issues: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_key_issues, 0);
    }

    #[test]
    fn migration_047_roundtrips_and_preserves_database_integrity() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        assert!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'agent_runs'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
                > 0
        );

        rollback_migration(&conn, "047_agent_run_foundation", MIGRATION_047_DOWN);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'agent_runs'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );

        apply_migration(&conn, "047_agent_run_foundation", MIGRATION_047_UP, false).unwrap();
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
        let foreign_key_issues: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_key_issues, 0);
    }

    #[test]
    fn migration_048_creates_frozen_run_confirmation_facts() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(agent_run_confirmations)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        for required in [
            "confirmation_id",
            "run_id",
            "plan_hash",
            "plan_json",
            "expires_at",
            "status",
        ] {
            assert!(
                columns.contains(&required.to_string()),
                "missing {required}"
            );
        }
    }

    #[test]
    fn migration_033_creates_summary_shaped_memory_and_deliberation_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in ["conversation_summaries", "deliberation_states"] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {table}");

            let columns = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .flatten()
                .collect::<Vec<_>>();
            assert!(!columns.contains(&"full_prompt".to_string()));
            assert!(!columns.contains(&"full_messages".to_string()));
            assert!(!columns.contains(&"note_content".to_string()));
        }

        conn.execute(
            "INSERT INTO sessions (session_key, scene, created_at, updated_at)
             VALUES ('memory-session', 'knowledge_lookup', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO ai_traces (request_id, scene, status, created_at)
             VALUES ('req-deliberation', 'knowledge_lookup', 'running', datetime('now'))",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO conversation_summaries
             (session_id, seq_start, seq_end, content_hash, goal_summary,
              preference_summary, decision_summary, open_threads_summary, created_at, updated_at)
             VALUES (?1, 1, 48,
                     '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
                     'goal', 'preference', 'decision', 'open', datetime('now'), datetime('now'))",
            [session_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO deliberation_states
             (request_id, session_id, current_goal, plan_outline_json, assumptions_json,
              open_questions_json, evidence_gaps_json, verification_json, status, created_at, updated_at)
             VALUES ('req-deliberation', ?1, 'goal', '[]', '[]', '[]', '[]',
                     '{\"passed\":true,\"items\":[]}', 'verified', datetime('now'), datetime('now'))",
            [session_id],
        )
        .unwrap();
    }

    #[test]
    fn migration_034_creates_summary_shaped_writing_and_research_state_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in ["writing_states", "research_states"] {
            let has: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {table}");

            let columns = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .flatten()
                .collect::<Vec<_>>();
            assert!(!columns.contains(&"full_content".to_string()));
            assert!(!columns.contains(&"note_content".to_string()));
            assert!(!columns.contains(&"raw_selection".to_string()));
            assert!(!columns.contains(&"raw_web_page".to_string()));
        }

        conn.execute(
            "INSERT INTO ai_traces (request_id, scene, status, created_at)
             VALUES ('req-writing-state', 'drafting_assist', 'completed', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ai_traces (request_id, scene, status, created_at)
             VALUES ('req-research-state', 'research_synthesis', 'completed', datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO writing_states
             (request_id, target_path, draft_version_hash, document_goal, audience, genre,
              structure_outline_json, key_arguments_json, material_packet_ids_json,
              citation_labels_json, style_constraints_json, revision_records_json,
              created_at, updated_at)
             VALUES ('req-writing-state', 'Drafts/report.md', 'hash', 'goal', 'audience', 'memo',
                     '[]', '[]', '[\"ev-1\"]', '[\"S1\"]', '[\"style\"]', '[]',
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO research_states
             (request_id, research_question, sub_questions_json, sources_json,
              credibility_summary, freshness_summary, conflicts_json, counter_arguments_json,
              evidence_gaps_json, preliminary_conclusions_json, created_at, updated_at)
             VALUES ('req-research-state', 'topic', '[]', '[]', 'cred', 'fresh',
                     '[]', '[]', '[]', '[]', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
    }

    #[test]
    fn reign_in_provider_schema_has_minimal_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let provider_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = 'web_evidence_providers'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap();
        assert!(provider_exists, "missing web_evidence_providers table");

        for table in [
            "mcp_tool_inventory",
            "mcp_health_events",
            "skill_runtime_requirements",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap();
            assert!(!exists, "{table} must be removed by reign-in migration");
        }
    }
    #[test]
    fn migration_041_converts_legacy_http_transport_to_https_contract() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE mcp_server_catalog (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                transport TEXT NOT NULL CHECK (transport IN ('stdio', 'http', 'sse')),
                command TEXT,
                args_json TEXT NOT NULL DEFAULT '[]',
                url TEXT,
                env_schema_json TEXT NOT NULL DEFAULT '{}',
                capability_tags_json TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL DEFAULT 'user',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO mcp_server_catalog
                (id, display_name, transport, args_json, url, env_schema_json,
                 capability_tags_json, source)
            VALUES
                ('remote', 'Remote MCP', 'http', '[]', 'https://example.com/mcp',
                 '{}', '[]', 'test');
            ",
        )
        .unwrap();

        conn.execute_batch(MIGRATION_041_UP).unwrap();
        let transport: String = conn
            .query_row(
                "SELECT transport FROM mcp_server_catalog WHERE id = 'remote'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(transport, "https");
        let err = conn
            .execute(
                "INSERT INTO mcp_server_catalog
                 (id, display_name, transport, args_json, env_schema_json, capability_tags_json)
                 VALUES ('legacy', 'Legacy', 'http', '[]', '{}', '[]')",
                [],
            )
            .unwrap_err();
        assert!(err.to_string().contains("CHECK constraint failed"));
    }
    #[test]
    fn migration_registry_covers_all_sql_files() {
        use std::collections::BTreeSet;

        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let disk: BTreeSet<String> = fs::read_dir(&migrations_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".sql") && !name.ends_with(".down.sql"))
            .filter_map(|name| name.strip_suffix(".sql").map(str::to_string))
            .collect();

        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let applied: BTreeSet<String> = conn
            .prepare("SELECT name FROM _migrations")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .flatten()
            .collect();
        let optional: BTreeSet<String> = ["002_vec", "010_knowledge_index"]
            .into_iter()
            .map(str::to_string)
            .collect();

        let missing: Vec<_> = disk.difference(&applied).collect();
        let missing_required: Vec<_> = missing
            .into_iter()
            .filter(|name| !optional.contains(name.as_str()))
            .collect();
        assert!(
            missing_required.is_empty(),
            "unregistered required migrations: {missing_required:?}"
        );

        let missing_down: Vec<_> = disk
            .iter()
            .filter(|name| !migrations_dir.join(format!("{name}.down.sql")).exists())
            .collect();
        assert!(
            missing_down.is_empty(),
            "migrations without down scripts: {missing_down:?}"
        );
    }

    #[test]
    fn migration_018_legacy_skill_install_sources_is_removed_by_reign_in() {
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
        assert!(
            !has,
            "skill_install_sources must be removed by reign-in migration"
        );
    }

    #[test]
    fn migration_018_roundtrip_final_state_has_no_legacy_table() {
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
        assert!(!has);
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
