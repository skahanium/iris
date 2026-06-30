use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "skills_list",
            description: "List installed Agent Skills for global and current vault scopes.",
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
            name: "mcp_runtime_profiles_list",
            description: "List configured MCP runtime profiles and readiness metadata. Does not invoke MCP tools or expose secrets.",
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
            name: "mcp_runtime_diagnostics",
            description: "Read MCP runtime diagnostics for a profile: profile status, discovered tool inventory, and recent health events. Metadata only; never launches external processes.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile_id": {"type": "string", "description": "optional MCP runtime profile id"},
                    "health_limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 20}
                }
            }),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "mcp_runtime_tools_list",
            description: "Run a confirmed live MCP stdio tools/list discovery for one configured profile. Starts a bounded local process and stores sanitized tool inventory.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile_id": {"type": "string", "description": "MCP runtime profile id"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                },
                "required": ["profile_id"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "mcp_runtime_health_check",
            description: "Run a confirmed live MCP stdio health check for one configured profile. Starts a bounded local process and records metadata-only health status.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile_id": {"type": "string", "description": "MCP runtime profile id"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                },
                "required": ["profile_id"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "mcp_runtime_capability_call",
            description: "Call an approved MCP provider through an Iris capability mapping. The model supplies a stable capability and arguments, never a raw MCP tool name.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "capability": {"type": "string", "description": "Stable Iris capability such as web.search"},
                    "arguments": {"type": "object", "description": "Provider arguments validated by the selected MCP tool schema"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                },
                "required": ["capability", "arguments"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },        ToolCatalogEntry {
            name: "mcp_runtime_profile_upsert",
            description: "Register or update an MCP runtime profile in the controlled Iris registry. Does not start the MCP server; requires confirmation because it changes future runtime capability wiring.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "MCP runtime profile id"},
                    "server_id": {"type": "string", "description": "MCP server catalog id"},
                    "vault_scope_hash": {"type": ["string", "null"], "description": "optional vault scope hash"},
                    "display_name": {"type": "string"},
                    "enabled": {"type": "boolean", "default": false},
                    "transport_config_json": {"type": "string", "description": "JSON object with transport-specific non-secret config"},
                    "env_bindings_json": {"type": "string", "description": "JSON object mapping env names to credential binding ids, never raw secrets"},
                    "status": {"type": "string", "enum": ["unknown", "ready", "degraded", "unavailable", "blocked"], "default": "unknown"},
                    "last_error": {"type": ["string", "null"]},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                },
                "required": ["id", "server_id", "display_name", "enabled", "transport_config_json", "env_bindings_json"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "mcp_runtime_profile_toggle",
            description: "Enable or disable an MCP runtime profile in the controlled Iris registry. Does not start external processes by itself.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile_id": {"type": "string", "description": "MCP runtime profile id"},
                    "enabled": {"type": "boolean"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                },
                "required": ["profile_id", "enabled"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "mcp_runtime_profile_delete",
            description: "Delete an MCP runtime profile and its stored MCP runtime metadata from the controlled Iris registry.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "profile_id": {"type": "string", "description": "MCP runtime profile id"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                },
                "required": ["profile_id"]
            }),
            access_level: ToolAccessLevel::ManageSkills,
            requires_confirmation: true,
            implementation: ToolImplementationStatus::Dispatchable,
            default_enabled_without_skill: true,
            scene_affinity: &[],
            max_results: None,
        },
        ToolCatalogEntry {
            name: "skills_install",
            description: "Install an Agent Skill from url, git, local path, or registry. SkillHub uses source=registry, registry=skillhub, path_or_url=<skill name or page URL>.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string", "enum": ["url", "git", "local", "registry"]},
                    "path_or_url": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "subpath": {"type": "string"},
                    "registry": {"type": "string", "description": "required for registry installs; defaults to skillhub"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
                    "expected_sha256": {"type": "string", "description": "optional SHA-256 expected value for URL install integrity checks"}
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
                    "reason": {"type": "string", "description": "shown in confirmation"},
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
            description: "Uninstall an installed Agent Skill.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
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
            description: "Update an installed Agent Skill from its recorded install source.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
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
            description: "Enable or disable an installed Agent Skill.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "enabled": {"type": "boolean"},
                    "reason": {"type": "string", "description": "shown in confirmation"},
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
            description: "Read a resource file from an installed Skill references, resources, or assets directory.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Skill name"},
                    "scope": {"type": "string", "enum": ["global", "vault"], "default": "vault"},
                    "relative_path": {"type": "string", "description": "example: references/guide.md"}
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
                    "reason": {"type": "string", "description": "shown in confirmation"},
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
