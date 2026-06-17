use serde::{Deserialize, Serialize};

use crate::ai_types::PatchProposal;
use crate::error::AppResult;

/// 补丁应用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    pub success: bool,
    pub new_content_hash: String,
    pub new_content: String,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

/// 应用补丁到内容。
///
/// 委托 `writing_workflow::validate_patch` 进行验证（含哈希校验），
/// 然后执行拼接。修复后的容量计算避免了过度分配。
pub fn apply_patch(patch: &PatchProposal, current_content: &str) -> AppResult<String> {
    crate::ai_runtime::writing_workflow::validate_patch(patch, current_content)
        .map_err(|e| crate::error::AppError::msg(format!("补丁验证失败: {e}")))?;

    let replaced_len = patch.range.end - patch.range.start;
    let mut new_content =
        String::with_capacity(current_content.len() - replaced_len + patch.replacement_text.len());
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    Ok(new_content)
}

/// 应用补丁到文件（带乐观锁）
pub fn apply_patch_to_file(
    cas_store: &super::store::CasObjectStore,
    write_guard: &super::write_guard::WriteGuard,
    patch: &PatchProposal,
    current_content: &str,
) -> AppResult<PatchApplyResult> {
    // 1. 验证 base_content_hash（hash mismatch 返回 success: false，不报错）
    let current_hash = super::hash::content_hash_str(current_content);
    if current_hash != patch.base_content_hash {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: String::new(),
            new_content: String::new(),
            error: Some(format!(
                "内容哈希不匹配，请刷新后重试。期望哈希: {}，实际哈希: {}",
                patch.base_content_hash, current_hash
            )),
            warnings: vec![],
        });
    }

    // 2. 验证范围和原文（不重复验证哈希）
    validate_patch_content(patch, current_content)?;

    // 3. 应用补丁
    let replaced_len = patch.range.end - patch.range.start;
    let mut new_content =
        String::with_capacity(current_content.len() - replaced_len + patch.replacement_text.len());
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    let new_hash = cas_store.write_content(&new_content)?;

    // 4. 更新写入守卫
    write_guard.mark(&patch.target_path, &new_hash);

    Ok(PatchApplyResult {
        success: true,
        new_content_hash: new_hash,
        new_content,
        error: None,
        warnings: vec![],
    })
}

/// 验证补丁范围和原文匹配（不含哈希校验）。
///
/// 与 `writing_workflow::validate_patch` 中的范围/原文校验逻辑一致。
fn validate_patch_content(patch: &PatchProposal, current_content: &str) -> AppResult<()> {
    let content_len = current_content.len();
    if patch.range.start > patch.range.end
        || patch.range.end > content_len
        || !current_content.is_char_boundary(patch.range.start)
        || !current_content.is_char_boundary(patch.range.end)
    {
        return Err(crate::error::AppError::msg(format!(
            "补丁范围越界: [{}, {}) 超出内容长度 {}",
            patch.range.start, patch.range.end, content_len
        )));
    }

    let actual_original = &current_content[patch.range.start..patch.range.end];
    if actual_original != patch.original_text {
        return Err(crate::error::AppError::msg(format!(
            "原文不一致: 期望 {:?}，实际 {:?}",
            &patch.original_text[..patch.original_text.len().min(50)],
            &actual_original[..actual_original.len().min(50)]
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::{PatchProposal, RiskLevel, SourceSpan};
    use crate::cas::hash::content_hash_str;
    use crate::cas::store::CasObjectStore;
    use crate::cas::write_guard::WriteGuard;

    fn make_patch(
        base_content_hash: String,
        range: std::ops::Range<usize>,
        original_text: &str,
        replacement_text: &str,
    ) -> PatchProposal {
        PatchProposal {
            id: "test-patch".to_string(),
            target_path: "/test/file.md".to_string(),
            base_content_hash,
            range: SourceSpan {
                start: range.start,
                end: range.end,
            },
            original_text: original_text.to_string(),
            replacement_text: replacement_text.to_string(),
            evidence_packet_ids: vec![],
            risk_level: RiskLevel::Low,
            warnings: vec![],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_apply_patch_success() {
        let content = "Hello, World!";
        let hash = content_hash_str(content);
        let patch = make_patch(hash, 7..12, "World", "Rust");
        let result = apply_patch(&patch, content).unwrap();
        assert_eq!(result, "Hello, Rust!");
    }

    #[test]
    fn test_apply_patch_range_out_of_bounds() {
        let content = "Hello";
        let hash = content_hash_str(content);
        let patch = make_patch(hash, 0..10, "Hello", "Hi");
        let err = apply_patch(&patch, content).unwrap_err();
        assert!(
            err.to_string().contains("越界"),
            "should report range error"
        );
    }

    #[test]
    fn test_apply_patch_original_text_mismatch() {
        let content = "Hello, World!";
        let hash = content_hash_str(content);
        let patch = make_patch(hash, 0..5, "Wrong", "Hi");
        let err = apply_patch(&patch, content).unwrap_err();
        assert!(
            err.to_string().contains("原文不一致"),
            "should report text mismatch"
        );
    }

    #[test]
    fn test_apply_patch_to_file_success() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CasObjectStore::new(tmp.path().to_path_buf()).unwrap();
        store.enable_encryption([9u8; 32]);
        let guard = WriteGuard::new();

        let content = "Hello, World!";
        let hash = content_hash_str(content);
        let patch = make_patch(hash, 7..12, "World", "Rust");

        let result = apply_patch_to_file(&store, &guard, &patch, content).unwrap();
        assert!(result.success);
        assert_eq!(result.new_content, "Hello, Rust!");
        assert!(!result.new_content_hash.is_empty());
        assert!(result.error.is_none());

        // 验证内容已写入 CAS
        let stored = store.read_blob_content(&result.new_content_hash).unwrap();
        assert_eq!(stored, "Hello, Rust!");
    }

    #[test]
    fn test_apply_patch_to_file_hash_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CasObjectStore::new(tmp.path().to_path_buf()).unwrap();
        let guard = WriteGuard::new();

        let content = "Hello, World!";
        let patch = make_patch("wrong_hash".to_string(), 7..12, "World", "Rust");

        let result = apply_patch_to_file(&store, &guard, &patch, content).unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("哈希不匹配"));
    }

    #[test]
    fn test_apply_patch_to_file_range_out_of_bounds() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CasObjectStore::new(tmp.path().to_path_buf()).unwrap();
        let guard = WriteGuard::new();

        let content = "Hello";
        let hash = content_hash_str(content);
        let patch = make_patch(hash, 0..10, "Hello", "Hi");

        let err = apply_patch_to_file(&store, &guard, &patch, content).unwrap_err();
        assert!(err.to_string().contains("越界"));
    }

    #[test]
    fn test_apply_patch_to_file_original_text_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CasObjectStore::new(tmp.path().to_path_buf()).unwrap();
        let guard = WriteGuard::new();

        let content = "Hello, World!";
        let hash = content_hash_str(content);
        let patch = make_patch(hash, 0..5, "Wrong", "Hi");

        let err = apply_patch_to_file(&store, &guard, &patch, content).unwrap_err();
        assert!(err.to_string().contains("原文不一致"));
    }
}
