# Iris Skills 与 MCP Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建成完整的 Iris 受控 Skills 体系：简单 `SKILL.md` Skill 不依赖 MCP 也完整可用；复杂 Skill 通过 typed manifest、runtime status、MCP registry、capability resolver、权限确认和降级注入获得真实、可解释、可审计的能力边界。

**Architecture:** `SKILL.md` 是模型可读的行为入口，`iris.skill.toml` 是复杂能力的 typed contract。Skill 不直接启动 daemon、不直接消费任意 MCP tool；所有执行能力收敛到 Iris ToolCatalog、ToolPolicy、Capability Resolver、MCP/Provider Host Runtime、confirmation 和 audit 链路。SQLite 保存 runtime registry、tool inventory、health events 与状态快照；Skill 文件本身仍然是普通可读包。

**Tech Stack:** Rust/Tauri 2、serde/toml、SQLite migrations、tokio process/http runtime、reqwest/rustls、React 19、TypeScript IPC DTO、Vitest、Rust unit/integration tests。

---

## 0. 执行纪律

- [ ] 每个任务先写失败测试，再实现，再验证。
- [ ] 禁止使用 `apply_patch`。
- [ ] 禁止未经审批新建 worktree。
- [ ] 不新增依赖，除非先完成 AGPL-3.0 兼容性与替代方案说明。
- [ ] 不把 API key、token、用户笔记正文、raw MCP output 写入日志、SQLite、manifest、prompt 或确认 UI。
- [ ] 不伪造 `reasoning_content`，不为了通过 provider replay 写假推理。
- [ ] 不声称完成，除非对应 Rust/TS 测试、fmt、clippy/typecheck 已通过。

## 1. 文件与模块归属

### 后端核心

- Modify/Create: `src-tauri/src/ai_runtime/skills/manifest.rs` 负责 `iris.skill.toml` typed model、parser、strict validation、legacy fallback。
- Modify: `src-tauri/src/ai_runtime/skills/model.rs` 负责 Skill status DTO、runtime/workspace/capability summaries。
- Modify: `src-tauri/src/ai_runtime/skills/scan.rs` 负责扫描 `SKILL.md`、manifest、resources、workspace declaration。
- Modify: `src-tauri/src/ai_runtime/skills/activation.rs` 负责 section-level gate 与降级注入。
- Modify: `src-tauri/src/ai_runtime/skills/resources.rs` 负责 resource 读取与 required/optional 检查。
- Modify: `src-tauri/src/ai_runtime/skills/workspace.rs` 负责 workspace prepare、list、write 语义。
- Modify: `src-tauri/src/ai_runtime/skill_install_service.rs` 负责安装、启停、preflight、status snapshot。
- Create/Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs` 负责 SQLite-backed MCP registry、tool inventory、health events。
- Create/Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs` 负责 MCP stdio/http/sse runtime、tools/list、tools/call、health、timeout、caps、sanitized errors。
- Create/Modify: `src-tauri/src/ai_runtime/capability_resolver.rs` 负责 Skill/MCP/provider 到 Iris capability 的解析与阻塞原因。
- Modify: `src-tauri/src/ai_runtime/tool_catalog/skills.rs` 负责 Agent 可见 Skill/MCP 管理工具。
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/skills.rs` 负责 Agent 工具 dispatch。
- Modify: `src-tauri/src/ai_runtime/tool_policy.rs` 负责 meta tools、allowlist、confirmation gate。
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs` 负责权限原子和 tool permission profile。
- Modify: `src-tauri/src/ai_harness/harness_confirm.rs` 负责工具确认事务与 assistant resume 结果拆分。
- Modify: `src-tauri/src/ai_harness/harness/context.rs` 负责 activated skill prompt assembly。

### 数据与 IPC

- Create/Modify: `src-tauri/migrations/040_mcp_runtime_registry.sql`
- Create/Modify: `src-tauri/migrations/040_mcp_runtime_registry.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/ai.ts`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `docs/ipc-api-reference.md`

### 前端

