import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Iris AI reign-in target state", () => {
  it("removes external Skill install registry and service modules", () => {
    const runtimeMod = read("src-tauri/src/ai_runtime/mod.rs");
    const skillsImpl = read("src-tauri/src/ai_runtime/skills_impl.rs");
    const persona = read("src-tauri/src/ai_runtime/persona_resolver.rs");
    const taskPolicy = read("src-tauri/src/ai_runtime/agent_task_policy.rs");
    const aiTypes = read("src-tauri/src/ai_types/mod.rs");

    expect(runtimeMod).not.toContain("pub mod skill_registry");
    expect(runtimeMod).not.toContain("pub mod skill_install_service");
    expect(existsSync("src-tauri/src/ai_runtime/skill_registry.rs")).toBe(
      false,
    );
    expect(
      existsSync("src-tauri/src/ai_runtime/skill_install_service.rs"),
    ).toBe(false);

    for (const token of [
      "install_from_url",
      "install_from_git",
      "install_from_local",
      "SAFE_GIT_CLONE_ARGS",
      "validate_skill_remote_url",
      "validate_skill_git_url",
      "validate_local_skill_source",
    ]) {
      expect(skillsImpl).not.toContain(token);
    }

    for (const token of [
      "pub fn uninstall_skill",
      "pub fn toggle_skill",
      "pub fn read_skill_content",
      "pub fn write_skill_content",
      "pub fn set_enabled",
      "emit_skills_changed",
    ]) {
      expect(skillsImpl).not.toContain(token);
    }

    for (const token of ["启停或删除", "install, update, toggle"]) {
      expect(`${persona}\n${taskPolicy}\n${aiTypes}`).not.toContain(token);
    }
  });

  it("keeps Skills as prompt-only confirmed files without resources or workspace runtime", () => {
    const model = read("src-tauri/src/ai_runtime/skills/model.rs");
    const scan = read("src-tauri/src/ai_runtime/skills/scan.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");
    const ipc = read("src/lib/ipc.ts");

    for (const token of [
      "SkillRuntimeCapability",
      "SkillWorkspaceManifest",
      "SkillWorkspaceDocument",
      "requested_capabilities",
      "required_resources",
      "optional_resources",
      "workspace_manifest",
      "external_dependencies",
      "mcp_dependencies",
      "workspace_declared",
      "workspace_prepared",
      "workspace_root",
      "skills_read_resource",
    ]) {
      expect(model + scan + activation + ipc).not.toContain(token);
    }

    expect(model).toContain("SkillConfirmationStatus");
    expect(model).toContain("SkillScopeRule");
    expect(scan).toContain("confirmed_hash");
    expect(activation).toContain("SkillConfirmationStatus::Confirmed");
  });

  it("routes user-facing web evidence through WebEvidenceBroker only", () => {
    const writing = read("src-tauri/src/commands/writing_commands.rs");
    const document = read("src-tauri/src/commands/document_commands.rs");
    const citation = read("src-tauri/src/commands/citation_commands.rs");
    const engine = read("src-tauri/src/llm/engine.rs");
    const llmTypes = read("src-tauri/src/llm/mod.rs");
    const tsIpcTypes = read("src/types/ipc.ts");
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");

    for (const source of [writing, document, citation]) {
      expect(source).toContain("collect_web_evidence");
      expect(source).not.toContain("fetch_search_context_for_db");
      expect(source).not.toContain("web_packets_from_fetch");
    }

    expect(engine).not.toContain("apply_web_search");
    expect(engine).not.toContain("prepend_web_search_context_for_db");
    expect(engine).not.toContain("params.web_search");
    expect(llmTypes).not.toContain("web_search: Option<bool>");
    expect(tsIpcTypes).not.toContain("web_search?: boolean");
    expect(broker).toContain("pub async fn collect_web_evidence");
  });

  it("keeps evidence detail focused on evidence and conflicts", () => {
    const detail = read("src/components/ai/EvidenceDetailArtifact.tsx");
    const drawer = read("src/components/ai/ContextPacketDrawer.tsx");

    expect(detail).toContain("liveExcerpt");
    expect(detail).toContain("conflictGroup");
    expect(detail).toContain("conflictNote");
    expect(detail).not.toContain("Provider");
    expect(detail).not.toContain("extractionMethod");
    expect(detail).not.toContain("external_metadata_only");
    expect(detail).not.toContain("page body and excerpt were not saved");
    expect(drawer).toContain("liveExcerpt: packet.excerpt");
    expect(drawer).toContain("conflictGroup: packet.web?.conflict_group");
    expect(drawer).toContain("conflictNote: packet.web?.conflict_note");
  });

  it("resolves MCP only through explicit web evidence provider mappings", () => {
    const resolver = read("src-tauri/src/ai_runtime/capability_resolver.rs");
    const registry = read("src-tauri/src/ai_runtime/mcp_runtime_registry.rs");
    const hostRuntime = read("src-tauri/src/ai_runtime/mcp_host_runtime.rs");
    const dispatchTests = read(
      "src-tauri/src/ai_runtime/tool_dispatch/tests.rs",
    );

    expect(resolver).toContain("list_enabled_web_provider_mappings");
    expect(resolver).toContain('"web.search" | "web.fetch"');
    expect(resolver).not.toContain("list_runtime_profiles");
    expect(resolver).not.toContain("list_tool_inventory");
    expect(resolver).not.toContain(
      '"web.to_markdown" | "web.download_to_assets"',
    );
    expect(resolver).not.toContain(
      '"secret.use_named" | "process.run_readonly"',
    );

    for (const legacy of [
      "McpServerCatalogInput",
      "McpRuntimeProfileInput",
      "McpRuntimeProfileSummary",
      "McpToolInventoryInput",
      "McpToolInventorySummary",
      "McpHealthEventInput",
      "McpHealthEventSummary",
      "SkillRuntimeRequirementInput",
      "SkillRuntimeReadiness",
      "upsert_server_catalog",
      "upsert_runtime_profile",
      "set_runtime_profile_enabled",
      "delete_runtime_profile",
      "list_runtime_profiles",
      "record_tool_inventory",
      "list_tool_inventory",
      "record_health_event",
      "list_recent_health_events",
      "upsert_skill_runtime_requirement",
      "resolve_skill_runtime",
    ]) {
      expect(registry).not.toContain(legacy);
    }

    expect(dispatchTests).not.toContain("#[cfg(any())]");
    expect(dispatchTests).not.toContain("mcp_runtime_profile_upsert");
    expect(dispatchTests).not.toContain("mcp_runtime_capability_call");

    for (const rawTransportSurface of [
      "pub async fn discover_http_tools(",
      "pub async fn call_http_tool_with_sender(",
      "pub async fn call_http_tool(",
      "pub async fn discover_stdio_tools(",
    ]) {
      expect(hostRuntime).not.toContain(rawTransportSurface);
    }
    expect(hostRuntime).toContain("pub async fn call_provider_tool(");
  });

  it("does not expose generic web or process tools to the agent catalog", () => {
    const catalogBoundary = read(
      "src-tauri/src/ai_runtime/tool_catalog/boundary.rs",
    );
    const dispatch = read("src-tauri/src/ai_runtime/tool_dispatch_impl.rs");
    const boundaryDispatch = read(
      "src-tauri/src/ai_runtime/tool_dispatch/boundary.rs",
    );
    const harnessRun = read("src-tauri/src/ai_harness/harness/run.rs");
    const harnessTools = read("src-tauri/src/ai_harness/harness/tools.rs");

    for (const legacy of [
      "web_to_markdown",
      "web_download_to_assets",
      "web_citation_extract",
      "process_run_readonly",
      "process_run_network",
      "process_run_mutating",
      "process_long_running",
      "process_kill_owned",
    ]) {
      expect(catalogBoundary).not.toContain(`"${legacy}"`);
      expect(dispatch).not.toContain(`"${legacy}" =>`);
      expect(boundaryDispatch).not.toContain(`${legacy}_tool`);
      expect(harnessRun).not.toContain(`"${legacy}"`);
      expect(harnessTools).not.toContain(`"${legacy}"`);
    }

    for (const legacy of [
      "fetch_web_page",
      "readability_fetch",
      "web_fetch_batch",
      "rendered_fetch",
      "mcp_runtime_capability_call",
    ]) {
      expect(dispatch).not.toContain(`"${legacy}" =>`);
      expect(harnessRun).not.toContain(`"${legacy}"`);
      expect(harnessTools).not.toContain(`"${legacy}"`);
    }
  });
});
