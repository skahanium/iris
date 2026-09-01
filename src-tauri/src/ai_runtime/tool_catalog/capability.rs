use crate::ai_runtime::run_contract::CapabilityId;
use crate::ai_runtime::{ToolAccessLevel, ToolCapabilityAffinity};

use super::{ToolBudgetClass, ToolCatalogEntry};

impl ToolCatalogEntry {
    /// Whether this tool discovers candidate resources rather than reading one
    /// already-selected target. The loop uses this capability-shaped metadata
    /// only to bound one model turn; it never infers a task domain from it.
    pub(crate) fn is_discovery(&self) -> bool {
        matches!(
            self.name,
            "search_hybrid" | "search_semantic" | "search_keyword" | "list_vault" | "web_search"
        )
    }

    /// Stable capability contract required before this tool can be exposed or
    /// dispatched. This is intentionally exact by tool name: access levels are
    /// presentation metadata and must never broaden a Run authorization.
    pub(crate) fn required_capability_ids(&self) -> &'static [&'static str] {
        match self.name {
            "search_hybrid" | "search_semantic" | "search_keyword" | "list_vault"
            | "get_backlinks" | "vault_version_list" | "get_regulation" | "read_note"
            | "get_outline" => &["vault.read"],
            "get_context_packets" => &["context.read"],
            "system_time_now" | "app_context_read" | "capabilities_read" => &["runtime.read"],
            "web_search" => &["web.search"],
            "insert_text_at_cursor" | "replace_selection" => &["note.apply_patch"],
            "vault_create_note"
            | "vault_rename_move"
            | "vault_delete_to_trash"
            | "vault_asset_write" => &["vault.manage"],
            "memory_read" => &["memory.read"],
            "memory_write" => &["memory.write"],
            "scheduled_task_list" => &["schedule.read"],
            "scheduled_task_create" | "scheduled_task_delete" => &["schedule.manage"],
            "skills_list" => &["skills.read"],
            "spawn_subagent" => &["harness.child_run"],
            "conclude_reasoning" => &["harness.conclude"],
            "fs_pick_file" | "fs_pick_folder" | "fs_read_authorized_folder" => {
                &["external_fs.read"]
            }
            "fs_import_to_vault" => &["external_fs.import"],
            "fs_export" | "fs_write_authorized_export" => &["external_fs.export"],
            "doc_convert" | "doc_ocr" | "doc_extract_pdf" | "doc_extract_table" => {
                &["document.extract"]
            }
            "doc_normalize_markdown" | "doc_fix_links" => &["document.transform"],
            "doc_extract_citations" => &["document.citations"],
            "git_read_status" | "git_read_diff" | "git_read_log" => &["git.read"],
            "git_write_commit" => &["git.write"],
            "clipboard_read" => &["clipboard.read"],
            "clipboard_write" => &["clipboard.write"],
            "secret_exists" => &["secret.metadata.read"],
            "secret_create_update" => &["secret.manage"],
            "secret_read_plaintext" => &["secret.plaintext.read"],
            _ => &["internal.unrecognized_tool"],
        }
    }

    /// Whether this exact catalog tool is authorized by the immutable Run
    /// capability snapshot.
    pub(crate) fn is_authorized_by(&self, capabilities: &[CapabilityId]) -> bool {
        self.required_capability_ids().iter().any(|required| {
            capabilities
                .iter()
                .any(|capability| capability.as_str() == *required)
        })
    }

    /// Capability affinity for task-policy driven tool exposure.
    pub fn capability_affinity(&self) -> Vec<ToolCapabilityAffinity> {
        capability_affinity(self)
    }

    /// Budget classification for a concrete built-in dispatch.
    ///
    /// Execution metadata wins where it exists. The fallback is deliberately
    /// based on authority rather than task/domain wording so every catalog
    /// tool has one stable classification.
    pub(crate) fn budget_class(&self) -> ToolBudgetClass {
        if let Some(metadata) = self.execution_metadata {
            return match metadata.cost_class {
                "local" => ToolBudgetClass::Local,
                "network" => ToolBudgetClass::Network,
                "runtime" => ToolBudgetClass::Runtime,
                "external_read" => ToolBudgetClass::ExternalRead,
                _ => ToolBudgetClass::ConfirmedChange,
            };
        }
        match self.name {
            "fs_read_authorized_folder"
            | "doc_convert"
            | "doc_ocr"
            | "doc_extract_pdf"
            | "doc_extract_table" => ToolBudgetClass::ExternalRead,
            _ if self.requires_confirmation => ToolBudgetClass::ConfirmedChange,
            _ => match self.access_level {
                ToolAccessLevel::ReadIndex | ToolAccessLevel::ReadNoteSpan => {
                    ToolBudgetClass::Local
                }
                ToolAccessLevel::ReadProfile => ToolBudgetClass::Runtime,
                ToolAccessLevel::Network => ToolBudgetClass::Network,
                ToolAccessLevel::WriteCache
                | ToolAccessLevel::WriteMarkdown
                | ToolAccessLevel::WriteSettings
                | ToolAccessLevel::ManageSkills => ToolBudgetClass::ConfirmedChange,
            },
        }
    }
}

fn capability_affinity(entry: &ToolCatalogEntry) -> Vec<ToolCapabilityAffinity> {
    use ToolCapabilityAffinity::*;

    let mut capabilities = match entry.access_level {
        ToolAccessLevel::ReadIndex => vec![SearchNotes],
        ToolAccessLevel::ReadNoteSpan | ToolAccessLevel::ReadProfile => vec![ReadNotes],
        ToolAccessLevel::Network => vec![WebFetch],
        ToolAccessLevel::WriteMarkdown => vec![WriteNotes, PatchDocument],
        ToolAccessLevel::WriteCache | ToolAccessLevel::WriteSettings => vec![WriteNotes],
        ToolAccessLevel::ManageSkills => vec![SkillManagement],
    };

    match entry.name {
        "conclude_reasoning" | "spawn_subagent" | "get_context_packets" => {
            push_unique(&mut capabilities, ResearchSynthesis);
        }
        "get_regulation" => {
            push_unique(&mut capabilities, ResearchSynthesis);
        }
        name if name.starts_with("skills_") => {
            push_unique(&mut capabilities, SkillManagement);
        }
        name if name.starts_with("vault_") => {
            push_unique(&mut capabilities, VaultOrganize);
        }
        "add_tags" | "create_note_from_deposit" => {
            push_unique(&mut capabilities, VaultOrganize);
        }
        _ => {}
    }

    capabilities
}

fn push_unique(capabilities: &mut Vec<ToolCapabilityAffinity>, capability: ToolCapabilityAffinity) {
    if !capabilities.contains(&capability) {
        capabilities.push(capability);
    }
}