- Modify: `src/components/ai/SkillsPanel.tsx`
- Create: `src/components/ai/skills/SkillCard.tsx`
- Create: `src/components/ai/skills/SkillStatusBadges.tsx`
- Create: `src/components/ai/skills/McpProfilesPanel.tsx`
- Create: `src/components/ai/skills/McpProfileCard.tsx`
- Modify: `src/components/ai/ToolConfirmDialog.tsx`
- Modify: `src/components/ai/hooks/useAssistantConfirmations.ts`

### 测试

- Modify: `tests/phase4-skills-closed-loop.test.ts`
- Modify: `tests/agent-task-capability-contract.test.ts`
- Modify: `tests/ipc-boundary.test.ts`
- Modify: `tests/skills-settings-permissions.test.ts`
- Modify: `tests/use-assistant-confirmations.test.tsx`
- Modify: `tests/tool-confirm-dialog.test.tsx`
- Add colocated Rust tests in changed Rust modules。

---

## 2. 系统不变量

这些不变量必须被测试锁住，任何后续实现不得破坏：

```text
SKILL.md only => legacy_prompt_only => runtime_kind not_applicable => no MCP warning
installed + enabled != runtime ready
validation valid != runtime ready
workspace files=[] != workspace missing
MCP inventory tool name != model-visible tool name
MCP annotation != capability grant
tool side effect succeeded + assistant resume failed => partial success
```

---

## 3. 任务清单

### Task 1: 锁定简单 Skill 不依赖 MCP 的契约

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/manifest.rs`
- Modify: `src-tauri/src/ai_runtime/skills/scan.rs`
- Modify: `src-tauri/src/ai_runtime/skills/model.rs`
- Test: colocated Rust tests in `manifest.rs` / `scan.rs`
- Test: `tests/phase4-skills-closed-loop.test.ts`

- [ ] **Step 1: 写失败测试，证明无 manifest 的 `SKILL.md` 是 prompt-only**

Add a Rust test equivalent to:

```rust
#[test]
fn legacy_skill_without_manifest_is_prompt_only_and_runtime_not_applicable() {
    let entry = scan_fixture_skill("simple-skill-with-only-skill-md").unwrap();
    assert_eq!(entry.validation, "legacy");
    assert_eq!(entry.kind, "legacy_prompt_only");
    assert_eq!(entry.runtime_kind, "not_applicable");
    assert!(entry.runtime_ready);
    assert!(!entry.workspace_declared);
    assert_eq!(entry.availability, "available");
    assert!(entry.mcp_dependencies.is_empty());
    assert!(entry.degraded_reasons.is_empty());
}
```

Run:

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml legacy_skill_without_manifest_is_prompt_only_and_runtime_not_applicable --lib
```

Expected before implementation: FAIL because fields are missing or runtime semantics are ambiguous.

- [ ] **Step 2: Implement legacy fallback**

Rules:

```text
no iris.skill.toml -> validation legacy
kind -> legacy_prompt_only
runtime_kind -> not_applicable
runtime_ready -> true
workspace_declared -> false
workspace_prepared -> false
availability -> available when installed/enabled/readable
mcp_dependencies -> []
```

- [ ] **Step 3: Add TS contract test**

In `tests/phase4-skills-closed-loop.test.ts`, assert that `skills_list` response for a simple Skill does not contain MCP/runtime warning text or `runtime_unavailable` reason.

Run:

```powershell
npm.cmd --prefix D:\Iris run test -- tests/phase4-skills-closed-loop.test.ts
```

- [ ] **Step 4: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml skills --lib
npm.cmd --prefix D:\Iris run typecheck
```

Acceptance:

- Installing a `SKILL.md` only Skill shows behavior-layer availability.
- No MCP/profile/runtime warning appears for prompt-only Skill.
- Agent can match and inject its behavior instructions.

### Task 2: 完成 Manifest v1 strict validation

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/manifest.rs`
- Modify: `src-tauri/src/ai_runtime/skills/model.rs`
- Test: colocated Rust tests in `manifest.rs`

- [ ] **Step 1: 写失败测试，MCP command 不能写在 Skill manifest 中**

