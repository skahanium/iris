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
        let vault = self.store.base_path().to_path_buf();
        let (purged, freed) = purge_expired_items(&self.db, &vault)?;
        result.recycle_purged_count = purged;
        result.space_freed += freed;

        Ok(result)
    }

    /// 查找孤立对象（引用计数为 0）
    fn find_orphaned_objects(&self) -> AppResult<Vec<String>> {
        self.db.with_read_conn(|conn| {
            let mut stmt = conn.prepare("SELECT object_hash FROM cas_refs WHERE ref_count = 0")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.flatten().collect())
        })
    }

    /// 删除对象：数据库引用记录 + 物理文件
    /// 返回释放的字节数
    fn delete_object(&self, object_hash: &str) -> AppResult<u64> {
        // 1. 先删除数据库记录（保证原子性：DB 可回滚，文件不可恢复）
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM cas_refs WHERE object_hash = ?1", [object_hash])?;
            conn.execute(
                "DELETE FROM cas_ref_links WHERE source_hash = ?1 OR target_hash = ?1",
                [object_hash],
            )?;
            Ok(())
        })?;

        // 2. 删除物理文件，记录释放的空间
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

        Ok(freed)
    }
}
