# Iris AI Reign-In Implementation Plan

> **状态：已被取代（superseded）**。本计划已被 [`2026-07-01-iris-ai-harness-architecture.md`](./2026-07-01-iris-ai-harness-architecture.md) 取代，对应 spec [`2026-07-01-iris-reign-in-design.md`](../specs/2026-07-01-iris-reign-in-design.md) 已标注 superseded。下方内容仅保留作为历史与上下文，请勿据此规划新工作；联网证据目标态以新计划为准（MCP -> DDG，LLM vendor 退回普通 LLM provider）。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 根据 `docs/superpowers/specs/2026-07-01-iris-reign-in-design.md` 一次性完成 Iris AI 能力收口：Skills 回到自产 prompt-only，MCP 只作为联网 Provider，`WebEvidenceBroker` 成为唯一联网语义层，联网证据复用现有证据包与临时 tab。

**Architecture:** 后端以目标态契约测试先锁住删除边界，再删除旧 Skills/MCP 平台模型和 agent 可见通用工具；MCP runtime 只保留 transport 调用能力并由 broker 内部通过显式 `web.search` / `web.fetch` 映射使用。前端不新增联网证据范式：管理中心只承载 provider 配置与诊断，普通 AI 面板继续通过现有证据包和 `EvidenceDetailArtifact` 临时 tab 展示证据与冲突。

**Tech Stack:** Rust/Tauri 2、SQLite migrations、serde/serde_json、tokio、reqwest/rustls、React 19、TypeScript IPC DTO、Vitest、Rust unit/integration tests。

---

## 执行纪律

- [x] 不创建 worktree，除非用户明确允许。
- [x] 不新增依赖；若必须新增，先停止并补 AGPL-3.0 兼容性、替代方案和用户确认。
- [x] 每个行为变更先写目标态失败测试，再实现，再验证。
- [x] 旧能力被目标态覆盖后同轮删除；不保留 compatibility wrapper、隐藏入口或灰色平台壳。
- [x] 不新增 `skill_write_scopes`、`web_evidence_ledger`、`web_provider_health` 或独立 evidence 页面。
- [x] 不把 API key、token、用户笔记内容、完整 query、完整 URL、完整网页正文、cookie、password 写入日志、SQLite 审计、prompt 或确认 UI。
- [x] 不提交 git commit，除非用户明确要求。

## 目标不变量

```text
Iris is not a general Agent/plugin platform.
New skills are prompt-only SKILL.md files created by Iris and confirmed by the user.
SKILL.md scope is the scope fact source; DB only stores confirmed hash and enable/index state.
Changed skill hash disables the skill until reconfirmed.
Skill read context and PatchProposal write target use the same scope gate.
No URL/Git/Registry/SkillHub/local import path remains.
MCP tools are metadata until explicitly mapped to web.search/web.fetch.
Agent never sees arbitrary MCP tools.
Agent sees only one network tool: web_search.
fetch_web_page/readability_fetch/web_fetch_batch/rendered_fetch are not model-visible, dispatchable, confirmable, or policy-exposed.
Network disabled means zero native/MCP/model-provider outbound calls.
Provider results are merged, not raced-and-dropped.
Provider conflicts are marked, not adjudicated.
Evidence and conflicts reuse the existing AI evidence package and EvidenceDetailArtifact temporary tab.
Audit/cache never store full query, full URL, full page text, note content, or secrets.
```

## File Map

### Rust: Skills

- Modify: `src-tauri/src/ai_runtime/skills/manifest.rs` - prompt-only manifest parsing and deprecated-kind rejection.
- Modify: `src-tauri/src/ai_runtime/skills/frontmatter.rs` - scope frontmatter parsing.
- Modify: `src-tauri/src/ai_runtime/skills/model.rs` - prompt-only DTO, `scope_rules`, `content_hash`, `confirmed_hash`, `confirmation_status`.
- Modify: `src-tauri/src/ai_runtime/skills/scan.rs` - read `.iris/skills/*.md`, compute content/scope hash, mark changed skills as needing confirmation.
- Modify: `src-tauri/src/ai_runtime/skills/activation.rs` - inject only confirmed prompt-only skills and carry confirmed scope to runtime.
- Modify: `src-tauri/src/ai_runtime/skills_impl.rs` - remove external install/workspace/runtime APIs from public behavior; keep only list and controlled create/confirm services.
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/note.rs` and `src-tauri/src/ai_runtime/tool_dispatch/markdown.rs` - enforce skill scope before read/context/write proposals.

### Rust: MCP Provider and Broker

- Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs` - replace old server/profile/inventory/health model with minimal `web_evidence_providers` accessors.
- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs` - keep stdio/HTTPS transport and expose internal mapped `web.search` / `web.fetch` calls only to broker.
- Modify: `src-tauri/src/ai_runtime/capability_resolver.rs` - support only `web.search` / `web.fetch`, read explicit provider mappings, never read old inventory tables.
- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs` - provider selection, top-2 merge, URL deep read, conflict marking, packet conversion, provider metadata.
- Modify: `src-tauri/src/llm/search_web.rs` - native search cache isolation and no prompt-prefix injection.
- Modify: `src-tauri/src/llm/fetch_web_page.rs` - internal native fetch provider only, with provider/cache metadata.
- Modify: `src-tauri/src/ai_runtime/tool_audit.rs` - hash query/url, keep provider metadata, never store raw page/query/url.

