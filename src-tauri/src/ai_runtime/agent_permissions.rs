//! Markdown Agent permission contract for Phase 5.
//!
//! This module is the typed boundary between ToolCatalog/tool confirmation and
//! the user-facing permission model. It intentionally models Iris as a local
//! Markdown workspace, not as a general computer-control agent.

use crate::ai_runtime::tool_catalog::ToolCatalogEntry;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Atomic capability name used by preflight, grants, and audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentPermissionAtom {
    #[serde(rename = "vault.read")]
    VaultRead,
    #[serde(rename = "vault.search")]
    VaultSearch,
    #[serde(rename = "vault.write.patch")]
    VaultWritePatch,
    #[serde(rename = "vault.create_note")]
    VaultCreateNote,
    #[serde(rename = "vault.rename_move")]
    VaultRenameMove,
    #[serde(rename = "vault.delete_to_trash")]
    VaultDeleteToTrash,
    #[serde(rename = "vault.assets.read")]
    VaultAssetsRead,
    #[serde(rename = "vault.assets.write")]
    VaultAssetsWrite,
    #[serde(rename = "vault.versioning")]
    VaultVersioning,
    #[serde(rename = "runtime.context.read")]
    RuntimeContextRead,
    #[serde(rename = "fs.pick_file")]
    FsPickFile,
    #[serde(rename = "fs.pick_folder")]
    FsPickFolder,
    #[serde(rename = "fs.import_to_vault")]
    FsImportToVault,
    #[serde(rename = "fs.export")]
    FsExport,
    #[serde(rename = "fs.read_authorized_folder")]
    FsReadAuthorizedFolder,
    #[serde(rename = "fs.write_authorized_export")]
    FsWriteAuthorizedExport,
    #[serde(rename = "doc.convert")]
    DocConvert,
    #[serde(rename = "doc.ocr")]
    DocOcr,
    #[serde(rename = "doc.extract_pdf")]
    DocExtractPdf,
    #[serde(rename = "doc.extract_table")]
    DocExtractTable,
    #[serde(rename = "doc.normalize_markdown")]
    DocNormalizeMarkdown,
    #[serde(rename = "doc.fix_links")]
    DocFixLinks,
    #[serde(rename = "doc.extract_citations")]
    DocExtractCitations,
    #[serde(rename = "web.search")]
    WebSearch,
    #[serde(rename = "web.fetch")]
    WebFetch,
    #[serde(rename = "web.to_markdown")]
    WebToMarkdown,
    #[serde(rename = "web.download_to_assets")]
    WebDownloadToAssets,
    #[serde(rename = "web.citation_extract")]
    WebCitationExtract,
    #[serde(rename = "net.localhost")]
    NetLocalhost,
    #[serde(rename = "skill.read_resource")]
    SkillReadResource,
    #[serde(rename = "skill.write_storage")]
    SkillWriteStorage,
    #[serde(rename = "skill.request_capabilities")]
    SkillRequestCapabilities,
    #[serde(rename = "skill.execute_script_sandboxed")]
    SkillExecuteScriptSandboxed,
    #[serde(rename = "skill.install_dependency")]
    SkillInstallDependency,
    #[serde(rename = "skill.mcp_bridge")]
    SkillMcpBridge,
    #[serde(rename = "process.run_markdown_tool")]
    ProcessRunMarkdownTool,
    #[serde(rename = "process.run_readonly")]
    ProcessRunReadonly,
    #[serde(rename = "process.run_mutating")]
    ProcessRunMutating,
    #[serde(rename = "process.run_network")]
    ProcessRunNetwork,
    #[serde(rename = "process.long_running")]
    ProcessLongRunning,
    #[serde(rename = "process.kill_owned")]
    ProcessKillOwned,
    #[serde(rename = "git.read_status")]
    GitReadStatus,
    #[serde(rename = "git.read_diff")]
    GitReadDiff,
    #[serde(rename = "git.read_log")]
    GitReadLog,
    #[serde(rename = "git.write_commit")]
    GitWriteCommit,
    #[serde(rename = "clipboard.write")]
    ClipboardWrite,
    #[serde(rename = "clipboard.read")]
    ClipboardRead,
    #[serde(rename = "browser.read_page")]
    BrowserReadPage,
    #[serde(rename = "browser.screenshot")]
    BrowserScreenshot,
    #[serde(rename = "browser.control_page")]
    BrowserControlPage,
    #[serde(rename = "secret.exists")]
    SecretExists,
    #[serde(rename = "secret.use_named")]
    SecretUseNamed,
    #[serde(rename = "secret.create_update")]
    SecretCreateUpdate,
    #[serde(rename = "secret.read_plaintext")]
    SecretReadPlaintext,
    /// Existing non-note runtime state such as memory and scheduled task rows.
    #[serde(rename = "app_state.read")]
    AppStateRead,
    /// Existing non-note runtime state such as memory and scheduled task rows.
    #[serde(rename = "app_state.write")]
    AppStateWrite,
}

