use std::sync::Arc;

use iris_lib::cas::ref_counter::RefCounter;
use iris_lib::storage::db::Database;

fn setup() -> RefCounter {
    let db = Arc::new(Database::open_in_memory().unwrap());
    RefCounter::new(db)
}

#[test]
fn test_increment_ref_count() {
    let ref_counter = setup();
    let hash = "abc123";

    ref_counter.increment(hash).unwrap();
    assert_eq!(ref_counter.get_count(hash).unwrap(), 1);

    ref_counter.increment(hash).unwrap();
    assert_eq!(ref_counter.get_count(hash).unwrap(), 2);
}

#[test]
fn test_decrement_ref_count() {
    let ref_counter = setup();
    let hash = "abc123";

    ref_counter.increment(hash).unwrap();
    ref_counter.increment(hash).unwrap();
    ref_counter.decrement(hash).unwrap();

    assert_eq!(ref_counter.get_count(hash).unwrap(), 1);
}

#[test]
fn test_decrement_does_not_go_below_zero() {
    let ref_counter = setup();
    let hash = "abc123";

    ref_counter.decrement(hash).unwrap();
    assert_eq!(ref_counter.get_count(hash).unwrap(), 0);
}

#[test]
fn test_get_count_returns_zero_for_unknown_hash() {
    let ref_counter = setup();
    assert_eq!(ref_counter.get_count("nonexistent").unwrap(), 0);
}

#[test]
fn test_find_orphaned_objects() {
    let ref_counter = setup();

    ref_counter.increment("hash1").unwrap();
    ref_counter.increment("hash2").unwrap();
    ref_counter.decrement("hash1").unwrap();
    ref_counter.decrement("hash1").unwrap();

    let orphaned = ref_counter.find_orphaned_objects().unwrap();
    assert!(orphaned.contains(&"hash1".to_string()));
    assert!(!orphaned.contains(&"hash2".to_string()));
}

#[test]
fn test_find_orphaned_objects_empty_when_none() {
    let ref_counter = setup();

    ref_counter.increment("hash1").unwrap();
    ref_counter.increment("hash2").unwrap();

    let orphaned = ref_counter.find_orphaned_objects().unwrap();
    assert!(orphaned.is_empty());
}

#[test]
fn test_add_and_remove_ref_link() {
    let ref_counter = setup();

    ref_counter.add_ref_link("source1", "target1").unwrap();
    ref_counter.add_ref_link("source1", "target2").unwrap();

    ref_counter.remove_ref_link("source1", "target1").unwrap();

    ref_counter
        .remove_ref_link("source1", "nonexistent")
        .unwrap();
}

#[test]
fn test_add_ref_link_idempotent() {
    let ref_counter = setup();

    ref_counter.add_ref_link("s1", "t1").unwrap();
    ref_counter.add_ref_link("s1", "t1").unwrap();
}

#[test]
fn test_increment_updates_last_accessed() {
    let ref_counter = setup();
    let hash = "abc123";

    ref_counter.increment(hash).unwrap();

    let first_accessed: String = ref_counter
        .get_last_accessed(hash)
        .unwrap()
        .expect("last_accessed_at should exist after increment");

    std::thread::sleep(std::time::Duration::from_millis(10));

    ref_counter.increment(hash).unwrap();

    let second_accessed: String = ref_counter
        .get_last_accessed(hash)
        .unwrap()
        .expect("last_accessed_at should still exist after second increment");

    assert_ne!(
        first_accessed, second_accessed,
        "last_accessed_at should change on increment"
    );
}