### Rust: Tool Surface and Workflows

- Modify: `src-tauri/src/ai_runtime/tool_catalog/web.rs` - expose only `web_search`.
- Modify: `src-tauri/src/ai_runtime/tool_catalog/skills.rs` - expose only target-state skill awareness tools.
- Modify: `src-tauri/src/ai_runtime/tool_dispatch_impl.rs`, `src-tauri/src/ai_runtime/tool_dispatch/web.rs`, `src-tauri/src/ai_runtime/tool_dispatch/skills.rs` - delete legacy web/process/MCP/skill-management dispatch arms.
- Modify: `src-tauri/src/ai_runtime/tool_policy.rs`, `src-tauri/src/ai_runtime/agent_permissions.rs`, `src-tauri/src/ai_runtime/sandbox_profile.rs` - delete old tool permission routes and stale meta-skill bypasses.
- Modify: `src-tauri/src/commands/writing_commands.rs`, `src-tauri/src/commands/document_commands.rs`, `src-tauri/src/commands/citation_commands.rs`, `src-tauri/src/ai_workflows/research_workflow.rs` - collect web evidence through broker only.
- Modify: `src-tauri/src/llm/engine.rs` and `src-tauri/src/llm/mod.rs` - remove legacy model web-search prompt injection surface.

### Data and IPC

- Create/Modify: `src-tauri/migrations/042_reign_in_ai_capabilities.sql` and `.down.sql` - delete old platform tables, add minimal provider table and cache isolation columns.
- Modify: `src-tauri/src/storage/migrate.rs` - register 042 and make cache-column migration idempotent.
- Modify: `src-tauri/src/commands/ai_commands.rs`, `src-tauri/src/lib.rs`, `src/types/ipc.ts`, `src/types/ai.ts`, `src/lib/ipc.ts` - remove external skill/MCP platform IPC and add controlled skill/provider IPC.

### Frontend

- Modify: `src/components/ai/SkillsPanel.tsx`, `src/components/ai/skills/SkillCard.tsx`, `src/components/ai/skills/SkillStatusBadges.tsx` - remove external install/edit/runtime/MCP UI; show prompt-only skill confirmation and scope.
- Modify: `src/components/settings/ManagementCenterPanel.tsx`, `src/components/settings/RemovedVendorSearchSection.tsx`, `src/components/ai/skills/McpProfilesPanel.tsx`, `src/components/ai/skills/McpProfileCard.tsx` - present `AI -> 联网与证据` as provider configuration/diagnostics inside existing management center components; do not create a new evidence page.
- Modify: `src/components/ai/EvidenceDetailArtifact.tsx`, `src-tauri/src/ai_runtime/session_evidence.rs`, `src/types/ipc.ts` - evidence detail temporary tab shows only evidence, excerpt, citation and conflict; no provider process流水.
- Modify: `src/components/ai/ToolConfirmDialog.tsx`, `src/lib/tool-display-names.ts`, `src/lib/assistant-routing.ts`, `src/lib/assistant-taskplan.ts`, `src/lib/skill-install-notice.ts` - delete old tool/SkillHub UI paths.

### Docs

- Modify: `ROADMAP.md`, `ARCHITECTURE.md`, `docs/ipc-api-reference.md`, `docs/README.md`, `docs/design-system.md` - sync target state.
- Modify: superseded specs only to add "superseded" notice if needed; do not use them as implementation targets.

## Task 1: Target-State Contract Tests

**Files:**

- Add/Modify: `tests/reign-in-target-state.test.ts`
- Modify: `src-tauri/tests/agent_permission_boundaries.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/tests.rs`

- [x] **Step 1: Add negative surface tests**

Add or update a Vitest contract that reads source files and fails while old surfaces remain:

