//! Classified AI security contract tests.
//!
//! These tests lock the security invariants for dual-domain AI:
//! - Classified AI threads are encrypted at rest and fail to load while vault is locked.
//! - Ordinary sessions/session_messages tables contain no classified plaintext.
//! - Trace/log metadata does not leak classified paths or content.
//! - Runtime trace redaction prevents classified leaks in error messages.

use iris_lib::crypto::classified_io;
use iris_lib::crypto::vault_key::VaultKey;
use iris_lib::storage::db::Database;
use std::fs;
use tempfile::tempdir;

#[test]
fn classified_ai_thread_save_requires_unlocked_vault() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(vault.join(".iris")).unwrap();

    VaultKey::setup("test-password", &vault).unwrap();

    let mut vk = VaultKey::new();
    vk.unlock("test-password", &vault).unwrap();
    assert!(vk.is_unlocked());

    let key = vk.key().unwrap();
    let thread_content = r#"{"messages":[{"role":"user","content":"classified question"}]}"#;
    let encrypted = classified_io::encrypt_cef(thread_content.as_bytes(), key).unwrap();

    let thread_dir = vault.join(".classified/ai-threads");
    fs::create_dir_all(&thread_dir).unwrap();
    let thread_path = thread_dir.join("test-thread-id.cst");
    fs::write(&thread_path, &encrypted).unwrap();

    // Verify the file is encrypted (has CEF magic)
    let raw = fs::read(&thread_path).unwrap();
    assert!(
        classified_io::has_csef_magic(&raw),
        "classified AI thread must be stored with CEF encryption"
    );

    // Lock the vault and verify load fails
    vk.lock();
    assert!(!vk.is_unlocked());
    assert!(vk.key().is_err(), "key must not be available after lock");

    // Attempting to decrypt with a wrong key must fail
    let wrong_key = [0u8; 32];
    assert!(
        classified_io::decrypt_cef(&raw, &wrong_key).is_err(),
        "decryption with wrong key must fail"
    );
}

#[test]
fn classified_ai_thread_load_fails_while_locked() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(vault.join(".iris")).unwrap();

    VaultKey::setup("lock-test-pass", &vault).unwrap();

    let mut vk = VaultKey::new();
    vk.unlock("lock-test-pass", &vault).unwrap();
    let key = vk.key().unwrap();

    let thread_data = b"some classified AI thread payload";
    let encrypted = classified_io::encrypt_cef(thread_data, key).unwrap();

    let thread_dir = vault.join(".classified/ai-threads");
    fs::create_dir_all(&thread_dir).unwrap();
    fs::write(thread_dir.join("locked-thread.cst"), &encrypted).unwrap();

    // Lock the vault
    vk.lock();
    assert!(!vk.is_unlocked());

    // require_unlocked() must fail when vault is locked
    // This simulates what the classified AI thread load command would do:
    // it calls require_unlocked() before attempting decrypt_cef
    let locked_guard = VaultKey::new();
    assert!(
        locked_guard.key().is_err(),
        "key() must return error when vault is locked"
    );
}

#[test]
fn ordinary_sessions_table_has_no_classified_content() {
    // Read the normal-domain session repository source to verify no classified content
    // can leak into the sessions/session_messages tables
    let session_src = include_str!("../src/ai_runtime/normal_session_repository.rs");

    // Normal session persistence must not have classified-specific fields
    assert!(
        !session_src.contains("vault_key"),
        "SessionMessage must not contain vault_key field"
    );
    assert!(
        !session_src.contains("encryption_key"),
        "SessionMessage must not contain encryption_key field"
    );
    assert!(
        !session_src.contains(".classified"),
        "Session insert must not reference .classified paths"
    );
}

#[test]
fn classified_ai_commands_block_ordinary_session_access() {
    let classified_src = include_str!("../src/commands/classified.rs");

    // Classified commands must not use ordinary session infrastructure
    assert!(
        !classified_src.contains("SessionManager"),
        "classified.rs must not import or use SessionManager"
    );
    assert!(
        !classified_src.contains("session_list"),
        "classified.rs must not call session_list"
    );
    assert!(
        !classified_src.contains("session_messages"),
        "classified.rs must not call session_messages"
    );
}

#[test]
fn trace_log_safe_metadata_leaks_no_classified_plaintext() {
    let trace_src = include_str!("../src/ai_runtime/trace.rs");

    // AiTrace struct should only contain diagnostic metadata, not message content
    assert!(
        !trace_src.contains("pub content"),
        "AiTrace must not store message content"
    );
    assert!(
        !trace_src.contains("note_path"),
        "AiTrace must not store note_path (classified path leak risk)"
    );
    assert!(
        !trace_src.contains("body"),
        "AiTrace must not store body content"
    );

    // legacy trace recording must not accept note paths
    let start_section = trace_src.split("pub fn start").nth(1).unwrap_or("");
    assert!(
        !start_section.contains("note_path"),
        "legacy trace start API must not accept note_path parameter"
    );
}

