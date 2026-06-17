use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "skills_list",
            description: "列出已安装的 Agent Skills（全局 + 当前库）",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_install",
            description: "安装 Agent Skill（url / git / local / registry）。SkillHub: source=registry, registry=skillhub, path_or_url=<skill名或页面URL>。建议对 URL 安装提供 expected_sha256 校验内容完整性。",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string", "enum": ["url", "git", "local", "registry"]},
                    "path_or_url": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "subpath": {"type": "string"},
                    "registry": {"type": "string", "description": "registry 时必填，默认 skillhub"},
                    "reason": {"type": "string", "description": "展示于确认框"},
                    "expected_sha256": {"type": "string", "description": "URL 安装时可选的 SHA-256 预期值，用于校验下载内容完整性"}
                },
                "required": ["source", "path_or_url", "scope"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_prepare_workspace",
            description: "Prepare a Skill workspace under the current vault's hidden .iris/skills-workspaces/<skill>/ archive",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "reason": {"type": "string", "description": "shown in confirmation"}
                },
                "required": ["name"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_uninstall",
            description: "卸载已安装的 Agent Skill",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "reason": {"type": "string"}
                },
                "required": ["name", "scope"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_update",
            description: "根据记录的安装来源更新已安装的 Agent Skill",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "reason": {"type": "string"}
                },
                "required": ["name", "scope"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_toggle",
            description: "启用或禁用已安装的 Agent Skill",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "enabled": {"type": "boolean"},
                    "reason": {"type": "string"}
                },
                "required": ["name", "scope", "enabled"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_read_resource",
            description: "读取已安装 Skill 的 references/、resources/ 或 assets/ 下资源文件",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Skill 名称"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "relative_path": {"type": "string", "description": "如 references/guide.md"}
                },
                "required": ["name", "relative_path"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_workspace_list",
            description: "List files in a Skill's hidden derived-document workspace. Use this for Skill runtime artifacts instead of list_vault.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "path": {"type": "string", "description": "optional workspace-relative folder"}
                },
                "required": ["name"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_workspace_read",
            description: "Read a file from a Skill's hidden derived-document workspace. Use this for Skill runtime artifacts instead of read_note.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "relative_path": {"type": "string"}
                },
                "required": ["name", "relative_path"]
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_workspace_write",
            description: "Write a file into a Skill's hidden derived-document workspace. Use this for Skill runtime artifacts instead of vault_create_note.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "relative_path": {"type": "string"},
                    "content": {"type": "string"},
                    "mode": {"type": "string", "enum": ["create", "overwrite"], "default": "overwrite"},
                    "reason": {"type": "string"}
                },
                "required": ["name", "relative_path", "content"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: false,
            scene_affinity: &[],
            max_results: None,
        },
    ]
}
