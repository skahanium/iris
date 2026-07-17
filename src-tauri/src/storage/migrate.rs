use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};

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
const MIGRATION_049_UP: &str =
    include_str!("../../migrations/049_document_capability_policies.sql");
const MIGRATION_049_DOWN: &str =
    include_str!("../../migrations/049_document_capability_policies.down.sql");
const MIGRATION_050_UP: &str = include_str!("../../migrations/050_agent_run_explicit_action.sql");
const MIGRATION_050_DOWN: &str =
    include_str!("../../migrations/050_agent_run_explicit_action.down.sql");
const MIGRATION_052_UP: &str =
    include_str!("../../migrations/052_web_evidence_provider_runtime.sql");
const MIGRATION_052_DOWN: &str =
    include_str!("../../migrations/052_web_evidence_provider_runtime.down.sql");
const MIGRATION_053_UP: &str =
    include_str!("../../migrations/053_remove_model_slot_capabilities.sql");
const MIGRATION_053_DOWN: &str =
    include_str!("../../migrations/053_remove_model_slot_capabilities.down.sql");
const MIGRATION_054_UP: &str = include_str!("../../migrations/054_embedding_scheduler.sql");
const MIGRATION_054_DOWN: &str = include_str!("../../migrations/054_embedding_scheduler.down.sql");
const MIGRATION_055_UP: &str =
    include_str!("../../migrations/055_session_message_turn_context.sql");
const MIGRATION_055_DOWN: &str =
    include_str!("../../migrations/055_session_message_turn_context.down.sql");
const MIGRATION_051_UP: &str = include_str!("../../migrations/051_agent_harness_cutover.sql");
const MIGRATION_051_DOWN: &str =
    include_str!("../../migrations/051_agent_harness_cutover.down.sql");

