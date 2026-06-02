use std::fs;
use std::sync::Arc;

use chrono::Utc;

use crate::cas::store::CasObjectStore;
use crate::error::AppResult;
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
            self.delete_object(object_hash)?;
            result.deleted_count += 1;
        }

        // 3. 清理过期回收站条目
        let expired_items = self.find_expired_recycle_items()?;
        for item in expired_items {
            self.purge_recycle_item(&item)?;
            result.recycle_purged_count += 1;
        }

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

    /// 删除对象：物理文件 + 数据库引用记录
    fn delete_object(&self, object_hash: &str) -> AppResult<()> {
        // 1. 删除物理文件
        let object_path = self.store.object_path(object_hash)?;
        if object_path.exists() {
            fs::remove_file(&object_path)?;
        }

        // 2. 删除引用记录
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM cas_refs WHERE object_hash = ?1", [object_hash])?;
            conn.execute(
                "DELETE FROM cas_ref_links WHERE source_hash = ?1 OR target_hash = ?1",
                [object_hash],
            )?;
            Ok(())
        })?;

        Ok(())
    }

    /// 查找过期回收站条目
    fn find_expired_recycle_items(&self) -> AppResult<Vec<(String, String)>> {
        self.db.with_read_conn(|conn| {
            let now = Utc::now().to_rfc3339();
            let mut stmt =
                conn.prepare("SELECT id, trash_rel_dir FROM recycle_bin WHERE expires_at <= ?1")?;
            let rows = stmt.query_map([&now], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(rows.flatten().collect())
        })
    }

    /// 清除回收站条目：删除目录 + 数据库记录
    fn purge_recycle_item(&self, (id, trash_rel_dir): &(String, String)) -> AppResult<()> {
        // 1. 删除回收站目录
        let trash_dir = self.store.base_path().join(trash_rel_dir);
        if trash_dir.exists() {
            fs::remove_dir_all(&trash_dir)?;
        }

        // 2. 删除数据库记录
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM recycle_bin WHERE id = ?1", [id])?;
            Ok(())
        })?;

        Ok(())
    }
}