```rust
#[test]
fn manifest_rejects_embedded_mcp_command() {
    let manifest = r#"
schema_version = "1"
name = "bad-search"
kind = "mcp_dependent"

[[mcp.dependencies]]
profile_id = "bad"
command = "npx"
required = true
"#;
    let err = parse_manifest_str(manifest).unwrap_err();
    assert!(err.to_string().contains("security-sensitive"));
}
```

- [ ] **Step 2: 写失败测试，metadata unknown warning 不阻断**

```rust
#[test]
fn manifest_allows_unknown_metadata_with_warning() {
    let result = parse_manifest_str(valid_prompt_only_with_extra_metadata()).unwrap();
    assert!(result.warnings.iter().any(|warning| warning.contains("metadata")));
}
```

- [ ] **Step 3: 写失败测试，raw secret marker 被拒绝且不回显 secret**

```rust
#[test]
fn manifest_rejects_raw_secret_markers_without_echoing_value() {
    let manifest = r#"
schema_version = "1"
name = "bad-secret"
kind = "mcp_dependent"

[[mcp.dependencies]]
profile_id = "search"
required = true
note = "token=sk-live-should-not-appear"
"#;
    let err = parse_manifest_str(manifest).unwrap_err().to_string();
    assert!(err.contains("secret"));
    assert!(!err.contains("sk-live-should-not-appear"));
}
```

- [ ] **Step 4: Implement strict allowlist**

Required rules:

```text
required fields: schema_version, name, kind
kind enum: prompt_only, resource, workspace, mcp_dependent, hybrid
security-sensitive unknown fields under runtime/mcp/permissions/workspace/capabilities -> error
ordinary metadata unknown fields -> warning
raw secret markers anywhere in manifest -> redacted error
```

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml manifest --lib
```

Acceptance:

- Manifest is a typed contract, not free-form executable config.
- Complex runtime capabilities only reference Iris-controlled profile/capability.

### Task 3: 完成 runtime/workspace 状态语义

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/model.rs`
- Modify: `src-tauri/src/ai_runtime/skills/scan.rs`
- Modify: `src-tauri/src/ai_runtime/skill_install_service.rs`
- Test: Rust status tests
- Test: `tests/phase4-skills-closed-loop.test.ts`

- [ ] **Step 1: 写失败测试，workspace declared 与 prepared 分离**

```rust
#[test]
fn workspace_declared_but_missing_is_not_prepared() {
    let status = compute_workspace_status(workspace_manifest_fixture(), missing_workspace_dir());
    assert!(status.workspace_declared);
    assert!(!status.workspace_prepared);
    assert!(!status.workspace_missing_items.is_empty());
    assert_eq!(status.generated_files_count, 0);
}
```

- [ ] **Step 2: 写失败测试，`files=[]` 不等于未准备**

```rust
#[test]
fn prepared_workspace_with_no_generated_files_is_ready() {
    let status = compute_workspace_status(workspace_manifest_fixture(), empty_existing_workspace());
    assert!(status.workspace_declared);
    assert!(status.workspace_prepared);
    assert!(status.workspace_missing_items.is_empty());
    assert_eq!(status.generated_files_count, 0);
}
```

- [ ] **Step 3: Implement status DTO**

Required fields:

```text
workspace_declared
workspace_prepared
workspace_missing_items
generated_files_count
runtime_kind
runtime_ready
runtime_status
availability
degraded_reasons
blocked_sections
activated_sections
blocked_capabilities
mcp_dependencies
```

- [ ] **Step 4: Verify UI payload semantics**

```powershell
npm.cmd --prefix D:\Iris run test -- tests/phase4-skills-closed-loop.test.ts
```

Acceptance:

- heartflow-like workspace/hybrid Skill is not judged broken because `files=[]`.
- prompt-only Skill does not show workspace readiness noise.

### Task 4: 完成 MCP Registry schema 与服务

**Files:**

- Create/Modify: `src-tauri/migrations/040_mcp_runtime_registry.sql`
- Create/Modify: `src-tauri/migrations/040_mcp_runtime_registry.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Create/Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs`
- Test: Rust registry/migration tests

- [ ] **Step 1: 写失败测试，profile upsert/list/toggle/delete round trip**