```ts
const removedToolNames = [
  "fetch_web_page",
  "readability_fetch",
  "web_fetch_batch",
  "rendered_fetch",
  "web_to_markdown",
  "web_download_to_assets",
  "web_citation_extract",
  "process_run_readonly",
  "process_run_network",
  "process_run_mutating",
  "process_long_running",
  "process_kill_owned",
  "mcp_runtime_capability_call",
  "mcp_runtime_profile_upsert",
  "mcp_runtime_tools_list",
  "mcp_runtime_health_check",
  "skills_install",
  "skills_prepare_workspace",
  "skills_migrate_legacy",
];

for (const name of removedToolNames) {
  expect(runtimeSources).not.toContain(`name: "${name}"`);
  expect(runtimeSources).not.toContain(`"${name}" =>`);
  expect(frontendSources).not.toContain(`case "${name}"`);
}
```

- [x] **Step 2: Add broker-only workflow tests**

Assert writing/document/citation commands call `collect_web_evidence` and do not call `fetch_search_context_for_db` or `web_packets_from_fetch`.

- [x] **Step 3: Add provider-only resolver tests**

In Rust, assert `capability_resolver.rs` supports only `web.search` and `web.fetch`, and source contracts forbid `list_runtime_profiles` / `list_tool_inventory` in the resolver.

- [x] **Step 4: Run RED**

Run:

```bash
npm run test -- tests/reign-in-target-state.test.ts
cargo test --manifest-path src-tauri/Cargo.toml agent_permission_boundaries --test agent_permission_boundaries
cargo test --manifest-path src-tauri/Cargo.toml tool_catalog --lib
```

Expected before implementation: failures identify remaining old tool, resolver, UI, or workflow paths.

Current evidence: `tests/reign-in-target-state.test.ts` now guards old Skill install/service modules, old public Skill runtime functions, stale Skill persona/task vocabulary, broker-only workflow paths, provider-only resolver mappings, evidence-detail scope, deleted generic web/process tools, and absence of disabled `#[cfg(any())]` MCP platform dispatch tests. Latest run: `npm run test -- tests/reign-in-target-state.test.ts` passed.

## Task 2: Collapse Skills to Confirmed Prompt-Only

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/manifest.rs`
- Modify: `src-tauri/src/ai_runtime/skills/frontmatter.rs`
- Modify: `src-tauri/src/ai_runtime/skills/model.rs`
- Modify: `src-tauri/src/ai_runtime/skills/scan.rs`
- Modify: `src-tauri/src/ai_runtime/skills/activation.rs`
- Modify: `src-tauri/src/ai_runtime/skills_impl.rs`
- Modify: `src/types/ai.ts`
- Test: `src-tauri/src/ai_runtime/skills/status_tests.rs`
- Test: `tests/phase4-skills-closed-loop.test.ts`

- [x] **Step 1: Write RED tests**

Add Rust tests for:

```rust
#[test]
fn rejects_complex_skill_kinds() {
    for kind in ["resource", "workspace", "mcp_dependent", "hybrid"] {
        let err = parse_skill_manifest_for_test(kind).unwrap_err().to_string();
        assert!(err.contains("prompt_only"), "{err}");
    }
}

#[test]
fn changed_skill_hash_requires_reconfirmation() {
    let entry = skill_entry_for_test("old-confirmed-hash", "new-content-hash");
    assert_eq!(entry.confirmation_status, SkillConfirmationStatus::NeedsConfirmation);
}
```

Add Vitest source contracts that `SkillsPanel.tsx` does not import `skillsRead`, `skillsWrite`, `skillsToggle`, `skillsUninstall` or render URL/Git/Registry/textarea editing UI.

- [x] **Step 2: Implement prompt-only DTOs and scan behavior**

Keep `SKILL.md` as the prompt/scope fact source. Database state may only record enable/index state and confirmed hash. If `content_hash != confirmed_hash`, mark `NeedsConfirmation` and skip activation.

- [x] **Step 3: Remove external install/runtime UI and IPC**

Delete public external install/workspace/runtime paths. If a legacy row/file is encountered, list it only as unsupported or ignore it safely; never activate it.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml skills --lib
npm run test -- tests/phase4-skills-closed-loop.test.ts tests/skills-settings-permissions.test.ts
```

Expected: prompt-only skills pass; complex kind/runtime/workspace/MCP skill paths are unreachable.