fn is_applied(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _migrations WHERE name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

/// Convert historical packet metadata into evidence-ledger IDs before dropping
/// the duplicate packet column. Only metadata survives; text excerpts and bodies
/// are intentionally ignored.
fn migrate_legacy_evidence_packets(conn: &Connection) -> AppResult<()> {
    let mut statement = conn.prepare(
        "SELECT id, session_id, seq, evidence_packets, evidence_refs_json
         FROM session_messages
         WHERE evidence_packets IS NOT NULL AND trim(evidence_packets) != ''",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (message_id, session_id, sequence, packet_json, existing_refs_json) in rows {
        let Ok(serde_json::Value::Array(items)) =
            serde_json::from_str::<serde_json::Value>(&packet_json)
        else {
            continue;
        };

        let mut refs = existing_refs_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<Vec<serde_json::Value>>(value).ok())
            .unwrap_or_default();

        for (packet_index, item) in items.iter().enumerate() {
            let Some(object) = item.as_object() else {
                continue;
            };
            let web = object.get("web").and_then(serde_json::Value::as_object);
            let source_kind = if object
                .get("source_type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("web"))
                || web.is_some()
            {
                "web"
            } else {
                "local"
            };
            let title = object
                .get("title")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("Historical evidence");
            let source_path = (source_kind == "local")
                .then(|| {
                    object
                        .get("source_path")
                        .and_then(serde_json::Value::as_str)
                })
                .flatten();
            let span = object
                .get("source_span")
                .and_then(serde_json::Value::as_object);
            let span_start = span
                .and_then(|value| value.get("start"))
                .and_then(serde_json::Value::as_i64);
            let span_end = span
                .and_then(|value| value.get("end"))
                .and_then(serde_json::Value::as_i64);
            let heading_path = object
                .get("heading_path")
                .and_then(serde_json::Value::as_str);
            let content_hash = (source_kind == "local")
                .then(|| {
                    object
                        .get("content_hash")
                        .and_then(serde_json::Value::as_str)
                })
                .flatten();
            let retrieval_reason = object
                .get("retrieval_reason")
                .and_then(serde_json::Value::as_str);
            let score = object.get("score").and_then(serde_json::Value::as_f64);
            let url = web
                .and_then(|value| value.get("url"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| object.get("url").and_then(serde_json::Value::as_str));
            let normalized_url = url.map(|value| value.trim().to_ascii_lowercase());
            let identity = if source_kind == "web" {
                normalized_url
                    .clone()
                    .unwrap_or_else(|| format!("message:{message_id}:packet:{packet_index}"))
            } else {
                format!(
                    "{}:{}:{}:{}",
                    source_path.unwrap_or(""),
                    span_start.unwrap_or(-1),
                    span_end.unwrap_or(-1),
                    content_hash.unwrap_or("")
                )
            };
            let packet_key = format!("historical:{source_kind}:{identity}");
            let existing = conn.query_row(
                "SELECT id, citation_label FROM session_evidence
                 WHERE session_id = ?1 AND packet_key = ?2",
                params![session_id, packet_key],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            );
            let (evidence_id, citation_label) = match existing {
                Ok(existing) => existing,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    let citation_index: i64 = conn.query_row(
                        "SELECT COALESCE(MAX(citation_index), 0) + 1
                         FROM session_evidence WHERE session_id = ?1",
                        [session_id],
                        |row| row.get(0),
                    )?;
                    let citation_label = format!("[C{citation_index}]");
                    conn.execute(
                        "INSERT INTO session_evidence
                         (session_id, citation_index, citation_label, packet_key, message_seq_first,
                          source_type, title, source_path, source_span_start, source_span_end,
                          heading_path, content_hash, retrieval_reason, score, url, normalized_url,
                          created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5,
                                 ?6, ?7, ?8, ?9, ?10,
                                 ?11, ?12, ?13, ?14, ?15, ?16,
                                 ?17)",
                        params![
                            session_id,
                            citation_index,
                            citation_label,
                            packet_key,
                            sequence,
                            source_kind,
                            title,
                            source_path,
                            span_start,
                            span_end,
                            heading_path,
                            content_hash,
                            retrieval_reason,
                            score,
                            url,
                            normalized_url,
                            chrono::Utc::now().to_rfc3339(),
                        ],
                    )?;
                    (conn.last_insert_rowid(), citation_label)
                }
                Err(error) => return Err(error.into()),
            };
            let evidence_id = evidence_id.to_string();
            if refs.iter().any(|entry| {
                entry.get("evidenceId").and_then(serde_json::Value::as_str)
                    == Some(evidence_id.as_str())
            }) {
                continue;
            }
            refs.push(serde_json::json!({
                "evidenceId": evidence_id,
                "sourceKind": source_kind,
                "title": title,
                "displayLabel": citation_label,
                "stale": false,
            }));
        }

        conn.execute(
            "UPDATE session_messages SET evidence_refs_json = ?1 WHERE id = ?2",
            params![serde_json::to_string(&refs)?, message_id],
        )?;
    }
    Ok(())
}

/// Apply the one-way legacy AI persistence cutover in a single transaction.
fn apply_agent_harness_cutover(conn: &Connection) -> AppResult<()> {
    const NAME: &str = "051_agent_harness_cutover";
    if is_applied(conn, NAME) {
        return Ok(());
    }

    let foreign_keys_enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys = OFF; BEGIN IMMEDIATE")?;
    let result = (|| -> AppResult<()> {
        migrate_legacy_evidence_packets(conn)?;
        conn.execute_batch(MIGRATION_051_UP)?;
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(AppError::msg(format!(
                "migration '{}' integrity check failed: {integrity}",
                NAME
            )));
        }
        let foreign_key_issues: i64 =
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if foreign_key_issues != 0 {
            return Err(AppError::msg(format!(
                "migration '{}' foreign key check failed: {foreign_key_issues} issues",
                NAME
            )));
        }
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES (?1, datetime('now'))",
            [NAME],
        )?;
        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            if foreign_keys_enabled != 0 {
                conn.execute_batch("PRAGMA foreign_keys = ON")?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            if foreign_keys_enabled != 0 {
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON");
            }
            Err(error)
        }
    }
}

