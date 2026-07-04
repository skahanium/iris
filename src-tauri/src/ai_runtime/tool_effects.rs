//! Central classification of tool execution effects.
//!
//! The harness uses this module to decide which already-authorized tool calls
//! can run concurrently. Permission, confirmation, and audit remain separate
//! gates; this module only describes execution ordering constraints.

use crate::ai_runtime::tool_catalog::{ToolCatalogEntry, ToolImplementationStatus};
use crate::ai_runtime::ToolAccessLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionClass {
    /// Read-only tools whose successful result can be merged deterministically
    /// in the model's original tool-call order.
    ParallelRead,
    /// Read tools kept sequential until explicitly classified as parallel-safe.
    SequentialRead,
    /// Tools that can mutate vault files, app state, settings, caches, exports,
    /// git state, or any external durable target.
    Mutation,
    /// Harness-level control-flow tools. These are orchestrated by the harness,
    /// not by the ordinary dispatch batcher.
    HarnessControl,
}

const HARNESS_CONTROL_TOOLS: &[&str] = &["spawn_subagent", "conclude_reasoning"];

const PARALLEL_READ_TOOLS: &[&str] = &[
    "search_hybrid",
    "search_semantic",
    "search_keyword",
    "get_regulation",
    "system_time_now",
    "app_context_read",
    "capabilities_read",
    "web_search",
    "read_note",
    "list_vault",
    "get_outline",
    "get_backlinks",
    "get_block_links",
    "memory_read",
    "scheduled_task_list",
    "vault_version_list",
    "skills_list",
    "git_read_status",
    "git_read_diff",
    "git_read_log",
    "secret_exists",
    "fs_read_authorized_folder",
    "doc_extract_citations",
];

const MUTATION_TOOLS: &[&str] = &[
    "memory_write",
    "scheduled_task_create",
    "scheduled_task_delete",
    "vault_create_note",
    "vault_rename_move",
    "vault_delete_to_trash",
    "vault_asset_write",
    "insert_text_at_cursor",
    "replace_selection",
    "fs_import_to_vault",
    "fs_export",
    "fs_write_authorized_export",
    "doc_normalize_markdown",
    "git_write_commit",
];

pub fn classify_tool(tool_name: &str) -> ToolExecutionClass {
    if HARNESS_CONTROL_TOOLS.contains(&tool_name) {
        ToolExecutionClass::HarnessControl
    } else if PARALLEL_READ_TOOLS.contains(&tool_name) {
        ToolExecutionClass::ParallelRead
    } else if MUTATION_TOOLS.contains(&tool_name) {
        ToolExecutionClass::Mutation
    } else {
        ToolExecutionClass::SequentialRead
    }
}

pub fn classify_catalog_entry(entry: &ToolCatalogEntry) -> ToolExecutionClass {
    let explicit = classify_tool(entry.name);
    if explicit != ToolExecutionClass::SequentialRead {
        return explicit;
    }

    if entry.implementation == ToolImplementationStatus::HarnessOnly {
        return ToolExecutionClass::HarnessControl;
    }

    if entry.requires_confirmation || is_mutating_access(entry.access_level) {
        return ToolExecutionClass::Mutation;
    }

    ToolExecutionClass::SequentialRead
}

pub fn is_parallel_read(entry: &ToolCatalogEntry) -> bool {
    classify_catalog_entry(entry) == ToolExecutionClass::ParallelRead
}

pub fn is_mutation(entry: &ToolCatalogEntry) -> bool {
    classify_catalog_entry(entry) == ToolExecutionClass::Mutation
}

fn is_mutating_access(access: ToolAccessLevel) -> bool {
    matches!(
        access,
        ToolAccessLevel::WriteCache
            | ToolAccessLevel::WriteMarkdown
            | ToolAccessLevel::WriteSettings
            | ToolAccessLevel::ManageSkills
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::tool_catalog::{catalog_find, TOOL_CATALOG};

    #[test]
    fn read_tools_are_parallel_read() {
        for name in PARALLEL_READ_TOOLS {
            assert_eq!(
                classify_tool(name),
                ToolExecutionClass::ParallelRead,
                "{name}"
            );
        }
    }

    #[test]
    fn write_tools_are_mutations() {
        for name in MUTATION_TOOLS {
            assert_eq!(classify_tool(name), ToolExecutionClass::Mutation, "{name}");
        }
    }

    #[test]
    fn harness_control_tools_are_not_parallel_reads() {
        for name in HARNESS_CONTROL_TOOLS {
            assert_eq!(
                classify_tool(name),
                ToolExecutionClass::HarnessControl,
                "{name}"
            );
        }
    }

    #[test]
    fn context_packets_stay_sequential_because_they_read_the_current_ledger() {
        let entry = catalog_find("get_context_packets").unwrap();
        assert_eq!(
            classify_catalog_entry(entry),
            ToolExecutionClass::SequentialRead
        );
    }
    #[test]
    fn catalog_entries_follow_explicit_classification() {
        for name in PARALLEL_READ_TOOLS
            .iter()
            .chain(MUTATION_TOOLS.iter())
            .chain(HARNESS_CONTROL_TOOLS.iter())
        {
            let entry =
                catalog_find(name).unwrap_or_else(|| panic!("catalog entry missing: {name}"));
            assert_eq!(classify_catalog_entry(entry), classify_tool(name), "{name}");
        }
    }

    #[test]
    fn unclassified_confirmed_or_write_tools_default_to_mutation() {
        for entry in TOOL_CATALOG.iter() {
            let class = classify_catalog_entry(entry);
            if class == ToolExecutionClass::SequentialRead
                && (entry.requires_confirmation || is_mutating_access(entry.access_level))
            {
                panic!("{} should not default to SequentialRead", entry.name);
            }
        }
    }
}