Current evidence: `skills_impl.rs` exposes only list/create-confirm style production services for Skills; URL/Git/local install, resource/workspace runtime modules and old toggle/uninstall/read/write IPC paths are removed from production surfaces. `write_confirmed_skill_content` rejects targets outside `.iris/skills` before creating parent directories, records confirmed hash, and changed hashes require reconfirmation. Latest runs passed: `cargo test --manifest-path src-tauri/Cargo.toml skills --lib`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `npm run test -- tests/skills-settings-permissions.test.ts tests/phase4-skills-closed-loop.test.ts tests/ipc-boundary.test.ts`.

## Task 3: Enforce One Skill Scope Gate for Read, Context, and PatchProposal

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/activation.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/note.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/markdown.rs`
- Modify: `src-tauri/src/ai_harness/harness/run.rs`
- Test: Rust tests in the touched modules.

- [x] **Step 1: Write RED tests**

Test path/glob/tag scope behavior:

```rust
#[test]
fn skill_scope_allows_matching_glob() {
    assert!(skill_scope_allows_path(&[rule("glob", "Daily/*.md")], "Daily/2026-07-01.md"));
}

#[test]
fn skill_scope_rejects_out_of_scope_patch_target_before_confirmation() {
    let err = validate_skill_scoped_patch(&[rule("glob", "Daily/*.md")], "Projects/x.md")
        .unwrap_err()
        .to_string();
    assert!(err.contains("outside the confirmed Skill scope"), "{err}");
}
```

- [x] **Step 2: Implement one shared scope helper**

Implement `skill_scope_allows_path(scope_rules, note_path, tag_index)` once and reuse it for note read/context selection and PatchProposal target validation. Path and glob rules are path-based; tag rules resolve through the vault tag index before allowing access.

- [x] **Step 3: Gate before user confirmation**

Reject out-of-scope PatchProposal targets before they enter confirmation UI. Skill activation still needs no separate confirmation; note mutation still requires normal user confirmation.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml skill_scope --lib
cargo test --manifest-path src-tauri/Cargo.toml --test ai_agent_phase2_contracts
```

Expected: read/context/write all share the same scope decision.

Current evidence: `ToolDispatchContext::ensure_active_skill_scope_allows_path` is reused by note read/context tools and markdown patch application; tag scope resolves through the vault tag index. Latest runs passed: `cargo test --manifest-path src-tauri/Cargo.toml skill_scope --lib`, `cargo test --manifest-path src-tauri/Cargo.toml --test ai_agent_phase2_contracts`, and `cargo test --manifest-path src-tauri/Cargo.toml tool_dispatch --lib`.

## Task 4: Replace MCP Platform Registry With Web Provider Model

**Files:**

- Modify: `src-tauri/migrations/042_reign_in_ai_capabilities.sql`
- Modify: `src-tauri/migrations/042_reign_in_ai_capabilities.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Test: Rust migration/registry tests and `tests/ipc-boundary.test.ts`.

- [x] **Step 1: Write RED tests**

Assert the target schema has `web_evidence_providers`, has no active old `mcp_*` platform tables as data sources, and IPC no longer exposes `mcp_runtime_*` management/capability commands.

- [x] **Step 2: Apply minimal provider schema**

Use one provider table containing id/name/enabled, transport kind/config reference, credential refs, explicit `web.search` / `web.fetch` mappings, and provider_config_hash. Do not persist tool inventory or health event tables.

- [x] **Step 3: Implement provider accessors**

Provide:

```rust
list_web_evidence_providers(db)
list_enabled_web_providers(db)
upsert_web_evidence_provider(db, input)
toggle_web_evidence_provider(db, provider_id, enabled)
delete_web_evidence_provider(db, provider_id)
```

Validation rejects plaintext secrets in JSON config and requires explicit mapping tool names.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml mcp_runtime_registry --lib
cargo test --manifest-path src-tauri/Cargo.toml migrate --lib
npm run test -- tests/ipc-boundary.test.ts
```

Expected: old MCP platform registry is not a target-state data source; provider IPC exists and stores no secrets.

Current evidence: `mcp_runtime_registry` now stores minimal `web_evidence_providers` mappings and rejects raw secret material; migration tests assert old MCP platform tables are not target-state data sources. Latest runs passed: `cargo test --manifest-path src-tauri/Cargo.toml mcp_runtime_registry --lib`, `cargo test --manifest-path src-tauri/Cargo.toml migrate --lib`, `npm run test -- tests/reign-in-target-state.test.ts tests/ipc-boundary.test.ts`.

## Task 5: Restrict Capability Resolver and MCP Host Runtime

**Files:**