```rust
#[test]
fn mcp_runtime_profile_round_trips_without_secrets() {
    let db = Database::open_in_memory().unwrap();
    upsert_server_catalog(&db, &fake_stdio_server()).unwrap();
    upsert_runtime_profile(&db, &fake_profile()).unwrap();
    let profiles = list_runtime_profiles(&db).unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, "fake-profile");
    assert!(profiles[0].enabled);

    set_runtime_profile_enabled(&db, "fake-profile", false).unwrap();
    assert!(!list_runtime_profiles(&db).unwrap()[0].enabled);

    delete_runtime_profile(&db, "fake-profile").unwrap();
    assert!(list_runtime_profiles(&db).unwrap().is_empty());
}
```

- [ ] **Step 2: 写失败测试，raw secret 被拒绝**

Reject config/env bindings containing:

```text
api_key
token=
bearer
sk-
password
secret
Authorization
```

- [ ] **Step 3: Implement registry service**

Required functions:

```rust
upsert_server_catalog
upsert_runtime_profile
set_runtime_profile_enabled
delete_runtime_profile
list_runtime_profiles
upsert_tool_inventory
list_tool_inventory
record_health_event
list_recent_health_events
resolve_skill_runtime_readiness
```

- [ ] **Step 4: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_runtime_registry --lib
```

Acceptance:

- Registry stores metadata, status, inventory and health, not secrets.
- Profile write operations are auditable and reversible.

### Task 5: 完成 Agent MCP profile 管理工具

**Files:**

- Modify: `src-tauri/src/ai_runtime/tool_catalog/skills.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/skills.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch_impl.rs`
- Modify: `src-tauri/src/ai_runtime/tool_policy.rs`
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
- Test: colocated Rust tests

- [ ] **Step 1: 写失败测试，Catalog 暴露 profile 管理工具**

Tools:

```text
mcp_runtime_profile_upsert
mcp_runtime_profile_toggle
mcp_runtime_profile_delete
mcp_runtime_profiles_list
mcp_runtime_tool_inventory_list
mcp_runtime_health_events_list
```

Expected:

```text
write tools -> requires_confirmation true, access_level ManageSkills
read tools -> read-only, no confirmation unless live runtime is touched
default_enabled_without_skill -> true for meta management tools
```

- [ ] **Step 2: 写失败测试，ToolPolicy 对写 registry 操作要求确认**

Expected verdict:

```rust
ToolPolicyVerdict::RequiresConfirmation
```

- [ ] **Step 3: 写失败测试，permission profile 使用高风险原子**

Required atoms:

```text
skill.mcp_bridge
skill.write_storage
```

- [ ] **Step 4: Implement catalog/policy/permissions/dispatch**

Dispatch round trip:

```text
upsert profile -> list shows enabled -> toggle disabled -> delete -> list empty
```

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_profile_management --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml tool_policy --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml agent_permissions --lib
```

Acceptance:

- Agent can request MCP profile management.
- Every registry write must pass confirmation.
- Meta tools do not depend on one active Skill allowlist.

### Task 6: 完成 MCP Host Runtime stdio tools/list 与 health

**Files:**

- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/skills.rs`
- Test: Rust fake stdio server tests

- [ ] **Step 1: 写失败测试，fake stdio server tools/list 成功并持久化 inventory**

```rust
#[tokio::test]
async fn stdio_tools_list_discovers_and_persists_inventory() {
    let db = Database::open_in_memory().unwrap();
    register_fake_stdio_profile(&db);
    let discovery = discover_profile_tools(&db, "fake-profile", test_options()).await.unwrap();
    assert_eq!(discovery.tools.len(), 2);
    let stored = list_tool_inventory(&db, "fake-profile").unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|tool| !tool.schema_hash.is_empty()));
}
```

- [ ] **Step 2: 写失败测试，stderr/stdout 过大被归一化**

Expected error codes:

```text
output_too_large
invalid_response
timeout
unavailable
```

- [ ] **Step 3: Implement bounded process runtime**

Requirements:

```text
structured command + args only
no shell string
cleared env by default
optional fixed cwd
timeout
max stdout line bytes
max stderr bytes
kill_on_drop
redacted/summarized stderr
health event on failure
```

- [ ] **Step 4: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_host_runtime --lib
```

Acceptance:

