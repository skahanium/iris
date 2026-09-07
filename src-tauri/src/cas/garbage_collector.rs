use std::fs;
use std::sync::Arc;

use crate::cas::store::CasObjectStore;
use crate::error::AppResult;
use crate::recycle::purge_expired_items;
use crate::storage::db::Database;

/// 垃圾回收结果
#[derive(Debug, Default)]
pub struct GarbageCollectionResult {
    pub orphaned_count: usize,
    pub deleted_count: usize,
    pub recycle_purged_count: usize,
    pub space_freed: u64,
}

/// CAS 垃圾回收器
pub struct GarbageCollector {
    store: CasObjectStore,
    db: Arc<Database>,
}

impl GarbageCollector {
    /// 创建新的垃圾回收器
    pub fn new(store: CasObjectStore, db: Arc<Database>) -> Self {
        Self { store, db }
    }

    /// 执行垃圾回收
    pub async fn collect(&self) -> AppResult<GarbageCollectionResult> {
        crate::storage::atomic_write::with_vault_move_lock(|| self.collect_locked())
    }

    fn collect_locked(&self) -> AppResult<GarbageCollectionResult> {
        let mut result = GarbageCollectionResult::default();

        // 1. 查找引用计数为 0 的对象
        let orphaned_objects = self.find_orphaned_objects()?;
        result.orphaned_count = orphaned_objects.len();

        // 2. 删除孤立对象
        for object_hash in &orphaned_objects {
            result.space_freed += self.delete_object(object_hash)?;
            result.deleted_count += 1;
        }

        // 3. 清理过期回收站条目（复用 recycle 模块逻辑）
        let vault = self.recycle_vault_path();
        let (purged, freed) = purge_expired_items(&self.db, &vault)?;
        result.recycle_purged_count = purged;
        result.space_freed += freed;

        Ok(result)
    }

    /// 查找孤立对象（引用计数为 0）
    fn find_orphaned_objects(&self) -> AppResult<Vec<String>> {
        self.db.with_read_conn(|conn| {
            let version_objects = crate::version::repository::referenced_object_hashes(conn)?;
            let mut stmt = conn.prepare("SELECT object_hash FROM cas_refs WHERE ref_count = 0")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows
                .flatten()
                .filter(|hash| !version_objects.contains(hash))
                .collect())
        })
    }

    fn recycle_vault_path(&self) -> std::path::PathBuf {
        let base = self.store.base_path();
        if base.file_name().is_some_and(|name| name == "cas")
            && base
                .parent()
                .and_then(std::path::Path::file_name)
                .is_some_and(|name| name == ".iris")
        {
            return base
                .parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(base)
                .to_path_buf();
        }
        base.to_path_buf()
    }

    /// 删除对象：数据库引用记录 + 物理文件
    /// 返回释放的字节数
    fn delete_object(&self, object_hash: &str) -> AppResult<u64> {
        // Keep database ownership until physical removal succeeds. A failed
        // filesystem operation must remain retryable instead of losing the
        // only durable record of the object.
        let object_path = self.store.object_path(object_hash)?;
        let freed = if object_path.exists() {
            let size = fs::metadata(&object_path).map(|m| m.len()).unwrap_or(0);
            fs::remove_file(&object_path)?;
            // 清理空的父目录（objects/ab/）
            if let Some(parent) = object_path.parent() {
                if parent
                    .read_dir()
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false)
                {
                    let _ = fs::remove_dir(parent);
                }
            }
            size
        } else {
            0
        };

        self.db.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let result = (|| {
                conn.execute("DELETE FROM cas_refs WHERE object_hash = ?1", [object_hash])?;
                conn.execute(
                    "DELETE FROM cas_ref_links WHERE source_hash = ?1 OR target_hash = ?1",
                    [object_hash],
                )?;
                Ok::<_, crate::error::AppError>(())
            })();
            match result {
                Ok(()) => conn.execute_batch("COMMIT").map_err(Into::into),
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(error)
                }
            }
        })?;

        Ok(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gc_preserves_durable_version_objects_with_stale_zero_refcount() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let state = crate::app::AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        let entry = crate::version::version_save_manual(&state, "note.md", "historical")
            .unwrap()
            .unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute("UPDATE cas_refs SET ref_count = 0", [])?;
                conn.execute("DELETE FROM files", [])?;
                Ok(())
            })
            .unwrap();
        let gc = GarbageCollector::new(
            state.cas_store().unwrap().as_ref().clone(),
            state.db.clone(),
        );
        gc.collect().await.unwrap();
        assert_eq!(
            crate::version::version_preview(&state, entry.id).unwrap(),
            "historical"
        );
    }

    #[tokio::test]
    async fn gc_purges_recycle_bundle_from_the_vault_not_the_cas_directory() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let bundle = vault.join(".iris/trash/expired");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("manifest.json"), "{}").unwrap();
        let state = crate::app::AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault.clone()).unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO recycle_bin
                 (id, original_path, title, deleted_at, expires_at, trash_rel_dir)
                 VALUES ('expired', 'note.md', 'Note', '2020-01-01T00:00:00Z',
                         '2020-01-01T00:00:00Z', '.iris/trash/expired')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        let gc = GarbageCollector::new(
            state.cas_store().unwrap().as_ref().clone(),
            state.db.clone(),
        );
        let result = gc.collect().await.unwrap();

        assert_eq!(result.recycle_purged_count, 1);
        assert!(!bundle.exists());
    }

    #[tokio::test]
    async fn gc_keeps_database_ownership_when_physical_removal_fails() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();
        let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let object_path = store.object_path(hash).unwrap();
        fs::create_dir_all(&object_path).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO cas_refs (object_hash, ref_count, created_at, last_accessed_at)
                 VALUES (?1, 0, datetime('now'), datetime('now'))",
                [hash],
            )?;
            Ok(())
        })
        .unwrap();
        let gc = GarbageCollector::new(store, db.clone());

        assert!(gc.collect().await.is_err());
        let count = db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM cas_refs WHERE object_hash = ?1",
                    [hash],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