- Modify: `src-tauri/src/ai_runtime/capability_resolver.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
- Test: Rust tests in both modules.

- [x] **Step 1: Write RED tests**

Assert:

```rust
assert!(is_supported_capability_for_test("web.search"));
assert!(is_supported_capability_for_test("web.fetch"));
for capability in ["web.to_markdown", "web.download_to_assets", "skill.mcp_bridge", "secret.use_named", "process.run_readonly"] {
    assert!(!is_supported_capability_for_test(capability));
}
```

- [x] **Step 2: Resolve only provider mappings**

`resolve_required_capability` reads `list_enabled_web_providers(db)` and finds explicit `web.search` / `web.fetch` mappings. It must not read runtime profiles, tool inventory or server catalog.

- [x] **Step 3: Expose only internal mapped calls**

`mcp_host_runtime` keeps stdio/HTTPS transport, but only broker-facing helpers can call mapped `web.search` / `web.fetch`. Do not expose agent-callable MCP tool execution.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml capability_resolver --lib
cargo test --manifest-path src-tauri/Cargo.toml mcp_host_runtime --lib
```

Expected: MCP is usable only as explicitly mapped web provider backend.

Current evidence: `capability_resolver` supports only `web.search` / `web.fetch` mappings from enabled providers; `mcp_host_runtime` no longer publicly exposes raw HTTP/stdio transport call/discovery functions and has URL policy tests for HTTPS, secret material, private hosts and localhost dev mode. Latest runs passed: `cargo test --manifest-path src-tauri/Cargo.toml capability_resolver --lib`, `cargo test --manifest-path src-tauri/Cargo.toml mcp_host_runtime --lib`, `npm run test -- tests/reign-in-target-state.test.ts tests/ipc-boundary.test.ts`, and `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.

## Task 6: Complete WebEvidenceBroker as the Only Network Semantics Layer

**Files:**

- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Modify: `src-tauri/src/llm/search_web.rs`
- Modify: `src-tauri/src/llm/fetch_web_page.rs`
- Modify: `src-tauri/src/ai_runtime/tool_audit.rs`
- Modify: `src-tauri/src/ai_harness/evidence_mixer.rs`
- Test: Rust broker/cache/audit tests and `tests/reign-in-target-state.test.ts`.

- [x] **Step 1: Write RED tests**

Test disabled gate, top-2 merge, canonical URL dedup with provider metadata, conflict marking, URL deep read, and sanitized audit:

```rust
#[test]
fn audit_summary_hashes_query_and_url() {
    let summary = summarize_web_tool_for_audit("web_search", json!({
        "query": "private full query",
        "urls": ["https://example.com/private?a=1"]
    }));
    assert!(summary.contains("query_hash"));
    assert!(summary.contains("url_hash"));
    assert!(!summary.contains("private full query"));
    assert!(!summary.contains("https://example.com/private"));
}
```

- [x] **Step 2: Implement provider planning and dispatch**

Provider list is native LLM vendor/DDG plus enabled MCP providers with explicit mappings. Choose top-2 by fixed priority `MCP > LLM vendor > DDG`, run concurrently, merge successful results, and record failures as diagnostics only.

- [x] **Step 3: Implement fetch and URL deep read**

Search enriches top-K URLs. Explicit user URLs enter broker through `web_search` arguments and use native/MCP fetch providers; no standalone fetch agent tools remain.

- [x] **Step 4: Enforce cache/audit privacy**

Cache keys and queries include `vault_id + provider_id/kind + provider_config_hash + broker_version`. Audit stores hashes and provider metadata only.

- [x] **Step 5: Verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml web_evidence_broker --lib
cargo test --manifest-path src-tauri/Cargo.toml tool_audit --lib
npm run test -- tests/reign-in-target-state.test.ts
```

Expected: all network paths use broker semantics and no raw query/url/body leaks through audit.

Current evidence: `web_evidence_broker --lib` covers disabled gate, provider top-2 priority, query sanitization, URL deep read, MCP/native fetch provider selection, in-memory circuit breaker blocking, successful fetch merge, canonical URL dedup and conflict marking; `search_web --lib` and `fetch_web_page --lib` cover cache scope isolation and LRU pruning; `tool_audit --lib` covers sanitized query hashing; `tests/reign-in-target-state.test.ts` and `tests/web-evidence-broker.test.ts` cover source-level broker-only network contracts.

## Task 7: Delete Legacy Agent Tools, Permissions, Policies, and Prompts

**Files:**