impl AgentPermissionAtom {
    /// Stable string used in SQL and IPC payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VaultRead => "vault.read",
            Self::VaultSearch => "vault.search",
            Self::VaultWritePatch => "vault.write.patch",
            Self::VaultCreateNote => "vault.create_note",
            Self::VaultRenameMove => "vault.rename_move",
            Self::VaultDeleteToTrash => "vault.delete_to_trash",
            Self::VaultAssetsRead => "vault.assets.read",
            Self::VaultAssetsWrite => "vault.assets.write",
            Self::VaultVersioning => "vault.versioning",
            Self::RuntimeContextRead => "runtime.context.read",
            Self::FsPickFile => "fs.pick_file",
            Self::FsPickFolder => "fs.pick_folder",
            Self::FsImportToVault => "fs.import_to_vault",
            Self::FsExport => "fs.export",
            Self::FsReadAuthorizedFolder => "fs.read_authorized_folder",
            Self::FsWriteAuthorizedExport => "fs.write_authorized_export",
            Self::DocConvert => "doc.convert",
            Self::DocOcr => "doc.ocr",
            Self::DocExtractPdf => "doc.extract_pdf",
            Self::DocExtractTable => "doc.extract_table",
            Self::DocNormalizeMarkdown => "doc.normalize_markdown",
            Self::DocFixLinks => "doc.fix_links",
            Self::DocExtractCitations => "doc.extract_citations",
            Self::WebSearch => "web.search",
            Self::WebFetch => "web.fetch",
            Self::WebToMarkdown => "web.to_markdown",
            Self::WebDownloadToAssets => "web.download_to_assets",
            Self::WebCitationExtract => "web.citation_extract",
            Self::NetLocalhost => "net.localhost",
            Self::SkillReadResource => "skill.read_resource",
            Self::SkillWriteStorage => "skill.write_storage",
            Self::SkillRequestCapabilities => "skill.request_capabilities",
            Self::SkillExecuteScriptSandboxed => "skill.execute_script_sandboxed",
            Self::SkillInstallDependency => "skill.install_dependency",
            Self::SkillMcpBridge => "skill.mcp_bridge",
            Self::ProcessRunMarkdownTool => "process.run_markdown_tool",
            Self::ProcessRunReadonly => "process.run_readonly",
            Self::ProcessRunMutating => "process.run_mutating",
            Self::ProcessRunNetwork => "process.run_network",
            Self::ProcessLongRunning => "process.long_running",
            Self::ProcessKillOwned => "process.kill_owned",
            Self::GitReadStatus => "git.read_status",
            Self::GitReadDiff => "git.read_diff",
            Self::GitReadLog => "git.read_log",
            Self::GitWriteCommit => "git.write_commit",
            Self::ClipboardWrite => "clipboard.write",
            Self::ClipboardRead => "clipboard.read",
            Self::BrowserReadPage => "browser.read_page",
            Self::BrowserScreenshot => "browser.screenshot",
            Self::BrowserControlPage => "browser.control_page",
            Self::SecretExists => "secret.exists",
            Self::SecretUseNamed => "secret.use_named",
            Self::SecretCreateUpdate => "secret.create_update",
            Self::SecretReadPlaintext => "secret.read_plaintext",
            Self::AppStateRead => "app_state.read",
            Self::AppStateWrite => "app_state.write",
        }
    }
}

/// User-facing risk tier for permission prompts and audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl PermissionRiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// Scope kinds that can later be persisted as grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScopeKind {
    Request,
    Session,
    Vault,
    Folder,
    Skill,
    Global,
}

