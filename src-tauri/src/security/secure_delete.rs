//! 安全文件删除 - 覆写后删除临时文件

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::error::AppResult;

/// 安全删除文件：先用零覆写整个文件内容，再删除
///
/// 用于处理包含敏感信息的临时文件（如 API 响应缓存）。
/// 覆写 + `sync_all` 确保操作系统将覆写操作刷入存储介质，
/// 随后 `remove_file` 释放目录条目。
#[allow(dead_code)]
pub fn secure_delete(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(path)?;
    let len = metadata.len();

    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;

    let zeros = vec![0u8; 4096];
    let mut written = 0u64;
    while written < len {
        let chunk = std::cmp::min(4096, (len - written) as usize);
        file.write_all(&zeros[..chunk])?;
        written += chunk as u64;
    }
    file.sync_all()?;

    // 正常删除
    std::fs::remove_file(path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn secure_delete_removes_file() {
        let tmp = NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "sensitive data").unwrap();
        assert!(tmp.path().exists());

        secure_delete(tmp.path()).unwrap();
        assert!(!tmp.path().exists());
    }

    #[test]
    fn secure_delete_nonexistent_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let ghost = dir.path().join("does_not_exist.txt");
        assert!(secure_delete(&ghost).is_ok());
    }
}
