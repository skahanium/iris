use chrono::Utc;

use crate::error::AppResult;
use crate::storage::db::Database;

/// 引用计数管理器
pub struct RefCounter {
    db: Database,
}

impl RefCounter {
    /// 创建新的引用计数管理器
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 增加引用计数
    pub fn increment(&self, object_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO cas_refs (object_hash, ref_count, object_type, size, created_at, last_accessed_at)
                 VALUES (?1, 1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(object_hash) DO UPDATE SET
                     ref_count = ref_count + 1,
                     last_accessed_at = excluded.last_accessed_at",
                rusqlite::params![
                    object_hash,
                    "unknown",
                    0,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    /// 减少引用计数
    pub fn decrement(&self, object_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE cas_refs SET
                     ref_count = MAX(0, ref_count - 1),
                     last_accessed_at = ?1
                 WHERE object_hash = ?2",
                rusqlite::params![Utc::now().to_rfc3339(), object_hash],
            )?;
            Ok(())
        })
    }

    /// 获取引用计数
    pub fn get_count(&self, object_hash: &str) -> AppResult<u32> {
        self.db.with_conn(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT ref_count FROM cas_refs WHERE object_hash = ?1",
                    [object_hash],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok(count as u32)
        })
    }

    /// 添加引用关系
    pub fn add_ref_link(&self, source_hash: &str, target_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO cas_ref_links (source_hash, target_hash)
                 VALUES (?1, ?2)",
                rusqlite::params![source_hash, target_hash],
            )?;
            Ok(())
        })
    }

    /// 删除引用关系
    pub fn remove_ref_link(&self, source_hash: &str, target_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM cas_ref_links WHERE source_hash = ?1 AND target_hash = ?2",
                rusqlite::params![source_hash, target_hash],
            )?;
            Ok(())
        })
    }

    /// 查找引用计数为 0 的对象
    pub fn find_orphaned_objects(&self) -> AppResult<Vec<String>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT object_hash FROM cas_refs WHERE ref_count = 0")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.flatten().collect())
        })
    }
}