impl PermissionScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Session => "session",
            Self::Vault => "vault",
            Self::Folder => "folder",
            Self::Skill => "skill",
            Self::Global => "global",
        }
    }
}

/// User decision vocabulary for Phase 5 permission prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allow,
    AllowOnce,
    AllowForSession,
    DenyOnce,
    DenyAlwaysForThisSkill,
    OpenSettings,
}

impl PermissionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::AllowOnce => "allow_once",
            Self::AllowForSession => "allow_for_session",
            Self::DenyOnce => "deny_once",
            Self::DenyAlwaysForThisSkill => "deny_always_for_this_skill",
            Self::OpenSettings => "open_settings",
        }
    }
}

/// Safe, content-free effect summary for UI and audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionEffectSummary {
    pub permission_name: String,
    pub scope_kind: PermissionScopeKind,
    pub scope_summary: String,
    pub risk_level: PermissionRiskLevel,
    pub reversible_by: String,
    pub blocked_reason: Option<String>,
}

/// Stable permission profile for a tool or planned capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPermissionProfile {
    pub atoms: Vec<AgentPermissionAtom>,
    pub risk_level: PermissionRiskLevel,
    pub supported: bool,
}

/// Preflight result for a single tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPreflight {
    pub tool_name: String,
    pub decision: PermissionDecision,
    pub effects: Vec<PermissionEffectSummary>,
    pub blocked: bool,
}

/// Metadata-only grant input persisted after explicit user consent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionGrantInput<'a> {
    pub permission_name: &'a str,
    pub decision: PermissionDecision,
    pub scope_kind: PermissionScopeKind,
    pub scope_value: Option<&'a str>,
    pub risk_level: PermissionRiskLevel,
    pub skill_id: Option<&'a str>,
    pub expires_at: Option<&'a str>,
}

/// Stored permission grant record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionGrantRecord {
    pub permission_name: String,
    pub decision: PermissionDecision,
    pub scope_kind: PermissionScopeKind,
    pub scope_value: Option<String>,
    pub risk_level: PermissionRiskLevel,
    pub skill_id: Option<String>,
    pub expires_at: Option<String>,
}

/// Metadata-only audit input for user permission decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionAuditInput<'a> {
    pub request_id: &'a str,
    pub skill_id: Option<&'a str>,
    pub tool_name: &'a str,
    pub permission_name: &'a str,
    pub decision: PermissionDecision,
    pub scope_summary: &'a str,
    pub risk_level: PermissionRiskLevel,
    pub result_status: &'a str,
}

