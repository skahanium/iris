use iris_lib::cas::encryption::{self, CasKeyRing};
use iris_lib::cas::store::{
    CasObjectStore, CommitMetadata, CommitObject, ObjectType, TreeEntry, TreeObject,
};
use iris_lib::error::AppError;
use tempfile::tempdir;

fn encrypted_store() -> (tempfile::TempDir, CasObjectStore) {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
    store.enable_encryption([0xA5; 32]);
    (dir, store)
}

#[test]
fn test_store_and_retrieve_blob() {
    let (_dir, store) = encrypted_store();

    let content = "Hello, World!";
    let hash = store.store_blob(content.as_bytes()).unwrap();

    let retrieved = store.read_blob(&hash).unwrap();
    assert_eq!(retrieved, content.as_bytes());
}

#[test]
fn test_store_and_retrieve_blob_as_string() {
    let (_dir, store) = encrypted_store();

    let content = "Hello, World!";
    let hash = store.store_blob(content.as_bytes()).unwrap();

    let retrieved = store.read_blob_content(&hash).unwrap();
    assert_eq!(retrieved, content);
}

#[test]
fn test_store_and_retrieve_tree() {
    let (_dir, store) = encrypted_store();

    let tree = TreeObject {
        hash: String::new(),
        entries: vec![TreeEntry {
            name: "test.md".to_string(),
            object_hash: "abc123".to_string(),
            object_type: ObjectType::Blob,
            mode: "100644".to_string(),
        }],
        ref_count: 1,
        created_at: chrono::Utc::now(),
    };

    let hash = store.store_tree(&tree).unwrap();
    let retrieved = store.read_tree(&hash).unwrap();

    assert_eq!(retrieved.entries.len(), 1);
    assert_eq!(retrieved.entries[0].name, "test.md");
    assert_eq!(retrieved.entries[0].object_type, ObjectType::Blob);
}

#[test]
fn test_store_and_retrieve_commit() {
    let (_dir, store) = encrypted_store();

    let commit = CommitObject {
        hash: String::new(),
        tree_hash: "tree_hash".to_string(),
        parent_hash: None,
        author: "Iris".to_string(),
        message: "Test commit".to_string(),
        metadata: CommitMetadata {
            file_id: 1,
            version_no: "20260101000000000".to_string(),
            label: None,
            kind: "manual".to_string(),
            word_count: 10,
            is_finalized: false,
        },
        created_at: chrono::Utc::now(),
    };

    let hash = store.store_commit(&commit).unwrap();
    let retrieved = store.read_commit(&hash).unwrap();

    assert_eq!(retrieved.message, "Test commit");
    assert_eq!(retrieved.metadata.file_id, 1);
    assert_eq!(retrieved.metadata.kind, "manual");
}

#[test]
fn test_content_deduplication() {
    let (_dir, store) = encrypted_store();

    let content = "Same content";
    let hash1 = store.store_blob(content.as_bytes()).unwrap();
    let hash2 = store.store_blob(content.as_bytes()).unwrap();

    assert_eq!(hash1, hash2);
}

#[test]
fn test_update_and_read_ref() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let hash = "abc123";
    store.update_ref("versions/1", hash).unwrap();

    let retrieved = store.read_ref("versions/1").unwrap();
    assert_eq!(retrieved, Some(hash.to_string()));
}

#[test]
fn test_read_nonexistent_ref() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let retrieved = store.read_ref("nonexistent").unwrap();
    assert_eq!(retrieved, None);
}

#[test]
fn test_read_nonexistent_blob() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let result = store.read_blob("nonexistent_hash");
    assert!(result.is_err());
}

#[test]
fn test_write_content() {
    let (_dir, store) = encrypted_store();

    let content = "Test content";
    let hash = store.write_content(content).unwrap();

    let retrieved = store.read_blob_content(&hash).unwrap();
    assert_eq!(retrieved, content);
}

#[test]
fn test_object_path_format() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    let path = store.object_path(hash).unwrap();

    assert!(path.ends_with("ab/cdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"));
}

#[test]
fn test_object_path_rejects_short_hash() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
    assert!(store.object_path("").is_err());
    assert!(store.object_path("a").is_err());
    assert!(store.object_path("ab").is_ok());
}

#[test]
fn test_store_blob_requires_encryption_key() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let result = store.store_blob(b"must not be plaintext");

    assert!(result.is_err());
}

#[test]
fn test_v2_blob_roundtrip_uses_version_header() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
    let ring = CasKeyRing::from_keys(vec![[0xAA; 32], [0xBB; 32]]).unwrap();
    store.enable_encryption_ring(ring);

    let content = b"versioned snapshot body";
    let hash = store.store_blob(content).unwrap();

    let raw = std::fs::read(store.object_path(&hash).unwrap()).unwrap();
    assert_eq!(
        &raw[0..4],
        b"CAS2",
        "new blobs must use the versioned header"
    );
    assert_eq!(raw[4], 1, "header must record the current ring version");

    let retrieved = store.read_blob(&hash).unwrap();
    assert_eq!(retrieved, content);
}

#[test]
fn test_legacy_case_blob_readable_with_ring() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
    let key = [0xAA; 32];
    store.enable_encryption_ring(CasKeyRing::from_keys(vec![key]).unwrap());

    let content = b"legacy snapshot body";
    let hash = iris_lib::cas::hash::content_hash(content);
    let path = store.object_path(&hash).unwrap();
    let mut buf = b"CASE".to_vec();
    buf.extend_from_slice(&encryption::encrypt_blob(content, &key).unwrap());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &buf).unwrap();

    let retrieved = store.read_blob(&hash).unwrap();
    assert_eq!(
        retrieved, content,
        "legacy CASE blobs must read as version 0"
    );
}

#[test]
fn test_blob_with_unknown_key_version_is_unreadable() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
    store.enable_encryption_ring(CasKeyRing::from_keys(vec![[0xAA; 32]]).unwrap());

    let content = b"blob encrypted under a retired key";
    let hash = iris_lib::cas::hash::content_hash(content);
    let path = store.object_path(&hash).unwrap();
    let mut buf = b"CAS2".to_vec();
    buf.push(9);
    buf.extend_from_slice(&encryption::encrypt_blob(content, &[0xBB; 32]).unwrap());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &buf).unwrap();

    let err = store.read_blob(&hash).unwrap_err();
    assert!(
        matches!(err, AppError::CasUnreadable(_)),
        "unknown key version must surface as unreadable, got: {err:?}"
    );
}