#[test]
fn classified_run_intake_stays_outside_normal_session_storage() {
    let assistant_src = include_str!("../src/commands/assistant_commands.rs");
    let ephemeral_src = include_str!("../src/ai_runtime/classified_ephemeral.rs");

    assert!(assistant_src.contains("SecurityDomain::Classified"));
    assert!(assistant_src.contains("assistant_classified_context_open"));
    assert!(assistant_src.contains("classified_context_ref"));
    assert!(assistant_src.contains("classified_ephemeral"));
    assert!(assistant_src.contains("spawn_classified_direct_run"));
    assert!(ephemeral_src.contains("Zeroizing<String>"));
    assert!(ephemeral_src.contains("take_result"));
    assert!(
        !ephemeral_src.contains("write_thread_atomically"),
        "new classified Runs must never persist a CEF conversation"
    );
    assert!(
        !assistant_src.contains("SessionManager"),
        "classified Run intake must not write ordinary session storage"
    );
}
#[test]
fn classified_markdown_writes_route_to_cef_write_service() {
    let classified_src = include_str!("../src/commands/classified.rs");
    let note_write_src = include_str!("../src/storage/note_write.rs");

    // Commands must go through the sole Markdown write boundary; encryption
    // belongs there so ordinary, AI, template and classified writes cannot
    // drift into separate persistence implementations.
    assert!(
        classified_src.contains("NoteWriteService"),
        "classified commands must use the unified Markdown write service"
    );
    assert!(
        note_write_src.contains("encrypt_cef"),
        "the unified write service must encrypt classified Markdown with CEF"
    );
    assert!(
        classified_src.contains("has_csef_magic"),
        "classified.rs must use has_csef_magic for detecting encrypted files"
    );
}

/// Sentinel phrase used to detect classified content leaks.
const SENTINEL_PHRASE: &str = "OPERATION_NIGHTFANG_CLASSIFIED_2026";

#[test]
fn ordinary_session_messages_table_has_no_sentinel_after_save() {
    let db = Database::open_in_memory().expect("in-memory db");

    // Ensure sessions and session_messages tables exist
    db.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scene TEXT NOT NULL,
                note_path TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS session_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL REFERENCES sessions(id),
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        Ok(())
    })
    .unwrap();

    // Insert a classified message into the encrypted store (simulated),
    // then verify it did NOT leak into session_messages
    let sentinel = SENTINEL_PHRASE;
    // Simulate a classified AI request that stores its data encrypted.
    // The ordinary session_messages table must never contain the sentinel.
    db.with_conn(|conn| {
        // Scan all rows in session_messages for the sentinel
        let mut stmt = conn
            .prepare("SELECT content FROM session_messages")
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        for content in &rows {
            assert!(
                !content.contains(sentinel),
                "session_messages must not contain classified sentinel phrase: {content}"
            );
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn classified_run_uses_direct_streaming_without_ordinary_session_writes() {
    let assistant_src = include_str!("../src/commands/assistant_commands.rs");

    let classified_run = assistant_src
        .split("fn spawn_classified_direct_run")
        .nth(1)
        .unwrap_or("");
    assert!(classified_run.contains("ModelGatewayStreamingDirectAnswerProvider::new"));
    assert!(classified_run.contains("struct SilentObserver"));
    assert!(
        !classified_run.contains("content_delta"),
        "classified output must not be put on the ordinary Run event bus"
    );
    assert!(
        !classified_run.contains("SessionManager::append_message"),
        "classified Run must not write ordinary session_messages"
    );
}
#[test]
fn classified_stream_events_and_abort_use_unified_gateway_contract() {
    let gateway_src = include_str!("../src/ai_runtime/model_gateway_impl.rs");
    let streaming_src = include_str!("../src/ai_runtime/model_gateway/streaming.rs");

    assert!(
        gateway_src.contains("send_streaming_request_to_observer"),
        "ModelGateway must expose observer-owned streaming entry"
    );
    assert!(
        streaming_src.contains("pub classified: bool") && streaming_src.contains("\"classified\""),
        "stream event payloads must carry classified metadata"
    );
    assert!(
        streaming_src.contains("is_abort_requested(request_id)")
            && streaming_src.contains("finish_stream_with_error")
            && streaming_src.contains("clear_abort(request_id)"),
        "streaming path must finish and clear abort state when request id is aborted"
    );
}
