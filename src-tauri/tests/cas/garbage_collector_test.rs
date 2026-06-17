use std::sync::Arc;

use chrono::Utc;
use iris_lib::cas::garbage_collector::GarbageCollector;
use iris_lib::cas::ref_counter::RefCounter;
use iris_lib::cas::store::CasObjectStore;
use iris_lib::storage::db::Database;
use tempfile::tempdir;

fn setup() -> (CasObjectStore, Arc<Database>, GarbageCollector) {
    let dir = tempdir().unwrap();
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
    store.enable_encryption([3u8; 32]);
    let gc = GarbageCollector::new(store.clone(), db.clone());
    (store, db, gc)
}

#[tokio::test]
async fn test_gc_removes_orphaned_objects() {
    let (store, db, gc) = setup();
    let rc = RefCounter::new(db.clone());

    // 存储对象并设置引用计数为 0（通过增减）
    let hash = store.store_blob(b"orphan content").unwrap();
    rc.increment(&hash).unwrap();
    rc.decrement(&hash).unwrap();
    assert_eq!(rc.get_count(&hash).unwrap(), 0);

    let result = gc.collect().await.unwrap();
    assert_eq!(result.orphaned_count, 1);
    assert_eq!(result.deleted_count, 1);

    // 对象文件应被删除
    let path = store.object_path(&hash).unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn test_gc_preserves_referenced_objects() {
    let (store, db, gc) = setup();
    let rc = RefCounter::new(db.clone());

    let hash = store.store_blob(b"referenced content").unwrap();
    rc.increment(&hash).unwrap();

    let result = gc.collect().await.unwrap();
    assert_eq!(result.orphaned_count, 0);
    assert_eq!(result.deleted_count, 0);

    // 对象文件应保留
    let path = store.object_path(&hash).unwrap();
    assert!(path.exists());
}

#[tokio::test]
async fn test_gc_removes_ref_links_for_orphaned_objects() {
    let (store, db, gc) = setup();
    let rc = RefCounter::new(db.clone());

    let hash1 = store.store_blob(b"obj1").unwrap();
    let hash2 = store.store_blob(b"obj2").unwrap();

    rc.increment(&hash1).unwrap();
    rc.increment(&hash2).unwrap();
    rc.add_ref_link(&hash1, &hash2).unwrap();

    // 使 hash1 成为孤立对象
    rc.decrement(&hash1).unwrap();

    let result = gc.collect().await.unwrap();
    assert_eq!(result.deleted_count, 1);

    // cas_ref_links 中 hash1 相关记录应被清除
    db.with_read_conn(|conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cas_ref_links WHERE source_hash = ?1 OR target_hash = ?1",
                [&hash1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        Ok(())
    })
    .unwrap();
}

#[tokio::test]
async fn test_gc_cleans_expired_recycle_bin_items() {
    let (store, db, gc) = setup();

    // 在回收站中插入一个已过期的条目
    let trash_rel = "trash/test_item";
    let trash_dir = store.base_path().join(trash_rel);
    std::fs::create_dir_all(&trash_dir).unwrap();
    std::fs::write(trash_dir.join("file.md"), "deleted content").unwrap();

    let past = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO recycle_bin (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "test-id-1",
                "/notes/deleted.md",
                "deleted",
                &past,
                &past,
                trash_rel,
            ],
        )?;
        Ok(())
    })
    .unwrap();

    let result = gc.collect().await.unwrap();
    assert_eq!(result.recycle_purged_count, 1);

    // 目录应被删除
    assert!(!trash_dir.exists());

    // 数据库记录应被删除
    db.with_read_conn(|conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM recycle_bin WHERE id = ?1",
                ["test-id-1"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        Ok(())
    })
    .unwrap();
}

#[tokio::test]
async fn test_gc_preserves_unexpired_recycle_bin_items() {
    let (store, db, gc) = setup();

    let trash_rel = "trash/future_item";
    let trash_dir = store.base_path().join(trash_rel);
    std::fs::create_dir_all(&trash_dir).unwrap();

    let future = (Utc::now() + chrono::Duration::days(30)).to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO recycle_bin (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "test-id-2",
                "/notes/future.md",
                "future",
                &future,
                &future,
                trash_rel,
            ],
        )?;
        Ok(())
    })
    .unwrap();

    let result = gc.collect().await.unwrap();
    assert_eq!(result.recycle_purged_count, 0);

    // 目录应保留
    assert!(trash_dir.exists());
}

#[tokio::test]
async fn test_gc_noop_when_no_orphaned_or_expired() {
    let (_store, _db, gc) = setup();

    let result = gc.collect().await.unwrap();
    assert_eq!(result.orphaned_count, 0);
    assert_eq!(result.deleted_count, 0);
    assert_eq!(result.recycle_purged_count, 0);
    assert_eq!(result.space_freed, 0);
}