- `mcp_runtime_tools_list` and `mcp_runtime_health_check` can execute after confirmation.
- Failure records health event without leaking raw output.

### Task 7: 完成 HTTPS/SSE MCP runtime

**Files:**

- Modify or split: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
- Optional Create: `src-tauri/src/ai_runtime/mcp_http_runtime.rs`
- Test: Rust mocked HTTP/SSE transport tests

- [ ] **Step 1: 写失败测试，HTTPS tools/list 成功**

Use a mocked sender or local test transport to return MCP initialize + tools/list JSON-RPC responses.

Expected call order:

```text
initialize
notifications/initialized
tools/list
```

- [ ] **Step 2: 写失败测试，危险网络目标拒绝且不发送请求**

Reject:

```text
http:// non-dev
localhost when dev mode false
127.0.0.1 when dev mode false
169.254.169.254
metadata.google.internal
private IP unless explicitly allowed by dev mode
redirect to denied target
token-bearing URL
username/password URL
```

- [ ] **Step 3: Implement request runtime**

Requirements:

```text
HTTPS by default
localhost HTTP only in explicit dev mode
redirect policy none
timeout/cancellation
response byte cap
JSON-RPC envelope validation
credential binding only through named secret, not raw header in config
normalized network/auth/output errors
```

- [ ] **Step 4: Connect profile discovery**

`discover_profile_tools` must route by profile transport:

```text
stdio -> stdio runtime
https -> HTTP JSON-RPC runtime
sse -> SSE runtime or explicit unsupported_transport until implemented
```

If SSE is not fully implemented in the same pass, it must return stable `provider_not_implemented`/`unsupported_transport`, not pretend readiness.

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml http_discovery --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_host_runtime --lib
```

Acceptance:

- anysearch can be represented as HTTPS/SSE MCP profile.
- Network boundary cannot be bypassed through Skill manifest or profile config.

### Task 8: 完成 Capability Resolver 与 MCP tools/call 执行链

**Files:**

- Create/Modify: `src-tauri/src/ai_runtime/capability_resolver.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/skills.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/skills.rs`
- Test: Rust resolver/tools-call tests
- Test: `tests/agent-task-capability-contract.test.ts`

- [ ] **Step 1: 写失败测试，MCP tool name 不能直接出现在模型 tool list**

```rust
#[test]
fn mcp_inventory_tool_names_are_not_directly_exposable() {
    let db = Database::open_in_memory().unwrap();
    record_fake_inventory(&db, "raw_search_tool", "web.search");
    let tools = tools_for_model_surface(&db, test_surface()).unwrap();
    assert!(!tools.iter().any(|tool| tool.name == "raw_search_tool"));
}
```

- [ ] **Step 2: 写失败测试，approved capability 才能 call**

```rust
#[tokio::test]
async fn capability_mapped_mcp_tool_call_requires_policy_and_confirmation() {
    let provider = resolve_required_capability(&db, "web.search").unwrap();
    assert_eq!(provider.provider_kind, "mcp");
    assert_eq!(provider.tool_name, "search");
    assert!(provider.requires_confirmation);
}
```

- [ ] **Step 3: 写失败测试，MCP annotations 不自动授权**

```rust
#[test]
fn mcp_annotation_does_not_grant_capability() {
    record_inventory_with_annotation_only(&db, "search", "web.search");
    let err = resolve_required_capability(&db, "web.search").unwrap_err();
    assert_eq!(err.reason_code(), "missing_mcp_profile");
}
```

- [ ] **Step 4: Implement execution flow**

Required flow:

```text
Agent intent -> Iris capability -> resolver selects provider/profile/tool -> permission preflight -> confirmation -> Host Runtime tools/call -> output normalization -> model-safe result
```

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml capability_resolver --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_tools_call --lib
npm.cmd --prefix D:\Iris run test -- tests/agent-task-capability-contract.test.ts
```

Acceptance:

- MCP is a provider behind stable Iris capabilities.
- anysearch maps to `web.search`, not to raw anysearch internal tool names.

