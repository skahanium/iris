use iris_lib::ai_runtime::agent_permissions::{
    audit_contains_sensitive_summary, permission_profile_for_tool, preflight_tool_permission,
    AgentPermissionAtom, PermissionDecision, PermissionRiskLevel,
};
use iris_lib::ai_runtime::tool_catalog::{catalog_find, TOOL_CATALOG};

#[test]
fn permission_profiles_cover_current_catalog() {
    for entry in TOOL_CATALOG.iter() {
        if entry.implementation
            == iris_lib::ai_runtime::tool_catalog::ToolImplementationStatus::Planned
        {
            continue;
        }
        let profile = permission_profile_for_tool(entry.name)
            .unwrap_or_else(|| panic!("missing permission profile for {}", entry.name));
        assert!(
            !profile.atoms.is_empty(),
            "{} must map to at least one permission atom",
            entry.name
        );
    }
}

#[test]
fn markdown_write_tools_map_to_patch_permission() {
    let profile = permission_profile_for_tool("replace_selection").unwrap();
    assert_eq!(profile.risk_level, PermissionRiskLevel::Medium);
    assert!(profile
        .atoms
        .contains(&AgentPermissionAtom::VaultWritePatch));

    let preflight = preflight_tool_permission(
        catalog_find("replace_selection").unwrap(),
        &serde_json::json!({
            "target_path": "notes/a.md",
            "replacement": "redacted body"
        }),
        None,
    );
    assert_eq!(preflight.decision, PermissionDecision::AllowOnce);
    assert_eq!(preflight.effects[0].permission_name, "vault.write.patch");
    assert_eq!(preflight.effects[0].risk_level, PermissionRiskLevel::Medium);
    assert!(preflight.effects[0].scope_summary.contains("notes/a.md"));
    assert!(!preflight.effects[0].scope_summary.contains("redacted body"));
}

#[test]
fn web_fetch_preflight_summarizes_domain_not_body() {
    let preflight = preflight_tool_permission(
        catalog_find("fetch_web_page").unwrap(),
        &serde_json::json!({
            "url": "https://example.com/articles/phase5",
            "reason": "read external evidence"
        }),
        None,
    );
    assert_eq!(preflight.decision, PermissionDecision::AllowOnce);
    assert_eq!(preflight.effects[0].permission_name, "web.fetch");
    assert_eq!(preflight.effects[0].risk_level, PermissionRiskLevel::Medium);
    assert!(preflight.effects[0].scope_summary.contains("example.com"));
    assert!(!preflight.effects[0]
        .scope_summary
        .contains("/articles/phase5"));
}

#[test]
fn secret_plaintext_read_is_never_supported() {
    let profile = permission_profile_for_tool("secret.read_plaintext").unwrap();
    assert_eq!(profile.risk_level, PermissionRiskLevel::Critical);
    assert!(profile
        .atoms
        .contains(&AgentPermissionAtom::SecretReadPlaintext));
    assert!(!profile.supported);
}

#[test]
fn audit_sensitive_summary_detector_flags_forbidden_material() {
    assert!(audit_contains_sensitive_summary("api_key=sk-test"));
    assert!(audit_contains_sensitive_summary("clipboard body: hello"));
    assert!(audit_contains_sensitive_summary("screenshot content bytes"));
    assert!(!audit_contains_sensitive_summary(
        "permission=vault.write.patch, path=notes/a.md, risk=medium"
    ));
}
