use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

fn planned(
    name: &'static str,
    description: &'static str,
    access_level: ToolAccessLevel,
    requires_confirmation: bool,
) -> ToolCatalogEntry {
    ToolCatalogEntry {
        name,
        description,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {"type": "string"}
            }
        }),
        access_level,
        requires_confirmation,
        implementation: ToolImplementationStatus::Planned,
        default_enabled_without_skill: false,
        scene_affinity: &[],
        max_results: None,
    }
}

fn dispatchable(
    name: &'static str,
    description: &'static str,
    access_level: ToolAccessLevel,
    requires_confirmation: bool,
    input_schema: serde_json::Value,
) -> ToolCatalogEntry {
    ToolCatalogEntry {
        name,
        description,
        input_schema,
        access_level,
        requires_confirmation,
        implementation: ToolImplementationStatus::Dispatchable,
        default_enabled_without_skill: false,
        scene_affinity: &[],
        max_results: None,
    }
}

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    use ToolAccessLevel as Access;
    use ToolImplementationStatus as Impl;

    let mut tools = vec![
        planned(
            "fs_pick_file",
            "请求用户选择一个外部文件",
            Access::ReadIndex,
            true,
        ),
        planned(
            "fs_pick_folder",
            "请求用户授权一个外部目录",
            Access::ReadIndex,
            true,
        ),
        dispatchable(
            "fs_import_to_vault",
            "将用户授权的外部 Markdown 文件导入当前 vault",
            Access::WriteMarkdown,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "source_path": {"type": "string"},
                    "authorized_root": {"type": "string"},
                    "target_path": {"type": "string"},
                    "overwrite": {"type": "boolean", "default": false}
                },
                "required": ["source_path", "authorized_root", "target_path"]
            }),
        ),
        dispatchable(
            "fs_export",
            "将内容导出到用户确认的外部目标",
            Access::WriteMarkdown,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "dest_path": {"type": "string"},
                    "authorized_root": {"type": "string"},
                    "content": {"type": "string"},
                    "overwrite": {"type": "boolean", "default": false}
                },
                "required": ["dest_path", "authorized_root", "content"]
            }),
        ),
        dispatchable(
            "fs_read_authorized_folder",
            "读取已由用户授权的外部目录摘要",
            Access::ReadIndex,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "authorized_root": {"type": "string"},
                    "max_entries": {"type": "integer", "default": 100}
                },
                "required": ["authorized_root"]
            }),
        ),
        dispatchable(
            "fs_write_authorized_export",
            "写入用户授权的导出目录",
            Access::WriteMarkdown,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "authorized_root": {"type": "string"},
                    "target_path": {"type": "string"},
                    "content": {"type": "string"},
                    "overwrite": {"type": "boolean", "default": false}
                },
                "required": ["authorized_root", "target_path", "content"]
            }),
        ),
        planned(
            "doc_convert",
            "转换用户授权的文档到 Markdown 或 assets",
            Access::WriteCache,
            true,
        ),
        planned(
            "doc_ocr",
            "对用户授权文档或图片执行 OCR",
            Access::WriteCache,
            true,
        ),
        planned(
            "doc_extract_pdf",
            "从用户授权 PDF 提取文本",
            Access::WriteCache,
            true,
        ),
        planned(
            "doc_extract_table",
            "从用户授权文档提取表格",
            Access::WriteCache,
            true,
        ),
        dispatchable(
            "doc_normalize_markdown",
            "规范化 Markdown 内容",
            Access::WriteMarkdown,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {"type": "string"}
                },
                "required": ["content"]
            }),
        ),
        planned(
            "doc_fix_links",
            "修复 Markdown 内部链接",
            Access::WriteMarkdown,
            true,
        ),
        dispatchable(
            "doc_extract_citations",
            "从文档中抽取引用元数据",
            Access::WriteCache,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {"type": "string"}
                },
                "required": ["content"]
            }),
        ),
        dispatchable(
            "git_write_commit",
            "创建 Git commit",
            Access::WriteMarkdown,
            true,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"},
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"}
                    }
                },
                "required": ["message", "paths"]
            }),
        ),
        planned(
            "clipboard_write",
            "写入系统剪贴板",
            Access::WriteCache,
            true,
        ),
        planned("clipboard_read", "读取系统剪贴板", Access::ReadIndex, true),
        planned(
            "secret_create_update",
            "创建或更新 named credential",
            Access::WriteSettings,
            true,
        ),
        planned(
            "secret_read_plaintext",
            "读取明文凭据（Iris 永不支持）",
            Access::ReadIndex,
            true,
        ),
    ];

    tools.extend([
        ToolCatalogEntry {
            name: "git_read_status",
            description: "读取当前 vault 的 git status 摘要，不返回文件正文",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "max_chars": {"type": "integer", "default": 12000}
                }
            }),
            access_level: Access::ReadIndex,
            requires_confirmation: false,
            implementation: Impl::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "git_read_diff",
            description: "读取当前 vault 的 git diff 摘要，默认仅返回 stat",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "include_patch": {"type": "boolean", "default": false},
                    "max_chars": {"type": "integer", "default": 12000}
                }
            }),
            access_level: Access::ReadIndex,
            requires_confirmation: false,
            implementation: Impl::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "git_read_log",
            description: "读取当前 vault 的 git log 摘要",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 20, "maximum": 50},
                    "max_chars": {"type": "integer", "default": 12000}
                }
            }),
            access_level: Access::ReadIndex,
            requires_confirmation: false,
            implementation: Impl::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "secret_exists",
            description: "检查 named credential 是否存在，不读取明文值",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "service": {"type": "string", "description": "仅允许 iris.llm.* 或 iris.minimax"}
                },
                "required": ["service"]
            }),
            access_level: Access::ReadIndex,
            requires_confirmation: false,
            implementation: Impl::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
    ]);
    tools
}