/// Map current and planned tool names to Phase 5 permission atoms.
pub fn permission_profile_for_tool(tool_name: &str) -> Option<ToolPermissionProfile> {
    use AgentPermissionAtom as Atom;
    use PermissionRiskLevel as Risk;

    let profile = match tool_name {
        "search_hybrid" | "search_semantic" | "search_keyword" | "list_vault" | "get_backlinks"
        | "get_block_links" | "conclude_reasoning" | "spawn_subagent" => {
            (vec![Atom::VaultSearch], Risk::Low, true)
        }
        "read_note" | "get_outline" | "get_regulation" | "get_context_packets" => {
            (vec![Atom::VaultRead], Risk::Low, true)
        }
        "system_time_now" | "app_context_read" | "capabilities_read" => {
            (vec![Atom::RuntimeContextRead], Risk::Low, true)
        }
        "web_search" => (vec![Atom::WebSearch], Risk::Low, true),
        "fetch_web_page" | "web_fetch_batch" | "readability_fetch" | "rendered_fetch" => {
            (vec![Atom::WebFetch], Risk::Medium, true)
        }
        "insert_text_at_cursor" | "replace_selection" | "add_tags" => {
            (vec![Atom::VaultWritePatch], Risk::Medium, true)
        }
        "create_note_from_deposit" | "vault_create_note" => {
            (vec![Atom::VaultCreateNote], Risk::Medium, true)
        }
        "vault_rename_move" => (vec![Atom::VaultRenameMove], Risk::High, true),
        "vault_delete_to_trash" => (vec![Atom::VaultDeleteToTrash], Risk::High, true),
        "vault_asset_write" => (vec![Atom::VaultAssetsWrite], Risk::Medium, true),
        "vault_version_list" => (vec![Atom::VaultVersioning], Risk::Low, true),
        "fs_pick_file" => (vec![Atom::FsPickFile], Risk::High, false),
        "fs_pick_folder" => (vec![Atom::FsPickFolder], Risk::High, false),
        "fs_import_to_vault" => (vec![Atom::FsImportToVault], Risk::High, true),
        "fs_export" => (vec![Atom::FsExport], Risk::High, true),
        "fs_read_authorized_folder" => (vec![Atom::FsReadAuthorizedFolder], Risk::High, true),
        "fs_write_authorized_export" => (vec![Atom::FsWriteAuthorizedExport], Risk::High, true),
        "doc_convert" => (vec![Atom::DocConvert], Risk::Medium, false),
        "doc_ocr" => (vec![Atom::DocOcr], Risk::Medium, false),
        "doc_extract_pdf" => (vec![Atom::DocExtractPdf], Risk::Medium, false),
        "doc_extract_table" => (vec![Atom::DocExtractTable], Risk::Medium, false),
        "doc_normalize_markdown" => (vec![Atom::DocNormalizeMarkdown], Risk::Medium, true),
        "doc_fix_links" => (vec![Atom::DocFixLinks], Risk::Medium, false),
        "doc_extract_citations" => (vec![Atom::DocExtractCitations], Risk::Medium, true),
        "web_to_markdown" => (vec![Atom::WebToMarkdown], Risk::Medium, true),
        "web_download_to_assets" => (vec![Atom::WebDownloadToAssets], Risk::Medium, true),
        "web_citation_extract" => (vec![Atom::WebCitationExtract], Risk::Medium, true),
        "net_localhost" => (vec![Atom::NetLocalhost], Risk::High, false),
        "skill_request_capabilities" => (vec![Atom::SkillRequestCapabilities], Risk::Low, true),
        "skill_execute_script_sandboxed" => (
            vec![Atom::SkillExecuteScriptSandboxed],
            Risk::Critical,
            false,
        ),
        "skill_install_dependency" => (vec![Atom::SkillInstallDependency], Risk::Critical, false),
        "skill_mcp_bridge" => (vec![Atom::SkillMcpBridge], Risk::Critical, false),
        "process_run_markdown_tool" => (vec![Atom::ProcessRunMarkdownTool], Risk::High, false),
        "process_run_readonly" => (vec![Atom::ProcessRunReadonly], Risk::High, true),
        "process_run_mutating" => (vec![Atom::ProcessRunMutating], Risk::High, false),
        "process_run_network" => (vec![Atom::ProcessRunNetwork], Risk::Critical, false),
        "process_long_running" => (vec![Atom::ProcessLongRunning], Risk::Critical, false),
        "process_kill_owned" => (vec![Atom::ProcessKillOwned], Risk::High, false),
        "git_read_status" => (vec![Atom::GitReadStatus], Risk::Low, true),
        "git_read_diff" => (vec![Atom::GitReadDiff], Risk::Low, true),
        "git_read_log" => (vec![Atom::GitReadLog], Risk::Low, true),
        "git_write_commit" => (vec![Atom::GitWriteCommit], Risk::High, true),
        "clipboard_write" => (vec![Atom::ClipboardWrite], Risk::High, false),
        "clipboard_read" => (vec![Atom::ClipboardRead], Risk::High, false),
        "browser_read_page" => (vec![Atom::BrowserReadPage], Risk::High, false),
        "browser_screenshot" => (vec![Atom::BrowserScreenshot], Risk::High, false),
        "browser_control_page" => (vec![Atom::BrowserControlPage], Risk::High, false),
        "confirm_block_link"
        | "save_genre_template"
        | "update_user_rule"
        | "memory_write"
        | "scheduled_task_create"
        | "scheduled_task_delete" => (vec![Atom::AppStateWrite], Risk::Medium, true),
        "memory_read" | "scheduled_task_list" => (vec![Atom::AppStateRead], Risk::Low, true),
        "skills_list"
        | "mcp_runtime_profiles_list"
        | "mcp_runtime_diagnostics"
        | "skills_read_resource"
        | "skills_workspace_list"
        | "skills_workspace_read" => (vec![Atom::SkillReadResource], Risk::Low, true),
        "mcp_runtime_tools_list" | "mcp_runtime_health_check" | "mcp_runtime_capability_call" => (
            vec![Atom::SkillMcpBridge, Atom::ProcessRunReadonly],
            Risk::High,
            true,
        ),
        "mcp_runtime_profile_upsert"
        | "mcp_runtime_profile_toggle"
        | "mcp_runtime_profile_delete" => (
            vec![Atom::SkillMcpBridge, Atom::SkillWriteStorage],
            Risk::High,
            true,
        ),
        "skills_install"
        | "skills_prepare_workspace"
        | "skills_uninstall"
        | "skills_update"
        | "skills_toggle"
        | "skills_workspace_write" => (vec![Atom::SkillWriteStorage], Risk::High, true),
        "secret.exists" | "secret_exists" => (vec![Atom::SecretExists], Risk::Low, true),
        "secret.use_named" | "secret_use_named" => (vec![Atom::SecretUseNamed], Risk::High, false),
        "secret.create_update" | "secret_create_update" => {
            (vec![Atom::SecretCreateUpdate], Risk::Critical, false)
        }
        "secret.read_plaintext" | "secret_read_plaintext" => {
            (vec![Atom::SecretReadPlaintext], Risk::Critical, false)
        }
        _ => return None,
    };

    Some(ToolPermissionProfile {
        atoms: profile.0,
        risk_level: profile.1,
        supported: profile.2,
    })
}