/// Restore a legacy-readable schema without reactivating cancelled Runs.
fn rollback_agent_harness_cutover(conn: &Connection) -> AppResult<()> {
    const NAME: &str = "051_agent_harness_cutover";
    if !is_applied(conn, NAME) {
        return Ok(());
    }

    let foreign_keys_enabled: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys = OFF; BEGIN IMMEDIATE")?;
    let result = (|| -> AppResult<()> {
        conn.execute_batch(MIGRATION_051_DOWN)?;
        let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(AppError::msg(format!(
                "rollback '{}' integrity check failed: {integrity}",
                NAME
            )));
        }
        let foreign_key_issues: i64 =
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if foreign_key_issues != 0 {
            return Err(AppError::msg(format!(
                "rollback '{}' foreign key check failed: {foreign_key_issues} issues",
                NAME
            )));
        }
        conn.execute("DELETE FROM _migrations WHERE name = ?1", [NAME])?;
        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            if foreign_keys_enabled != 0 {
                conn.execute_batch("PRAGMA foreign_keys = ON")?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            if foreign_keys_enabled != 0 {
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON");
            }
            Err(error)
        }
    }
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
    apply_migration(
        conn,
        "049_document_capability_policies",
        MIGRATION_049_UP,
        false,
    )?;
    apply_migration(
        conn,
        "050_agent_run_explicit_action",
        MIGRATION_050_UP,
        false,
    )?;
    apply_agent_harness_cutover(conn)?;
    apply_migration(
        conn,
        "052_web_evidence_provider_runtime",
        MIGRATION_052_UP,
        false,
    )?;
    apply_migration(
        conn,
        "053_remove_model_slot_capabilities",
        MIGRATION_053_UP,
        false,
    )?;
    apply_migration(conn, "054_embedding_scheduler", MIGRATION_054_UP, false)?;
    apply_migration(
        conn,
        "055_session_message_turn_context",
        MIGRATION_055_UP,
        false,
    )?;

    Ok(())
}

fn rollback_migration(conn: &Connection, name: &str, sql: &str) {
    let _ = conn.execute_batch(sql);
    let _ = conn.execute("DELETE FROM _migrations WHERE name = ?1", [name]);
}

