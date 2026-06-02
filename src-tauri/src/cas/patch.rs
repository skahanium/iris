use serde::{Deserialize, Serialize};

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

/// 应用补丁到内容
pub fn apply_patch(
    patch: &crate::ai_runtime::PatchProposal,
    current_content: &str,
) -> AppResult<String> {
    // 验证补丁范围
    let content_len = current_content.len();
    if patch.range.start > content_len || patch.range.end > content_len {
        return Err(crate::error::AppError::msg(format!(
            "补丁范围越界: [{}, {}) 超出内容长度 {}",
            patch.range.start, patch.range.end, content_len
        )));
    }

    // 验证原文匹配
    let actual_original = &current_content[patch.range.start..patch.range.end];
    if actual_original != patch.original_text {
        return Err(crate::error::AppError::msg(format!(
            "原文不一致: 期望 {:?}，实际 {:?}",
            &patch.original_text[..patch.original_text.len().min(50)],
            &actual_original[..actual_original.len().min(50)]
        )));
    }

    // 应用补丁
    let mut new_content =
        String::with_capacity(current_content.len() + patch.replacement_text.len());
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    Ok(new_content)
}

/// 应用补丁到文件（带乐观锁）
pub fn apply_patch_to_file(
    cas_store: &super::store::CasObjectStore,
    write_guard: &super::write_guard::WriteGuard,
    patch: &crate::ai_runtime::PatchProposal,
    current_content: &str,
) -> AppResult<PatchApplyResult> {
    // 1. 验证 base_content_hash
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

    // 2. 应用补丁
    let new_content = apply_patch(patch, current_content)?;
    let new_hash = cas_store.write_content(&new_content)?;

    // 3. 更新写入守卫
    write_guard.mark(&patch.target_path, &new_hash);

    Ok(PatchApplyResult {
        success: true,
        new_content_hash: new_hash,
        new_content,
        error: None,
        warnings: vec![],
    })
}