### Task 9: 完成 section-level activation gate

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/activation.rs`
- Modify: `src-tauri/src/ai_runtime/skills/prompt.rs`
- Modify: `src-tauri/src/ai_harness/harness/context.rs`
- Test: Rust activation/harness context tests

- [ ] **Step 1: 写失败测试，runtime section blocked 时不注入执行说明**

```rust
#[test]
fn runtime_blocked_section_is_replaced_by_degradation_message() {
    let plan = build_activation_plan(mcp_dependent_skill(), runtime_missing()).unwrap();
    assert!(plan.activated_sections.contains(&"behavior".into()));
    assert!(plan.blocked_sections.contains(&"web-search-usage".into()));
    assert!(!plan.prompt_text.contains("call anysearch"));
    assert!(plan.prompt_text.contains("MCP profile 未启用"));
}
```

- [ ] **Step 2: 写失败测试，prompt-only 完整注入**

Legacy prompt-only and typed prompt-only must preserve existing behavior injection when enabled and readable.

- [ ] **Step 3: Implement gates**

Gate inputs:

```text
requires_runtime
requires_capabilities
requires_resources
requires_workspace
```

Plan output:

```text
activated_sections
blocked_sections
degraded_reasons
blocked_capabilities
prompt_text
```

- [ ] **Step 4: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml activation --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml harness_context --lib
```

Acceptance:

- heartflow behavior layer can be active while daemon layer is unavailable.
- anysearch runtime missing does not guide the model to pretend it can search.

### Task 10: 完成 Resource 与 Workspace 受控访问

**Files:**

- Modify: `src-tauri/src/ai_runtime/skills/resources.rs`
- Modify: `src-tauri/src/ai_runtime/skills/workspace.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/skills.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/skills.rs`
- Test: Rust resource/workspace tests
- Test: `tests/phase4-skills-closed-loop.test.ts`

- [ ] **Step 1: 写失败测试，required resource 缺失阻塞对应 section**

```rust
#[test]
fn missing_required_resource_blocks_resource_section() {
    let status = inspect_skill_resources(resource_skill_fixture_missing_required()).unwrap();
    assert_eq!(status.availability, "partial");
    assert!(status.degraded_reasons.iter().any(|reason| reason.contains("missing resource")));
}
```

- [ ] **Step 2: 写失败测试，workspace write 必须确认**

Expected catalog policy:

```text
skills_workspace_write -> requires_confirmation true
skills_workspace_list -> read-only
skills_read_resource -> read-only
```

- [ ] **Step 3: Implement controlled APIs**

Required tools:

```text
skills_read_resource
skills_workspace_list
skills_workspace_write
```

Rules:

```text
resource path must stay inside skill package
workspace path must stay inside .iris/skills-workspaces/<skill>
write operations produce confirmation summary
no writes to user notes without explicit separate note-edit tool
```

- [ ] **Step 4: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml skills_resource --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml skills_workspace --lib
npm.cmd --prefix D:\Iris run test -- tests/phase4-skills-closed-loop.test.ts
```

Acceptance:

- Resources are read through controlled skill resource APIs.
- Workspace writes are explicit side effects with confirmation.

### Task 11: 完成确认事务语义与 UI 展示

**Files:**

- Modify: `src-tauri/src/ai_harness/harness_confirm.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Modify: `src/components/ai/hooks/useAssistantConfirmations.ts`
- Modify: `src/components/ai/ToolConfirmDialog.tsx`
- Test: `src-tauri/src/ai_harness/harness_confirm_tests.rs`
- Test: `tests/use-assistant-confirmations.test.tsx`
- Test: `tests/tool-confirm-dialog.test.tsx`

- [ ] **Step 1: 写失败测试，side effect committed + resume failed 是 partial success**

```rust
#[test]
fn install_success_resume_provider_error_is_partial_success() {
    let result = confirm_tool_then_resume(fake_install_success(), fake_provider_400()).unwrap();
    assert_eq!(result.tool_execution_outcome.status, "succeeded");
    assert!(result.tool_execution_outcome.side_effect_committed);
    assert_eq!(result.assistant_resume_outcome.status, "failed");
    assert_eq!(result.assistant_resume_outcome.failure_class, Some("provider_bad_request".into()));
}
```

