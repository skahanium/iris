use iris_lib::cas::store::{
    CasObjectStore, CommitMetadata, CommitObject, ObjectType, TreeEntry, TreeObject,
};
use tempfile::tempdir;

#[test]
fn test_store_and_retrieve_blob() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let content = "Hello, World!";
    let hash = store.store_blob(content.as_bytes()).unwrap();

    let retrieved = store.read_blob(&hash).unwrap();
    assert_eq!(retrieved, content.as_bytes());
}

#[test]
fn test_store_and_retrieve_blob_as_string() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let content = "Hello, World!";
    let hash = store.store_blob(content.as_bytes()).unwrap();

    let retrieved = store.read_blob_content(&hash).unwrap();
    assert_eq!(retrieved, content);
}

#[test]
fn test_store_and_retrieve_tree() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

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
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

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
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

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
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let content = "Test content";
    let hash = store.write_content("test.md", content).unwrap();

    let retrieved = store.read_blob_content(&hash).unwrap();
    assert_eq!(retrieved, content);
}

#[test]
fn test_object_path_format() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    let path = store.object_path(hash);

    assert!(path.ends_with("ab/cdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"));
}
