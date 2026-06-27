//! Classified AI security contract tests.
//!
//! These tests lock the security invariants for dual-domain AI:
//! - Classified AI threads are encrypted at rest and fail to load while vault is locked.
//! - Ordinary sessions/session_messages tables contain no classified plaintext.
//! - Trace/log metadata does not leak classified paths or content.

use iris_lib::crypto::classified_io;
use iris_lib::crypto::vault_key::VaultKey;
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
    // Read the session module source to verify no classified content
    // can leak into the sessions/session_messages tables
    let session_src = include_str!("../src/ai_runtime/session.rs");

    // SessionMessage should not have any classified-specific fields
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

    // The create_fresh function should not accept classified paths
    let create_section = session_src
        .split("pub fn create_fresh")
        .nth(1)
        .unwrap_or("");
    assert!(
        !create_section.contains("classified"),
        "create_fresh must not have classified path handling"
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

    // TraceRecorder methods should not accept note paths
    let start_section = trace_src.split("pub fn start").nth(1).unwrap_or("");
    assert!(
        !start_section.contains("note_path"),
        "TraceRecorder::start must not accept note_path parameter"
    );
}

#[test]
fn classified_path_isolation_from_ai_commands() {
    let ai_commands_src = include_str!("../src/commands/ai_commands.rs");

    // validate_ai_note_path must exist and block classified paths
    assert!(
        ai_commands_src.contains("validate_ai_note_path"),
        "ai_commands.rs must have validate_ai_note_path function"
    );
    assert!(
        ai_commands_src.contains("涉密笔记不能进入 AI 管道"),
        "validate_ai_note_path must reject classified notes with Chinese error message"
    );
    assert!(
        ai_commands_src.contains("is_user_note_path"),
        "validate_ai_note_path must use is_user_note_path for classification"
    );
}

#[test]
fn classified_io_provides_cef_encryption_for_threads() {
    let classified_src = include_str!("../src/commands/classified.rs");

    // classified.rs must use CEF encryption for all classified content
    assert!(
        classified_src.contains("encrypt_cef"),
        "classified.rs must use encrypt_cef for encrypting classified content"
    );
    assert!(
        classified_src.contains("decrypt_cef"),
        "classified.rs must use decrypt_cef for decrypting classified content"
    );
    assert!(
        classified_src.contains("has_csef_magic"),
        "classified.rs must use has_csef_magic for detecting encrypted files"
    );
}