- Modify: `src-tauri/src/ai_runtime/tool_catalog/web.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/skills.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch_impl.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/web.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/skills.rs`
- Modify: `src-tauri/src/ai_runtime/tool_policy.rs`
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
- Modify: `src-tauri/src/ai_runtime/sandbox_profile.rs`
- Modify: `src-tauri/src/ai_runtime/persona_resolver.rs`
- Modify: `src/components/ai/ToolConfirmDialog.tsx`
- Modify: `src/lib/tool-display-names.ts`
- Test: target-state Rust/Vitest tests.

- [x] **Step 1: Write RED search contract**

Assert old names are absent from catalog, dispatch, permission profiles, policy allowlists, confirmation UI, model prompt text and display-name maps.

- [x] **Step 2: Keep only target tools**

Agent network tool surface contains only `web_search`. Agent skills surface contains only non-platform skill awareness. Remove `skills_toggle`, `skills_uninstall`, `skills_write`, `skills_read`, MCP management and process/browser/network generic tools from model-visible paths.

- [x] **Step 3: Verify GREEN**

Run:

```bash
rg -n "fetch_web_page|readability_fetch|web_fetch_batch|rendered_fetch|web_to_markdown|web_download_to_assets|web_citation_extract|process_run_readonly|process_run_network|process_run_mutating|process_long_running|process_kill_owned|mcp_runtime_capability_call|skills_install|skills_toggle|skills_uninstall" src-tauri/src src src-tauri/tests tests
cargo test --manifest-path src-tauri/Cargo.toml agent_permission_boundaries --test agent_permission_boundaries
npm run test -- tests/tool-confirm-dialog.test.tsx tests/phase5-permission-ui.test.ts
```

Expected: runtime/UI hits are gone; remaining hits are only negative tests or historical/superseded docs.

Current evidence: old Skills/MCP/process/web platform names are constrained to negative tests, internal native provider implementation, superseded historical documents, or explicit removal notes in the checked source set; disabled positive MCP platform dispatch tests were deleted. Latest runs passed: `cargo test --manifest-path src-tauri/Cargo.toml tool_dispatch --lib`, `cargo test --manifest-path src-tauri/Cargo.toml persona_resolver --lib`, `cargo test --manifest-path src-tauri/Cargo.toml agent_task_policy --lib`, `cargo test --manifest-path src-tauri/Cargo.toml --test agent_permission_boundaries`, `npm run test -- tests/reign-in-target-state.test.ts tests/tool-confirm-dialog.test.tsx tests/phase5-permission-ui.test.ts`, and `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.

## Task 8: Wire Workflows Through Broker and Remove Prompt Prefix Injection

**Files:**

- Modify: `src-tauri/src/commands/writing_commands.rs`
- Modify: `src-tauri/src/commands/document_commands.rs`
- Modify: `src-tauri/src/commands/citation_commands.rs`
- Modify: `src-tauri/src/ai_workflows/research_workflow.rs`
- Modify: `src-tauri/src/llm/engine.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Test: `tests/reign-in-target-state.test.ts` and relevant Rust workflow tests.

- [x] **Step 1: Write RED tests**

Assert command sources call `collect_web_evidence` and do not call `fetch_search_context_for_db`, `web_packets_from_fetch`, `apply_web_search` or `prepend_web_search_context`.

- [x] **Step 2: Replace inline web collection**

Each workflow builds `WebEvidenceBrokerInput` with query/urls/enabled/task/vault context, calls broker, converts items to packets, and mixes them with local evidence.

- [x] **Step 3: Remove legacy LLM web_search flag semantics**