- [ ] **Step 2: 写前端失败测试，UI 不显示“工具失败”**

Expected visible text:

```text
安装已完成，但继续生成回复失败
```

Forbidden visible text for this case:

```text
工具确认失败
Skill 安装失败
```

- [ ] **Step 3: Implement wire shape and render logic**

Required backend shape:

```text
toolExecutionOutcome
assistantResumeOutcome
```

- [ ] **Step 4: Preserve reasoning resume guard**

Do:

```text
if checkpoint contains assistant tool_call without reasoning_content and provider thinking is enabled, downgrade thinking for this resume only
```

Do not:

```text
fake reasoning_content
write hidden reasoning into history
change public IPC schema just to hide provider error
```

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml harness_confirm --lib
npm.cmd --prefix D:\Iris run test -- tests/use-assistant-confirmations.test.tsx tests/tool-confirm-dialog.test.tsx
```

Acceptance:

- Skill install success is not misreported as failure because assistant resume failed.
- Screenshot-style 400 no longer makes users think the Skill install was rolled back.

### Task 12: 完成 MCP 操作专用确认文案

**Files:**

- Modify: `src-tauri/src/ai_harness/harness/run.rs`
- Modify: `src/components/ai/ToolConfirmDialog.tsx`
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
- Test: `tests/tool-confirm-dialog.test.tsx`

- [ ] **Step 1: 写失败测试，profile upsert 确认显示 transport/scope/capability**

Expected visible fields:

```text
MCP profile id
server id
transport
scope
enabled
capability mapping
```

Forbidden:

```text
raw token
raw env value
raw secret
```

- [ ] **Step 2: 写失败测试，live tools/list 显示会触碰 runtime**

Expected copy distinguishes:

```text
注册 MCP Profile
发现 MCP 工具
检查 MCP 健康状态
调用 MCP-backed capability
```

- [ ] **Step 3: Implement confirmation preview builder**

Rules:

```text
stdio command/args may be displayed
secret binding only shown as binding id/configured
registry writes mention reversible path in Settings
live runtime operations mention process/network side effect
```

- [ ] **Step 4: Verify**

```powershell
npm.cmd --prefix D:\Iris run test -- tests/tool-confirm-dialog.test.tsx
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml harness --lib
```

Acceptance:

- User knows whether an action changes registry, starts a process, touches network, or calls a capability.
- UI does not leak secret material.

### Task 13: 完成 MCP/Providers 管理 UI

**Files:**

- Modify: `src/components/ai/SkillsPanel.tsx`
- Create: `src/components/ai/skills/SkillCard.tsx`
- Create: `src/components/ai/skills/SkillStatusBadges.tsx`
- Create: `src/components/ai/skills/McpProfilesPanel.tsx`
- Create: `src/components/ai/skills/McpProfileCard.tsx`
- Modify: `src/types/ai.ts`
- Modify: `src/lib/ipc.ts`
- Test: `tests/skills-settings-permissions.test.ts`

- [ ] **Step 1: 写失败测试，prompt-only 卡片不显示 runtime warning**

Expected UI facts:

```text
kind: prompt-only or legacy prompt-only
runtime: not applicable
no warning about missing MCP profile
```

- [ ] **Step 2: 写失败测试，MCP-dependent missing 显示依赖诊断**

Expected UI meaning:

```text
缺少或未启用 MCP profile
runtime unavailable
blocked capabilities listed
```

Forbidden phrasing:

```text
Skill 坏了
当前可用
```

- [ ] **Step 3: Implement UI sections**

Tabs:

```text
Skills
MCP / Providers
```

Skill card fields:

```text
validation
kind
enabled
availability
runtime_status
workspace_status
activated_sections
blocked_sections
mcp_dependencies
```

MCP card fields:

```text
profile_id
display_name
transport
scope
enabled
health_status
last_error
tool_inventory_count
credential_binding_status
```

- [ ] **Step 4: Verify**

```powershell
npm.cmd --prefix D:\Iris run test -- tests/skills-settings-permissions.test.ts
npm.cmd --prefix D:\Iris run typecheck
npm.cmd --prefix D:\Iris run lint
```

Acceptance:

- User can distinguish behavior layer, workspace layer, runtime layer, MCP/provider layer.
- UI no longer compresses layered state into one “当前可用” label.

### Task 14: 完成 IPC/API contract docs

**Files:**

- Modify: `docs/ipc-api-reference.md`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Test: `tests/ipc-boundary.test.ts`

- [ ] **Step 1: 写 contract test，TS wrapper 覆盖 MCP commands**

Required wrappers:

```text
mcpRuntimeProfileUpsert
mcpRuntimeProfileToggle
mcpRuntimeProfileDelete
mcpRuntimeProfilesList
mcpRuntimeToolsList
mcpRuntimeHealthCheck
mcpRuntimeCapabilityCall
mcpRuntimeToolInventoryList
mcpRuntimeHealthEventsList
```

- [ ] **Step 2: Document state semantics**

Docs must state:

```text
prompt-only does not require MCP
runtime_ready vs activation_ready
workspace_declared vs workspace_prepared
availability available/partial/unavailable
partial success confirmation outcome
MCP inventory does not equal model tool exposure
```

- [ ] **Step 3: Verify**

```powershell
npm.cmd --prefix D:\Iris run test -- tests/ipc-boundary.test.ts
npm.cmd --prefix D:\Iris run typecheck
```

Acceptance:

- Frontend/backend contract can be understood from docs and tests together.

### Task 15: 完成总体验证与回归

**Files:** all touched files。

- [ ] **Step 1: Rust targeted tests**

```powershell
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml skills --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml manifest --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_runtime_registry --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml mcp_host_runtime --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml capability_resolver --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml harness_confirm --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml tool_catalog --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml tool_policy --lib
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml agent_permissions --lib
```

- [ ] **Step 2: Rust quality**

```powershell
cargo fmt --manifest-path D:\Iris\src-tauri\Cargo.toml --all -- --check
cargo clippy --manifest-path D:\Iris\src-tauri\Cargo.toml --all-targets -- -D warnings
```

- [ ] **Step 3: TypeScript tests**

```powershell
npm.cmd --prefix D:\Iris run test -- tests/phase4-skills-closed-loop.test.ts tests/agent-task-capability-contract.test.ts tests/ipc-boundary.test.ts tests/skills-settings-permissions.test.ts tests/use-assistant-confirmations.test.tsx tests/tool-confirm-dialog.test.tsx
npm.cmd --prefix D:\Iris run typecheck
npm.cmd --prefix D:\Iris run lint
```

- [ ] **Step 4: Manual scenario checklist**

```text
Install simple SKILL.md only skill -> enabled -> available -> no runtime warning
Install anysearch -> installed/enabled -> runtime unavailable until MCP profile enabled
Register fake MCP stdio profile -> tools/list confirmation -> inventory stored
Disable profile -> dependent skill degraded
Enable profile -> dependent section injectable
Install heartflow -> behavior active -> daemon/runtime unavailable shown separately
Confirm skill install then simulate provider resume error -> partial success UI
```

## 4. 完成定义

This plan is complete only when all of the following are true:

- 简单 Skill 不依赖 MCP 的路径有测试保护。
- MCP-dependent Skill 的安装、启用、runtime missing、runtime ready、section gate 都有测试保护。
- MCP profile registry、Host Runtime、tools/list、tools/call、health、profile 管理都有权限确认和审计。
- MCP tool 不直接暴露给模型。
- UI 能清晰区分 Skill、workspace、runtime、MCP/provider 状态。
- partial success 不再被显示成 tool failure。
- 所有验证命令通过。

## 5. 风险与取舍

- 把 MCP 当 provider 管理，而不是让 Skill 自带 daemon，会牺牲部分 Hermes/OpenClaw 式“装完就跑”的便利，但能避免任意执行和状态误报。
- Prompt-only 不要求 manifest，可以保持低门槛；但它不能声明自己有 runtime 能力，避免正文里一句“我会搜索”被 Iris 当成真实能力。
- MCP `tools/call` 必须晚于 capability resolver，否则 runtime 接通会成为新的安全绕过点。
- UI 必须展示分层状态，哪怕比单个“可用”标签复杂，因为这是避免用户误判的核心。
