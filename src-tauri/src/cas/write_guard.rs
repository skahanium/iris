use std::collections::HashMap;
use std::sync::Mutex;
use crate::error::AppResult;

/// 写入守卫 - 实现乐观锁
pub struct WriteGuard {
    /// 最近写入的文件哈希缓存
    recent_writes: Mutex<HashMap<String, String>>,
}

impl WriteGuard {
    /// 创建新的写入守卫
    pub fn new() -> Self {
        Self {
            recent_writes: Mutex::new(HashMap::new()),
        }
    }

    /// 标记文件已写入
    pub fn mark(&self, path: &str, hash: &str) {
        let mut cache = self.recent_writes.lock().unwrap();
        cache.insert(path.to_string(), hash.to_string());
    }

    /// 检查是否应该跳过 watcher 事件
    pub fn should_skip_watcher(&self, path: &str, hash: &str) -> bool {
        let cache = self.recent_writes.lock().unwrap();
        if let Some(recent_hash) = cache.get(path) {
            if recent_hash == hash {
                return true;
            }
        }
        false
    }

    /// 验证写入操作（乐观锁）
    pub fn validate_write(
        &self,
        _path: &str,
        base_content_hash: &str,
        current_content: &str,
    ) -> AppResult<()> {
        let current_hash = super::hash::content_hash_str(current_content);

        if current_hash != base_content_hash {
            return Err(crate::error::AppError::msg(format!(
                "文件已被修改，请刷新后重试。期望哈希: {}，实际哈希: {}",
                base_content_hash, current_hash
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
    fn test_validate_write_passes() {
        let guard = WriteGuard::new();
        let content = "Hello, World!";
        let hash = super::super::hash::content_hash_str(content);
        assert!(guard.validate_write("/test/file.md", &hash, content).is_ok());
    }

    #[test]
    fn test_validate_write_fails_on_mismatch() {
        let guard = WriteGuard::new();
        let hash = super::super::hash::content_hash_str("original");
        assert!(guard.validate_write("/test/file.md", &hash, "modified").is_err());
    }

    #[test]
    fn test_default() {
        let guard = WriteGuard::default();
        assert!(!guard.should_skip_watcher("/any/path", "hash"));
    }
}