Remove prompt-prefix injection. If the old DTO field must remain temporarily for IPC compatibility, it is ignored with a warning and documented as deprecated until TS callers are migrated.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml web_evidence_broker --lib
cargo check --manifest-path src-tauri/Cargo.toml
npm run test -- tests/reign-in-target-state.test.ts
```

Expected: workflows produce web evidence only through broker.

Current evidence: writing/document/citation workflow sources call `collect_web_evidence`; legacy inline fetch and prompt-prefix web search identifiers are absent from runtime sources and remain only in negative source-contract assertions. Latest runs passed: `cargo test --manifest-path src-tauri/Cargo.toml web_evidence_broker --lib`, `cargo check --manifest-path src-tauri/Cargo.toml`, and `npm run test -- tests/reign-in-target-state.test.ts`.

## Task 9: Reuse Existing Evidence Package and Temporary Tab

**Files:**

- Modify: `src-tauri/src/ai_runtime/session_evidence.rs`
- Modify: `src/components/ai/EvidenceDetailArtifact.tsx`
- Modify: `src/components/ai/UnifiedAssistantPanel.tsx` if detail payload wiring needs metadata.
- Modify: `src/types/ipc.ts`
- Test: `tests/evidence-detail-artifact.test.tsx` or existing evidence detail tests.

- [x] **Step 1: Write RED tests**

Render `EvidenceDetailArtifactView` with a web evidence record containing `liveExcerpt`, `canonicalUrl`, `conflictGroup`, `conflictNote`, `providerId`, and `extractionMethod`. Assert:

```ts
expect(screen.getByText(/来源冲突|Conflict/)).toBeInTheDocument();
expect(screen.getByText(/short excerpt/)).toBeInTheDocument();
expect(
  screen.queryByText(
    /extractionMethod|cache hit|fetch backend|provider ranking/i,
  ),
).not.toBeInTheDocument();
expect(screen.queryByText("mcp.primary")).not.toBeInTheDocument();
```

- [x] **Step 2: Store/display only evidence detail fields**

Session evidence may include source title, safe URL/domain, citation label, short excerpt, trust level, retrieval reason if user-facing, and conflict metadata. Do not display provider process metadata in the temporary tab.

- [x] **Step 3: Verify GREEN**

Run:

```bash
npm run test -- tests/evidence-detail-artifact.test.tsx tests/tool-confirm-dialog.test.tsx
```

Expected: evidence and conflicts appear in the existing temporary tab; no new evidence page or provider process UI exists.

Current evidence: `tests/evidence-detail-artifact.test.tsx` covers web evidence records with `liveExcerpt`, safe source/citation fields and conflict metadata, while asserting provider process metadata such as provider ids, extraction method, cache/fetch backend details and ranking are not shown. `EvidenceDetailArtifact` renders the existing evidence temporary tab only; no new evidence page or provider process UI was added. Latest run passed: `npm run test -- tests/evidence-detail-artifact.test.tsx tests/tool-confirm-dialog.test.tsx`.

## Task 10: Frontend Settings and IPC Cleanup

**Files:**

- Modify: `src/lib/ipc.ts`
- Modify: `src/types/ipc.ts`
- Modify: `src/types/ai.ts`
- Modify: `src/components/settings/ManagementCenterPanel.tsx`
- Modify: `src/components/settings/RemovedVendorSearchSection.tsx`
- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `src/components/ai/skills/McpProfileCard.tsx`
- Modify: `src/components/ai/SkillsPanel.tsx`
- Test: `tests/ipc-boundary.test.ts`, `tests/management-center-contract.test.ts`, `tests/skills-settings-permissions.test.ts`.

- [x] **Step 1: Write RED tests**

Assert `src/lib/ipc.ts` does not export old external skill/MCP platform calls and does export target provider/skill-confirm calls. Assert management center labels are `AI -> Skills` and `AI -> 联网与证据`, not "install tools for AI".

- [x] **Step 2: Update existing components only**

Use existing management center and MCP profile component files as provider configuration/diagnostic surfaces. Do not create a standalone Web Evidence page or a new repeated component unless an existing file cannot reasonably hold the UI.

- [x] **Step 3: Verify GREEN**

Run:

```bash
npm run test -- tests/ipc-boundary.test.ts tests/management-center-contract.test.ts tests/skills-settings-permissions.test.ts
npm run typecheck
```

Expected: frontend type surface matches Rust IPC and no external-install platform UI remains.

Current evidence: existing management center/provider component files carry `AI -> Skills` and `AI -> 联网与证据` provider configuration/diagnostic surfaces; IPC exports target provider and skill create/confirm calls without old external Skill/MCP platform calls. Latest runs passed: `npm run test -- tests/management-center-contract.test.ts tests/skills-settings-permissions.test.ts tests/ipc-boundary.test.ts` and `npm run typecheck`.

## Task 11: Documentation Sync

**Files:**

- Modify: `ROADMAP.md`
- Modify: `ARCHITECTURE.md`
- Modify: `docs/ipc-api-reference.md`
- Modify: `docs/README.md`
- Modify: `docs/design-system.md`
- Modify: superseded historical specs only for superseded notice.
- Test: `tests/reign-in-target-state.test.ts` or docs source contracts.

- [x] **Step 1: Mark old specs as superseded**

Add this at the top of historical specs:

```markdown
> Superseded by [2026-07-01-iris-reign-in-design.md](./2026-07-01-iris-reign-in-design.md).
> This file is retained for historical context only and must not be used as the implementation target.
```

- [x] **Step 2: Update ROADMAP and architecture**

Replace external skill install/MCP tool platform language with prompt-only Skills, MCP Provider, broker-only network and existing evidence package reuse.

- [x] **Step 3: Verify docs**

Run:

```bash
rg -n "SkillHub|skills_install|fetch_web_page|readability_fetch|web_fetch_batch|rendered_fetch|mcp_runtime_capability_call|URL / Git / 本地 / 拖拽|联网摘要|web_evidence_ledger|web_provider_health" ROADMAP.md ARCHITECTURE.md docs src tests
```

Expected: old terms appear only in superseded historical context, negative tests, migration rollback, or explicit removal notes.

Current evidence: the 2026-06-21 network lifecycle spec and 2026-06-30 Skills/MCP runtime spec are marked superseded by the 2026-07-01 reign-in design; ROADMAP now describes Iris-generated confirmed prompt-only Skills and broker-only web evidence; architecture and design-system docs no longer describe external Skill installation or agent-visible fetch tools as active behavior. Latest runs passed: `npm run format:check` and the docs source search above, with remaining hits limited to target spec removal requirements, superseded/historical context, explicit removal notes, plans, audits, or negative tests.

## Task 12: Final Verification and Completion Audit

**Files:** no planned new files. Fix failures in the owning task files.

- [x] **Step 1: Rust quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- [x] **Step 2: Frontend quality gates**

Run:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
```

