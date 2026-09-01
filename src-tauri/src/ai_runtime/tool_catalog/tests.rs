use super::*;
use crate::ai_runtime::tool_dispatch::{DISPATCHABLE_TOOL_NAMES, HARNESS_ONLY_TOOL_NAMES};

#[test]
fn catalog_owns_execution_metadata() {
    let web = catalog_find("web_search")
        .and_then(|entry| entry.execution_metadata)
        .expect("web_search metadata");
    assert_eq!(web.cost_class, "network");
    assert_eq!(web.output_policy, "bounded_packets");
    assert_eq!(web.evidence_policy, "current_run_web");

    let read = catalog_find("read_note")
        .and_then(|entry| entry.execution_metadata)
        .expect("read_note metadata");
    assert_eq!(read.cost_class, "local");
    assert_eq!(read.evidence_policy, "current_run_local");

    assert!(
        catalog_find("conclude_reasoning").is_some_and(|entry| entry.execution_metadata.is_none()),
        "internal-only tools without a production execution policy must not advertise metadata"
    );
}

#[test]
fn web_search_uses_the_open_query_and_current_run_url_contract_only() {
    let properties = catalog_find("web_search")
        .expect("web_search catalog entry")
        .input_schema["properties"]
        .as_object()
        .expect("web_search properties");

    assert!(properties.contains_key("query"));
    assert!(properties.contains_key("urls"));
    assert!(
        !properties.contains_key("gap"),
        "the generic loop must not expose the retired closed EvidenceGap protocol"
    );
}

#[test]
fn catalog_owns_one_stable_budget_class_for_every_budgeted_tool_kind() {
    assert_eq!(
        catalog_tool_budget_class("search_keyword"),
        Some(ToolBudgetClass::Local)
    );
    assert_eq!(
        catalog_tool_budget_class("web_search"),
        Some(ToolBudgetClass::Network)
    );
    assert_eq!(
        catalog_tool_budget_class("fs_read_authorized_folder"),
        Some(ToolBudgetClass::ExternalRead)
    );
    assert_eq!(
        catalog_tool_budget_class("system_time_now"),
        Some(ToolBudgetClass::Runtime)
    );
    assert_eq!(
        catalog_tool_budget_class("insert_text_at_cursor"),
        Some(ToolBudgetClass::ConfirmedChange)
    );
    assert_eq!(catalog_tool_budget_class("unknown_frozen_tool"), None);
}

#[test]
fn discovery_calls_are_catalog_metadata_not_domain_routing() {
    for tool_name in [
        "search_hybrid",
        "search_semantic",
        "search_keyword",
        "list_vault",
        "web_search",
    ] {
        assert!(
            catalog_find(tool_name)
                .expect("discovery tool in catalog")
                .is_discovery(),
            "{tool_name}"
        );
    }
    for tool_name in ["read_note", "get_regulation", "system_time_now"] {
        assert!(
            !catalog_find(tool_name)
                .expect("exact tool in catalog")
                .is_discovery(),
            "{tool_name}"
        );
    }
}

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
fn retired_current_fact_domain_tools_are_not_agent_visible() {
    for name in [
        "weather_lookup",
        "news_lookup",
        "finance_lookup",
        "entertainment_lookup",
        "sports_lookup",
    ] {
        assert!(
            catalog_find(name).is_none(),
            "{name} is a retired domain route and must not be exposed to a new Run"
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
fn dead_block_links_tool_is_not_exposed() {
    assert!(catalog_find("get_block_links").is_none());
    assert!(!DISPATCHABLE_TOOL_NAMES.contains(&"get_block_links"));
    assert!(
        crate::ai_runtime::agent_permissions::permission_profile_for_tool("get_block_links")
            .is_none()
    );
    assert!(
        crate::ai_runtime::subagent_coordinator::SubAgentCoordinator::child_tool_surface(&[
            "get_block_links".to_string()
        ])
        .is_empty()
    );
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
