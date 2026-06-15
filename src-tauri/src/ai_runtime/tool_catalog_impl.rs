use std::sync::LazyLock;

use crate::ai_runtime::{AiScene, ToolAccessLevel};

#[path = "tool_catalog/boundary.rs"]
mod boundary_impl;
#[path = "tool_catalog/groups.rs"]
mod groups_impl;
#[path = "tool_catalog/read.rs"]
mod read_impl;
#[path = "tool_catalog/root.rs"]
mod root_impl;
#[path = "tool_catalog/skills.rs"]
mod skills_impl;
#[path = "tool_catalog/vault.rs"]
mod vault_impl;
#[path = "tool_catalog/web.rs"]
mod web_impl;
#[path = "tool_catalog/write.rs"]
mod write_impl;

/// Implementation status of a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolImplementationStatus {
    /// Has a real handler in `dispatch_tool_inner`.
    Dispatchable,
    /// Handled inside the harness loop (e.g. `spawn_subagent`, `conclude_reasoning`).
    HarnessOnly,
    /// Registered for future implementation; not currently exposed.
    Planned,
}

/// A single entry in the global tool catalog.
#[derive(Debug, Clone)]
pub struct ToolCatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub access_level: ToolAccessLevel,
    pub requires_confirmation: bool,
    pub implementation: ToolImplementationStatus,
    /// Whether this tool is available when no skill is active.
    pub default_enabled_without_skill: bool,
    /// Scenes where this tool is naturally relevant (superset of old scene_allowlist).
    pub scene_affinity: &'static [AiScene],
    /// Optional cap on result count passed to the retrieval layer.
    pub max_results: Option<u32>,
}

/// The complete built-in tool catalog. Add new tools through group modules only.
pub static TOOL_CATALOG: LazyLock<Vec<ToolCatalogEntry>> =
    LazyLock::new(groups_impl::collect_tool_catalog);

/// Tool names that have real `dispatch_tool_inner` handlers.
pub fn catalog_dispatchable_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.implementation == ToolImplementationStatus::Dispatchable)
        .map(|e| e.name)
        .collect()
}

/// Tool names handled inside the harness loop (not via dispatch).
pub fn catalog_harness_only_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.implementation == ToolImplementationStatus::HarnessOnly)
        .map(|e| e.name)
        .collect()
}

/// Tool names that can be exposed to the model (dispatchable or harness-only).
pub fn catalog_exposable_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.implementation != ToolImplementationStatus::Planned)
        .map(|e| e.name)
        .collect()
}

/// Core read-only tools available without any skill activation.
pub fn catalog_default_readonly_names() -> Vec<&'static str> {
    TOOL_CATALOG
        .iter()
        .filter(|e| e.default_enabled_without_skill && !e.requires_confirmation)
        .map(|e| e.name)
        .collect()
}

/// Look up a catalog entry by name.
pub fn catalog_find(name: &str) -> Option<&'static ToolCatalogEntry> {
    TOOL_CATALOG.iter().find(|e| e.name == name)
}

/// Total number of catalog entries.
pub fn catalog_total_count() -> usize {
    TOOL_CATALOG.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::tool_dispatch::{DISPATCHABLE_TOOL_NAMES, HARNESS_ONLY_TOOL_NAMES};

    #[test]
    fn catalog_has_all_dispatchable_tools() {
        let catalog_disp = catalog_dispatchable_names();
        for name in DISPATCHABLE_TOOL_NAMES {
            assert!(
                catalog_disp.contains(name),
                "dispatch tool '{name}' missing from catalog dispatchable list"
            );
        }
    }

    #[test]
    fn catalog_has_all_harness_only_tools() {
        let catalog_ho = catalog_harness_only_names();
        for name in HARNESS_ONLY_TOOL_NAMES {
            assert!(
                catalog_ho.contains(name),
                "harness-only tool '{name}' missing from catalog harness-only list"
            );
        }
    }

    #[test]
    fn dispatch_list_matches_catalog() {
        let catalog_disp = catalog_dispatchable_names();
        for name in DISPATCHABLE_TOOL_NAMES {
            assert!(catalog_disp.contains(name), "{name} not in catalog");
        }
        for name in &catalog_disp {
            assert!(
                DISPATCHABLE_TOOL_NAMES.contains(name),
                "catalog dispatchable '{name}' not in DISPATCHABLE_TOOL_NAMES"
            );
        }
    }

    #[test]
    fn harness_only_list_matches_catalog() {
        let catalog_ho = catalog_harness_only_names();
        for name in HARNESS_ONLY_TOOL_NAMES {
            assert!(catalog_ho.contains(name), "{name} not in catalog");
        }
        for name in &catalog_ho {
            let in_harness_list = HARNESS_ONLY_TOOL_NAMES.contains(name);
            let entry = catalog_find(name).unwrap();
            let is_write_tool = entry.requires_confirmation;
            assert!(
                in_harness_list || is_write_tool,
                "catalog harness-only '{name}' is neither in HARNESS_ONLY_TOOL_NAMES nor a write tool"
            );
        }
    }

    #[test]
    fn no_duplicate_names() {
        let mut seen = Vec::new();
        for entry in TOOL_CATALOG.iter() {
            assert!(
                !seen.contains(&entry.name),
                "duplicate tool name: {}",
                entry.name
            );
            seen.push(entry.name);
        }
    }

    #[test]
    fn default_readonly_tools_present() {
        let defaults = catalog_default_readonly_names();
        let required = [
            "system_time_now",
            "app_context_read",
            "capabilities_read",
            "search_hybrid",
            "search_semantic",
            "search_keyword",
            "read_note",
            "list_vault",
            "get_outline",
            "get_backlinks",
            "conclude_reasoning",
        ];
        for name in required {
            assert!(
                defaults.contains(&name),
                "core default tool '{name}' missing from default_readonly list"
            );
        }
    }

    #[test]
    fn write_tools_not_in_default_readonly() {
        let defaults = catalog_default_readonly_names();
        let write_tools = [
            "insert_text_at_cursor",
            "replace_selection",
            "add_tags",
            "confirm_block_link",
            "save_genre_template",
            "update_user_rule",
            "create_note_from_deposit",
            "vault_create_note",
            "vault_rename_move",
            "vault_delete_to_trash",
            "vault_asset_write",
        ];
        for name in write_tools {
            assert!(
                !defaults.contains(&name),
                "write tool '{name}' should not be in default_readonly"
            );
        }
    }

    #[test]
    fn total_catalog_count() {
        assert_eq!(
            catalog_total_count(),
            83,
            "catalog should have exactly 83 tools"
        );
    }

    #[test]
    fn catalog_find_works() {
        assert!(catalog_find("read_note").is_some());
        assert!(catalog_find("nonexistent_tool").is_none());
    }

    #[test]
    fn catalog_exposes_skill_root_capability_tools() {
        for name in [
            "memory_read",
            "memory_write",
            "scheduled_task_create",
            "scheduled_task_list",
            "scheduled_task_delete",
            "web_fetch_batch",
            "readability_fetch",
            "rendered_fetch",
        ] {
            assert!(
                catalog_find(name).is_some(),
                "{name} missing from ToolCatalog"
            );
        }
        assert!(!catalog_find("memory_read").unwrap().requires_confirmation);
        assert!(catalog_find("memory_write").unwrap().requires_confirmation);
        assert!(
            catalog_find("web_fetch_batch")
                .unwrap()
                .requires_confirmation
        );
    }
}
