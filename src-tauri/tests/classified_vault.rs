//! Classified vault end-to-end integration tests (setup → unlock → encrypt → decrypt → lock).

use iris_lib::app::AppState;
use iris_lib::crypto::classified_io;
use iris_lib::crypto::vault_key::VaultKey;
use iris_lib::indexer::scan::scan_vault;
use iris_lib::storage::paths::{is_accessible_note_path, is_user_note_path};
use std::fs;
use tempfile::tempdir;

#[test]
fn full_classified_workflow_setup_unlock_write_read_lock() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(vault.join(".iris")).unwrap();

    VaultKey::setup("my-password", &vault).unwrap();
    assert!(vault.join(".iris/vault.json").exists());
    assert!(vault.join(".classified").exists());

    let mut vk = VaultKey::new();
    vk.unlock("my-password", &vault).unwrap();
    assert!(vk.is_unlocked());

    let key = vk.key().unwrap();
    let plaintext = "# Secret Note\n\nThis is classified content.";
    let encrypted = classified_io::encrypt_cef(plaintext.as_bytes(), key).unwrap();
    fs::write(vault.join(".classified/secret.md"), &encrypted).unwrap();

    let raw = fs::read(vault.join(".classified/secret.md")).unwrap();
    assert!(classified_io::has_csef_magic(&raw));
    let decrypted = classified_io::decrypt_cef(&raw, key).unwrap();
    assert_eq!(String::from_utf8_lossy(&decrypted), plaintext);

    vk.lock();
    assert!(!vk.is_unlocked());
    assert!(vk.key().is_err());

    let wrong_key = [0u8; 32];
    assert!(classified_io::decrypt_cef(&raw, &wrong_key).is_err());
}

#[test]
fn classified_files_excluded_from_user_note_path() {
    assert!(!is_user_note_path(".classified/secret.md"));
    assert!(!is_user_note_path(".classified"));
    assert!(is_user_note_path("notes/normal.md"));
}

#[test]
fn accessible_note_path_includes_classified_files() {
    assert!(is_accessible_note_path(".classified/secret.md"));
    assert!(!is_accessible_note_path(".classified"));
    assert!(is_accessible_note_path("notes/normal.md"));
    assert!(!is_accessible_note_path(".iris/meta.json"));
}

#[test]
fn encrypt_decrypt_roundtrip_large_file() {
    let mut plain = String::with_capacity(100_000);
    for i in 0..10_000 {
        plain.push_str(&format!("Line {i}: some classified content here.\n"));
    }
    let key = {
        let mut k = [0u8; 32];
        k[0] = 42;
        k
    };
    let enc = classified_io::encrypt_cef(plain.as_bytes(), &key).unwrap();
    let dec = classified_io::decrypt_cef(&enc, &key).unwrap();
    assert_eq!(String::from_utf8_lossy(&dec), plain);
}

#[test]
fn vault_scan_skips_classified_directory_entries() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();

    VaultKey::setup("scan-pass", &vault).unwrap();
    let mut vk = VaultKey::new();
    vk.unlock("scan-pass", &vault).unwrap();
    let key = vk.key().unwrap();

    let encrypted = classified_io::encrypt_cef(b"# Classified\n\nBody.", key).unwrap();
    fs::create_dir_all(vault.join("notes")).unwrap();
    fs::write(vault.join(".classified/hidden.md"), &encrypted).unwrap();
    fs::write(vault.join("notes/visible.md"), "# Visible\n\nBody.").unwrap();

    let state = AppState::new(dir.path().join("data")).unwrap();
    state.set_vault(vault.clone()).unwrap();

    let entries = state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();

    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"notes/visible.md"));
    assert!(!paths.iter().any(|p| p.starts_with(".classified/")));
}