/// Persist or replace a metadata-only permission grant.
pub fn upsert_permission_grant(db: &Database, input: &PermissionGrantInput<'_>) -> AppResult<()> {
    validate_permission_storage_value(input.scope_value)?;
    validate_permission_storage_value(input.skill_id)?;
    validate_permission_storage_value(input.expires_at)?;
    validate_permission_storage_value(Some(input.permission_name))?;

    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM agent_permission_grants
             WHERE permission_name = ?1
               AND scope_kind = ?2
               AND COALESCE(scope_value, '') = COALESCE(?3, '')
               AND COALESCE(skill_id, '') = COALESCE(?4, '')",
            params![
                input.permission_name,
                input.scope_kind.as_str(),
                input.scope_value,
                input.skill_id,
            ],
        )?;
        conn.execute(
            "INSERT INTO agent_permission_grants (
                permission_name, decision, scope_kind, scope_value, risk_level, skill_id, expires_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                input.permission_name,
                input.decision.as_str(),
                input.scope_kind.as_str(),
                input.scope_value,
                input.risk_level.as_str(),
                input.skill_id,
                input.expires_at,
            ],
        )?;
        Ok(())
    })
}

/// Find an active persisted grant for an exact permission/scope pair.
pub fn find_permission_grant(
    db: &Database,
    permission_name: &str,
    scope_kind: PermissionScopeKind,
    scope_value: Option<&str>,
    skill_id: Option<&str>,
) -> AppResult<Option<PermissionGrantRecord>> {
    validate_permission_storage_value(Some(permission_name))?;
    validate_permission_storage_value(scope_value)?;
    validate_permission_storage_value(skill_id)?;

    db.with_read_conn(|conn| {
        let result = conn.query_row(
            "SELECT permission_name, decision, scope_kind, scope_value, risk_level, skill_id, expires_at
             FROM agent_permission_grants
             WHERE permission_name = ?1
               AND scope_kind = ?2
               AND COALESCE(scope_value, '') = COALESCE(?3, '')
               AND COALESCE(skill_id, '') = COALESCE(?4, '')
               AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))
             ORDER BY updated_at DESC, id DESC
             LIMIT 1",
            params![permission_name, scope_kind.as_str(), scope_value, skill_id],
            |row| {
                let decision: String = row.get(1)?;
                let scope_kind: String = row.get(2)?;
                let risk_level: String = row.get(4)?;
                Ok(PermissionGrantRecord {
                    permission_name: row.get(0)?,
                    decision: parse_permission_decision(&decision).unwrap_or(PermissionDecision::DenyOnce),
                    scope_kind: parse_scope_kind(&scope_kind).unwrap_or(PermissionScopeKind::Request),
                    scope_value: row.get(3)?,
                    risk_level: parse_permission_risk_level(&risk_level).unwrap_or(PermissionRiskLevel::Critical),
                    skill_id: row.get(5)?,
                    expires_at: row.get(6)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    })
}

/// Record a metadata-only audit row for a permission decision.
pub fn record_permission_audit(db: &Database, input: &PermissionAuditInput<'_>) -> AppResult<()> {
    validate_permission_storage_value(Some(input.request_id))?;
    validate_permission_storage_value(input.skill_id)?;
    validate_permission_storage_value(Some(input.tool_name))?;
    validate_permission_storage_value(Some(input.permission_name))?;
    validate_permission_storage_value(Some(input.scope_summary))?;
    validate_permission_storage_value(Some(input.result_status))?;

    let scope_summary = safe_fragment(input.scope_summary);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO agent_permission_audit (
                request_id, skill_id, tool_name, permission_name, decision,
                scope_summary, risk_level, result_status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                input.request_id,
                input.skill_id,
                input.tool_name,
                input.permission_name,
                input.decision.as_str(),
                scope_summary,
                input.risk_level.as_str(),
                input.result_status,
            ],
        )?;
        Ok(())
    })
}