- [x] **Step 3: Final target-state search**

Run:

```bash
rg -n "fetch_web_page|readability_fetch|web_fetch_batch|rendered_fetch|web_to_markdown|web_download_to_assets|web_citation_extract|process_run_readonly|process_run_network|process_run_mutating|process_long_running|process_kill_owned|skills_install|skills_toggle|skills_uninstall|SkillHub|mcp_runtime_capability_call|mcp_runtime_profile_upsert|mcp_runtime_tools_list|skill\\.mcp_bridge|secret\\.use_named|web_evidence_ledger|web_provider_health|skill_write_scopes|联网摘要|Web Evidence 工作台" src src-tauri tests ROADMAP.md ARCHITECTURE.md docs
```

Expected: no runtime/UI positive surface remains. Remaining hits must be negative tests, migration rollback, or superseded historical documents.

- [x] **Step 4: Requirement-by-requirement audit**

For every section of `docs/superpowers/specs/2026-07-01-iris-reign-in-design.md`, record current evidence:

```text
A1-A5 Skills: source inspection + Rust tests + Vitest UI/IPCs + rg
B1-B4 MCP Provider: migration + registry/resolver tests + IPC/UI tests + rg
C1-C11 Network/Broker/Evidence UI: broker tests + command source tests + audit/cache tests + EvidenceDetailArtifact tests + rg
Docs/IPC/Migration: docs diff + migration tests + ipc-boundary tests
Quality: command outputs from Rust and frontend gates
```

Only after all evidence proves completion may this goal be marked complete.

Current evidence:

- Rust quality gates passed: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and `cargo test --manifest-path src-tauri/Cargo.toml` (887 lib tests passed, 2 ignored; integration and doc tests passed).
- Frontend quality gates passed: `npm run lint`, `npm run format:check`, `npm run typecheck`, and `npm run test` (274 test files, 1971 tests passed).
- Final target-state search passed with classification: remaining old-token hits are negative target-state tests/assertions, superseded or historical specs/plans/audits, explicit removal notes, migration rollback context, or internal native provider/cache implementation that is not model-visible, dispatchable, confirmable, or policy-exposed.
- A1-A5 Skills: confirmed prompt-only creation/list/confirm flows are covered by `skills_impl`, `skill_scope`, phase-4/permissions/ipc tests and source-contract rg; external URL/Git/local/SkillHub install, old toggle/uninstall/read/write IPC, external runtime manifests and out-of-scope confirmed writes are removed or rejected.
- B1-B4 MCP Provider: migration, provider registry/resolver, mapped host runtime and management-center IPC/UI tests cover provider-only operation; arbitrary MCP tools remain metadata until mapped to `web.search` / `web.fetch`, and raw transport helpers are not public broker bypasses.
- C1-C11 Network/Broker/Evidence UI: broker tests, command source contracts, audit/cache tests, workflow source contracts and `EvidenceDetailArtifact` tests cover single `web_search`, disabled-network zero outbound behavior, provider merge/conflict marking, cache isolation/LRU, no raw query/URL/body audit storage, and reuse of existing evidence package plus temporary detail tab.
- Docs/IPC/Migration: ROADMAP, architecture, design-system, IPC reference and superseded historical specs are synchronized to the target-state language; migrations and IPC boundary tests cover the new provider/skill-confirm surface.