/// Roll back all migrations in strict reverse order (for tests).
pub fn migrate_down(conn: &Connection) -> AppResult<()> {
    rollback_migration(conn, "055_session_message_turn_context", MIGRATION_055_DOWN);
    rollback_migration(conn, "054_embedding_scheduler", MIGRATION_054_DOWN);
    rollback_migration(
        conn,
        "053_remove_model_slot_capabilities",
        MIGRATION_053_DOWN,
    );
    rollback_migration(
        conn,
        "052_web_evidence_provider_runtime",
        MIGRATION_052_DOWN,
    );
    rollback_agent_harness_cutover(conn)?;
    rollback_migration(conn, "050_agent_run_explicit_action", MIGRATION_050_DOWN);
    rollback_migration(conn, "049_document_capability_policies", MIGRATION_049_DOWN);
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
    fn migration_054_marks_abandoned_zero_progress_generation_legacy_ready() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let phase: String = conn
            .query_row(
                "SELECT phase FROM embedding_generation_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let automatic_attempted: i64 = conn
            .query_row(
                "SELECT automatic_attempted FROM embedding_generation_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(phase, "legacy_ready");
        assert_eq!(automatic_attempted, 0);
    }

    #[test]
    fn migration_055_adds_immutable_turn_scope_and_display_mentions_with_empty_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let columns = conn
            .prepare("PRAGMA table_info(session_messages)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert!(columns.contains(&"context_scope_json".to_string()));
        assert!(columns.contains(&"display_mentions_json".to_string()));

        conn.execute(
            "INSERT INTO sessions (session_key, created_at, updated_at)
             VALUES ('migration-055-session', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, created_at)
             VALUES (?1, 1, 'user', 'legacy message', datetime('now'))",
            [session_id],
        )
        .unwrap();
        let stored: (String, String) = conn
            .query_row(
                "SELECT context_scope_json, display_mentions_json
                 FROM session_messages WHERE session_id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, ("[]".to_string(), "[]".to_string()));
    }

    #[test]
    fn migration_055_down_rebuilds_messages_without_losing_existing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        conn.execute(
            "INSERT INTO sessions (session_key, created_at, updated_at)
             VALUES ('migration-055-down', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, context_scope_json,
              display_mentions_json, created_at)
             VALUES (?1, 1, 'user', 'preserved', '{}', '[]', datetime('now'))",
            [session_id],
        )
        .unwrap();

        rollback_migration(
            &conn,
            "055_session_message_turn_context",
            MIGRATION_055_DOWN,
        );

        let content: String = conn
            .query_row(
                "SELECT content FROM session_messages WHERE session_id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(content, "preserved");
        let columns = conn
            .prepare("PRAGMA table_info(session_messages)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert!(!columns.contains(&"context_scope_json".to_string()));
        assert!(!columns.contains(&"display_mentions_json".to_string()));
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
              last_refreshed_at)
             VALUES ('custom', 'model-a', 'Model A', 'provider_discovered', 0,
                     datetime('now'), datetime('now'), datetime('now'))",
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
            "web_evidence_provider_runtime",
            "web_evidence_provider_health",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap();
            assert!(exists, "missing {table}");
        }

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
    fn migration_049_creates_document_capability_policies_with_rollback() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        let columns = conn
            .prepare("PRAGMA table_info(document_capability_policies)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            columns,
            vec![
                "id",
                "scope_kind",
                "scope_path",
                "capability",
                "decision",
                "created_at",
                "updated_at",
            ]
        );
        let denied = conn
            .execute(
                "INSERT INTO document_capability_policies
                 (scope_kind, scope_path, capability, decision)
                 VALUES ('document', 'notes/restricted.md', 'send_to_model', 'deny')",
                [],
            )
            .unwrap();
        assert_eq!(denied, 1);
        let invalid = conn.execute(
            "INSERT INTO document_capability_policies
             (scope_kind, scope_path, capability, decision)
             VALUES ('invalid', 'notes/a.md', 'read', 'allow')",
            [],
        );
        assert!(invalid.is_err());

        migrate_down(&conn).unwrap();
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'document_capability_policies'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 0);
    }
    #[test]
    fn migration_050_persists_explicit_actions_with_rollback() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(agent_runs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert!(columns.contains(&"explicit_action_json".to_string()));

        migrate_down(&conn).unwrap();
        let columns = conn
            .prepare("PRAGMA table_info(agent_runs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert!(!columns.contains(&"explicit_action_json".to_string()));
    }
    #[test]
    fn migration_051_cutover_migrates_legacy_fixture_and_roundtrips_safely() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        rollback_agent_harness_cutover(&conn).unwrap();

        conn.execute(
            "INSERT INTO sessions
             (session_key, scene, note_path, title, retention_policy, created_at, updated_at)
             VALUES ('legacy:note', 'drafting_assist', 'notes/legacy.md', 'Legacy', 'user_clearable',
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, evidence_packets, created_at)
             VALUES (?1, 1, 'assistant', 'legacy answer', ?2, datetime('now'))",
            rusqlite::params![
                session_id,
                r#"[{
                    "id":"packet-1","source_type":"note","source_path":"notes/legacy.md",
                    "title":"legacy source","heading_path":"section",
                    "source_span":{"start":0,"end":2},"content_hash":"legacy-hash",
                    "retrieval_reason":"legacy","score":0.8,"trust_level":"user_note",
                    "citation_label":"[L1]","stale":false
                }]"#,
            ],
        )
        .unwrap();

        for (request_id, status) in [
            ("legacy-request-complete", "completed"),
            ("legacy-request-running", "running"),
        ] {
            conn.execute(
                "INSERT INTO ai_traces
                 (request_id, scene, token_input, token_output, status, created_at)
                 VALUES (?1, 'drafting_assist', 12, 34, ?2, datetime('now'))",
                rusqlite::params![request_id, status],
            )
            .unwrap();
        }
        for (task_id, request_id, status) in [
            (
                "legacy-task-complete",
                "legacy-request-complete",
                "completed",
            ),
            ("legacy-task-running", "legacy-request-running", "running"),
        ] {
            conn.execute(
                "INSERT INTO agent_tasks
                 (task_id, request_id, session_id, kind, status, user_goal_summary,
                  budget_policy_json, created_at, updated_at, completed_at)
                 VALUES (?1, ?2, ?3, 'legacy', ?4, 'goal summary', '{}',
                         datetime('now'), datetime('now'), datetime('now'))",
                rusqlite::params![task_id, request_id, session_id, status],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO agent_task_steps
             (task_id, step_seq, kind, status, input_summary, output_summary, checkpoint_json,
              created_at, updated_at)
             VALUES ('legacy-task-complete', 1, 'answer', 'completed', 'input', 'output',
                     '{\"unsafe\":\"checkpoint\"}', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tool_audit
             (request_id, harness_round, tool_name, arguments_summary, success)
             VALUES ('legacy-request-complete', 1, 'read_note', 'path=notes/legacy.md', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_permission_audit
             (request_id, tool_name, permission_name, decision, scope_summary, risk_level, result_status)
             VALUES ('legacy-request-complete', 'read_note', 'vault.read', 'allow',
                     'path=notes/legacy.md', 'low', 'executed')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO writing_states
             (request_id, target_path, draft_version_hash, document_goal, created_at, updated_at)
             VALUES ('legacy-request-complete', 'notes/legacy.md', 'hash', 'history',
                     datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO research_states
             (request_id, research_question, created_at, updated_at)
             VALUES ('legacy-request-complete', 'history', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO deliberation_states
             (request_id, session_id, current_goal, plan_outline_json, assumptions_json,
              open_questions_json, evidence_gaps_json, verification_json, status, created_at, updated_at)
             VALUES ('legacy-request-complete', ?1, 'history', '[]', '[]', '[]', '[]',
                     '{}', 'completed', datetime('now'), datetime('now'))",
            [session_id],
        )
        .unwrap();

        apply_agent_harness_cutover(&conn).unwrap();

        let session_columns = conn
            .prepare("PRAGMA table_info(sessions)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .flatten()
            .collect::<Vec<_>>();
        assert!(!session_columns.contains(&"scene".to_string()));
        assert!(!session_columns.contains(&"note_path".to_string()));
        for table in [
            "ai_traces",
            "agent_tasks",
            "agent_task_steps",
            "agent_task_events",
            "deliberation_states",
            "writing_states",
            "research_states",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 0, "{table} must be removed by the cutover");
        }

        let (completed_status, token_input, token_output): (String, i64, i64) = conn
            .query_row(
                "SELECT status, token_input, token_output FROM agent_runs
                 WHERE run_id = 'legacy-task:legacy-task-complete'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(completed_status, "completed");
        assert_eq!((token_input, token_output), (12, 34));

        let (running_status, cancellation_code): (String, String) = conn
            .query_row(
                "SELECT status, error_code FROM agent_runs
                 WHERE run_id = 'legacy-task:legacy-task-running'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(running_status, "cancelled");
        assert_eq!(cancellation_code, "cancelled_legacy");

        let (turn_id, evidence_refs): (String, String) = conn
            .query_row(
                "SELECT turn_id, evidence_refs_json FROM session_messages WHERE session_id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(turn_id, "legacy-message:1");
        assert!(evidence_refs.contains("evidenceId"));
        assert!(!evidence_refs.contains("excerpt"));

        let tool_audit_target: String = conn
            .query_row("PRAGMA foreign_key_list(tool_audit)", [], |row| row.get(2))
            .unwrap();
        assert_eq!(tool_audit_target, "agent_runs");
        let permission_audit_target: String = conn
            .query_row(
                "PRAGMA foreign_key_list(agent_permission_audit)",
                [],
                |row| row.get(2),
            )
            .unwrap();
        assert_eq!(permission_audit_target, "agent_runs");
        assert_eq!(
            conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );

        rollback_agent_harness_cutover(&conn).unwrap();
        let (scene, note_path, message_count): (String, Option<String>, i64) = conn
            .query_row(
                "SELECT scene, note_path,
                        (SELECT COUNT(*) FROM session_messages WHERE session_id = sessions.id)
                 FROM sessions WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(scene, "legacy");
        assert_eq!(note_path, None);
        assert_eq!(message_count, 1);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM agent_tasks WHERE status = 'completed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );

        apply_agent_harness_cutover(&conn).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
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