/// Build a safe preflight payload for a tool call.
pub fn preflight_tool_permission(
    entry: &ToolCatalogEntry,
    args: &serde_json::Value,
    skill_id: Option<&str>,
) -> PermissionPreflight {
    let profile =
        permission_profile_for_tool(entry.name).unwrap_or_else(|| ToolPermissionProfile {
            atoms: vec![],
            risk_level: PermissionRiskLevel::Critical,
            supported: false,
        });
    let blocked = !profile.supported;
    let decision = if blocked {
        PermissionDecision::DenyOnce
    } else if entry.requires_confirmation || profile.risk_level != PermissionRiskLevel::Low {
        PermissionDecision::AllowOnce
    } else {
        PermissionDecision::Allow
    };
    let effects = profile
        .atoms
        .iter()
        .map(|atom| PermissionEffectSummary {
            permission_name: atom.as_str().to_string(),
            scope_kind: scope_kind_for_atom(*atom),
            scope_summary: scope_summary(entry.name, args, skill_id),
            risk_level: profile.risk_level,
            reversible_by: reversible_by(*atom),
            blocked_reason: blocked.then(|| "unsupported by Iris Markdown workspace scope".into()),
        })
        .collect();

    PermissionPreflight {
        tool_name: entry.name.to_string(),
        decision,
        effects,
        blocked,
    }
}

fn scope_kind_for_atom(atom: AgentPermissionAtom) -> PermissionScopeKind {
    match atom {
        AgentPermissionAtom::SkillReadResource
        | AgentPermissionAtom::SkillWriteStorage
        | AgentPermissionAtom::SkillRequestCapabilities
        | AgentPermissionAtom::SkillExecuteScriptSandboxed
        | AgentPermissionAtom::SkillInstallDependency
        | AgentPermissionAtom::SkillMcpBridge => PermissionScopeKind::Skill,
        AgentPermissionAtom::FsPickFile
        | AgentPermissionAtom::FsPickFolder
        | AgentPermissionAtom::FsReadAuthorizedFolder
        | AgentPermissionAtom::FsWriteAuthorizedExport => PermissionScopeKind::Folder,
        AgentPermissionAtom::SecretExists
        | AgentPermissionAtom::SecretUseNamed
        | AgentPermissionAtom::SecretCreateUpdate
        | AgentPermissionAtom::SecretReadPlaintext => PermissionScopeKind::Global,
        _ => PermissionScopeKind::Request,
    }
}

fn reversible_by(atom: AgentPermissionAtom) -> String {
    match atom {
        AgentPermissionAtom::VaultWritePatch
        | AgentPermissionAtom::VaultCreateNote
        | AgentPermissionAtom::VaultRenameMove
        | AgentPermissionAtom::VaultVersioning => "version history".into(),
        AgentPermissionAtom::VaultDeleteToTrash => "recycle bin restore".into(),
        AgentPermissionAtom::SkillWriteStorage => "Skills settings".into(),
        AgentPermissionAtom::SecretCreateUpdate => "system credential manager".into(),
        _ => "permission settings".into(),
    }
}

fn scope_summary(tool_name: &str, args: &serde_json::Value, skill_id: Option<&str>) -> String {
    if let Some(skill) = skill_id {
        return format!("skill={}", safe_fragment(skill));
    }
    if matches!(
        tool_name,
        "fetch_web_page" | "web_fetch_batch" | "readability_fetch" | "rendered_fetch"
    ) {
        return web_scope_summary(args);
    }
    for key in ["target_path", "path", "note_path"] {
        if let Some(path) = args.get(key).and_then(|v| v.as_str()) {
            return format!("path={}", safe_fragment(path));
        }
    }
    "current request".into()
}

