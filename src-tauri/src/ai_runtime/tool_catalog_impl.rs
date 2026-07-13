use std::sync::LazyLock;

use crate::ai_runtime::ToolAccessLevel;

#[path = "tool_catalog/boundary.rs"]
mod boundary_impl;
#[path = "tool_catalog/capability.rs"]
mod capability_impl;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolImplementationStatus {
    Dispatchable,
    HarnessOnly,
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
    pub default_enabled_without_skill: bool,
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
#[path = "tool_catalog/tests.rs"]
mod tests;
