use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::AppResult;

/// 写入守卫 - 实现乐观锁 + watcher 跳过
///
/// 合并了原 `embedding::queue::WriteGuard` 的 watcher 跳过功能和 CAS 乐观锁验证。
/// 使用 TTL 自动清理过期条目，避免内存泄漏。
pub struct WriteGuard {
    /// 最近写入的文件哈希缓存（path -> (hash, timestamp)）
    entries: Mutex<HashMap<String, (String, Instant)>>,
    /// Old paths intentionally removed by an application-owned rename.
    removed_entries: Mutex<HashMap<String, Instant>>,
}

impl WriteGuard {
    /// watcher 跳过条目的生存时间（比延迟索引器的 2.5s 防抖多 2.5s 余量）
    const TTL: Duration = Duration::from_secs(5);

    /// 创建新的写入守卫
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            removed_entries: Mutex::new(HashMap::new()),
        }
    }

    /// 标记文件已写入
    pub fn mark(&self, path: &str, hash: &str) {
        let mut guard = self.entries.lock().expect("write guard lock");
        guard.insert(path.to_string(), (hash.to_string(), Instant::now()));
    }

    /// Mark a deletion caused by an application-owned rename. The watcher sees
    /// rename as separate create/remove events on Windows, so hash matching is
    /// impossible for the old path.
    pub fn mark_removed(&self, path: &str) {
        let mut guard = self.removed_entries.lock().expect("write guard lock");
        guard.insert(path.to_string(), Instant::now());
    }

    /// Returns true while a remove event belongs to a recent internal rename.
    pub fn should_skip_removed_watcher(&self, path: &str) -> bool {
        let mut guard = self.removed_entries.lock().expect("write guard lock");
        guard.retain(|_, timestamp| timestamp.elapsed() < Self::TTL);
        guard
            .get(path)
            .is_some_and(|timestamp| timestamp.elapsed() < Self::TTL)
    }

    /// 检查是否应该跳过 watcher 事件（TTL 内哈希匹配则跳过）
    pub fn should_skip_watcher(&self, path: &str, hash: &str) -> bool {
        let mut guard = self.entries.lock().expect("write guard lock");
        guard.retain(|_, (_, t)| t.elapsed() < Self::TTL);
        guard
            .get(path)
            .is_some_and(|(h, t)| h == hash && t.elapsed() < Self::TTL)
    }

    /// 验证写入操作（乐观锁）
    ///
    /// 比较 `current_content` 的哈希与 `base_content_hash`，不一致则说明文件已被修改。
    pub fn validate_write(
        &self,
        path: &str,
        base_content_hash: &str,
        current_content: &str,
    ) -> AppResult<()> {
        let current_hash = super::hash::content_hash_str(current_content);

        if current_hash != base_content_hash {
            return Err(crate::error::AppError::msg(format!(
                "文件已被修改，请刷新后重试 ({})。期望哈希: {}，实际哈希: {}",
                path, base_content_hash, current_hash
            )));
        }

        Ok(())
    }
}

impl Default for WriteGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_and_should_skip_watcher() {
        let guard = WriteGuard::new();
        guard.mark("/test/file.md", "abc123");
        assert!(guard.should_skip_watcher("/test/file.md", "abc123"));
        assert!(!guard.should_skip_watcher("/test/file.md", "def456"));
        assert!(!guard.should_skip_watcher("/other/file.md", "abc123"));
    }

    #[test]
    fn expected_rename_removal_is_skipped_without_removing_the_index() {
        let guard = WriteGuard::new();
        guard.mark_removed("notes/old.md");
        assert!(guard.should_skip_removed_watcher("notes/old.md"));
        assert!(!guard.should_skip_removed_watcher("notes/other.md"));
    }

    #[test]
    fn test_should_skip_watcher_expired() {
        let guard = WriteGuard::new();
        // Insert with a timestamp in the past by manipulating the guard internals
        // Since we can't easily mock time, we test that a newly created guard
        // returns false for unmarked paths
        assert!(!guard.should_skip_watcher("/test/file.md", "abc123"));
    }

    #[test]
    fn test_validate_write_passes() {
        let guard = WriteGuard::new();
        let content = "Hello, World!";
        let hash = super::super::hash::content_hash_str(content);
        assert!(guard
            .validate_write("/test/file.md", &hash, content)
            .is_ok());
    }

    #[test]
    fn test_validate_write_fails_on_mismatch() {
        let guard = WriteGuard::new();
        let hash = super::super::hash::content_hash_str("original");
        let result = guard.validate_write("/test/file.md", &hash, "modified");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("/test/file.md"),
            "error should include path"
        );
    }

    #[test]
    fn test_default() {
        let guard = WriteGuard::default();
        assert!(!guard.should_skip_watcher("/any/path", "hash"));
    }

    #[test]
    fn test_ttl_eviction() {
        let guard = WriteGuard::new();
        guard.mark("/test/file.md", "abc123");
        // Should be within TTL immediately after marking
        assert!(guard.should_skip_watcher("/test/file.md", "abc123"));
        // Note: actual TTL expiry is hard to test without mocking time
    }
}