fn web_scope_summary(args: &serde_json::Value) -> String {
    if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
        return format!("domain={}", domain_from_url(url));
    }
    if let Some(urls) = args.get("urls").and_then(|v| v.as_array()) {
        let domains: Vec<String> = urls
            .iter()
            .filter_map(|v| v.as_str())
            .map(domain_from_url)
            .collect();
        if !domains.is_empty() {
            return format!("domains={}", domains.join(","));
        }
    }
    "domain=unknown".into()
}

fn domain_from_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

fn safe_fragment(value: &str) -> String {
    value.chars().take(160).collect()
}

/// Conservative guard used by audit tests and future insertion paths.
pub fn audit_contains_sensitive_summary(summary: &str) -> bool {
    let lower = summary.to_lowercase();
    [
        "api_key",
        "apikey",
        "token=",
        "bearer ",
        "password",
        "clipboard body",
        "screenshot content",
        "image base64",
        "note body",
        "external file body",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn validate_permission_storage_value(value: Option<&str>) -> AppResult<()> {
    if let Some(value) = value {
        if audit_contains_sensitive_summary(value) {
            return Err(AppError::msg(
                "permission metadata contains sensitive content",
            ));
        }
    }
    Ok(())
}

fn parse_scope_kind(value: &str) -> Option<PermissionScopeKind> {
    Some(match value {
        "request" => PermissionScopeKind::Request,
        "session" => PermissionScopeKind::Session,
        "vault" => PermissionScopeKind::Vault,
        "folder" => PermissionScopeKind::Folder,
        "skill" => PermissionScopeKind::Skill,
        "global" => PermissionScopeKind::Global,
        _ => return None,
    })
}

fn parse_permission_decision(value: &str) -> Option<PermissionDecision> {
    Some(match value {
        "allow" => PermissionDecision::Allow,
        "allow_once" => PermissionDecision::AllowOnce,
        "allow_for_session" => PermissionDecision::AllowForSession,
        "deny_once" => PermissionDecision::DenyOnce,
        "deny_always_for_this_skill" => PermissionDecision::DenyAlwaysForThisSkill,
        "open_settings" => PermissionDecision::OpenSettings,
        _ => return None,
    })
}

fn parse_permission_risk_level(value: &str) -> Option<PermissionRiskLevel> {
    Some(match value {
        "low" => PermissionRiskLevel::Low,
        "medium" => PermissionRiskLevel::Medium,
        "high" => PermissionRiskLevel::High,
        "critical" => PermissionRiskLevel::Critical,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_atom_strings_are_stable() {
        assert_eq!(
            AgentPermissionAtom::VaultWritePatch.as_str(),
            "vault.write.patch"
        );
        assert_eq!(
            AgentPermissionAtom::SecretReadPlaintext.as_str(),
            "secret.read_plaintext"
        );
    }

    #[test]
    fn live_mcp_runtime_tools_require_process_confirmation_permissions() {
        for name in [
            "mcp_runtime_tools_list",
            "mcp_runtime_health_check",
            "mcp_runtime_capability_call",
        ] {
            let profile = permission_profile_for_tool(name).unwrap_or_else(|| panic!("{name}"));
            assert!(profile.supported);
            assert_eq!(profile.risk_level, PermissionRiskLevel::High);
            assert!(profile.atoms.contains(&AgentPermissionAtom::SkillMcpBridge));
            assert!(profile
                .atoms
                .contains(&AgentPermissionAtom::ProcessRunReadonly));
        }
    }

    #[test]
    fn mcp_profile_management_requires_storage_confirmation_permissions() {
        for name in [
            "mcp_runtime_profile_upsert",
            "mcp_runtime_profile_toggle",
            "mcp_runtime_profile_delete",
        ] {
            let profile = permission_profile_for_tool(name).unwrap_or_else(|| panic!("{name}"));
            assert!(profile.supported);
            assert_eq!(profile.risk_level, PermissionRiskLevel::High);
            assert!(profile.atoms.contains(&AgentPermissionAtom::SkillMcpBridge));
            assert!(profile
                .atoms
                .contains(&AgentPermissionAtom::SkillWriteStorage));
        }
    }
}
