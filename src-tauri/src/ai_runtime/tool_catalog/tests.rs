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
fn reign_in_catalog_exposes_only_one_network_tool() {
    let names: Vec<&str> = TOOL_CATALOG.iter().map(|entry| entry.name).collect();
    assert!(names.contains(&"web_search"));
    for legacy in [
        "fetch_web_page",
        "web_fetch_batch",
        "readability_fetch",
        "rendered_fetch",
        "skills_install",
        "skills_prepare_workspace",
        "skills_update",
        "skills_toggle",
        "skills_workspace_list",
        "skills_workspace_read",
        "skills_workspace_write",
        "mcp_runtime_profiles_list",
        "mcp_runtime_diagnostics",
        "mcp_runtime_tool_inventory_list",
        "mcp_runtime_health_events_list",
        "mcp_runtime_tools_list",
        "mcp_runtime_health_check",
        "mcp_runtime_capability_call",
        "mcp_server_catalog_upsert",
        "mcp_runtime_profile_upsert",
        "mcp_runtime_profile_toggle",
        "mcp_runtime_profile_delete",
    ] {
        assert!(
            !names.contains(&legacy),
            "{legacy} must not be agent-visible"
        );
    }
}

#[test]
fn total_catalog_count() {
    assert!(
        catalog_total_count() < 98,
        "catalog should shrink after removing legacy Skills/MCP/fetch tools"
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
    ] {
        assert!(
            catalog_find(name).is_some(),
            "{name} missing from ToolCatalog"
        );
    }
    assert!(!catalog_find("memory_read").unwrap().requires_confirmation);
    assert!(catalog_find("memory_write").unwrap().requires_confirmation);
}
