# Iris AI Harness Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` for independent tasks or `superpowers:executing-plans` for inline execution. Track every checkbox and verify each task before moving on.

**Goal:** 将 Iris AI harness 收敛为 Intent Router -> TaskPlan -> Task Policy -> Capability Gate -> Provider Layer -> Dispatch/Permission 的清晰架构；MCP 成为联网证据主路径，DuckDuckGo 是唯一原生托底，MiniMax 退回普通 LLM provider。

**Architecture:** 渐进式重构，不推倒现有正确基础。先用行为测试锁住短答查证、显式研究、MCP->DDG provider 顺序、编辑区选区写入和会话隔离，再分层调整路由、broker、权限、UI 和文档。Legacy scene 只保留兼容，不再作为核心策略输入。

**Tech Stack:** Tauri 2.x, Rust, SQLite, React 19, TypeScript, TipTap/ProseMirror, TailwindCSS + shadcn/ui, Vitest, Cargo tests.

---

## 0. Non-Negotiables

- [ ] 不使用 `apply_patch`。
- [ ] 不新建 worktree，除非用户单独批准。
- [ ] 不新增依赖。
- [ ] 不删除 MCP runtime / registry / host runtime / broker 基础。
- [ ] 不把 MiniMax 作为 web evidence backend。
- [ ] 不把任意 MCP tools 自动暴露给 LLM。
- [ ] 不用 MCP 替代 native vault 文件写入。
- [ ] 不破坏 session、trace、pending confirmation、resume、cancel 生命周期。
- [ ] 不缓存完整 prompt、笔记正文、选区正文、secret、clipboard body、MCP header、完整 query、完整 URL、完整网页正文。

## 1. File Map

### Router / TaskPlan

- Modify: `src/lib/assistant-taskplan.ts`
  - 拆分 fresh web short answer 与 research。
  - 补过渡字段 `evidenceNeed` / `contextNeed` / `operationKind` / `outputShape`。
  - 调整决策顺序：选区写作优先于 research，fresh fact 不升格为 research。
- Modify: `src/lib/assistant-routing.ts`
  - 保持 legacy adapter，只从 TaskPlan 派生旧 intent。
- Modify: `src/types/ai.ts`
  - 增加过渡 TaskPlan 字段和类型。
- Modify: `src-tauri/src/ai_types/mod.rs`
  - Rust DTO 增加同名可选字段，保持 serde 兼容。
- Modify: `src-tauri/src/ai_runtime/task_plan.rs`
  - 增加 helper/summary 校验，作为 policy 输入边界。

### Policy / Permission / Tool Surface

- Modify: `src-tauri/src/ai_runtime/agent_task_policy.rs`
  - 从 TaskPlan facts 派生模型槽位、轮数、budget、联网/写入策略。
- Modify: `src-tauri/src/ai_runtime/tool_policy.rs`
  - 以 capability + policy 决定工具可见性。
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
  - 保持 vault/web/app_state 权限边界；MCP runtime/profile tools 不进入普通 permission profile。
- Modify: `src-tauri/src/ai_runtime/capability_resolver.rs`
  - MCP 只解析 `web.search` / `web.fetch`。

### Web Evidence / MCP

- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
  - search candidates 改为 enabled MCP providers -> DDG。
  - 删除 MiniMax candidate 构造。
  - 保留 MCP fetch + native fetch 合并能力。
- Modify: `src-tauri/src/llm/search_web.rs`
  - 保留 DDG/native 搜索；MiniMax search adapter 不再被 broker 调用。
- Modify: `src-tauri/src/commands/ai_commands.rs`
  - diagnostics 和 summary 不再称 MiniMax 为联网后端。
- Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs`
  - 保留 transport/mapping/credential refs 配置闭环。
- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
  - 保留 controlled stdio/HTTPS runtime，调用只经 broker-facing mapping。

### UI / Editor

- Modify: `src/components/settings/ManagementCenterPanel.tsx`
  - 联网总览显示 MCP providers + DuckDuckGo fallback。
  - 不渲染 MiniMax web backend 配置区。
- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
  - 文案改为 MCP provider 优先、DDG 原生托底。
  - 顶部不重复渲染 preset 卡片。
- Modify: `src/components/ai/skills/McpProfileCard.tsx`
  - 只保留“快速预设”下拉作为预设入口。
- Modify: `src/hooks/useInlineAi.ts`
  - 悬浮 AI 携带 ContextReference，写入动作进入 confirmation。
- Modify: `src/hooks/useAiBubbleSelection.ts`
  - 选区引用轻量化，避免完整长文本缓存/泄露。
- Modify: `src/components/ai/hooks/useAssistantTasks.ts`
  - 每轮请求独立 TaskPlan，不继承上轮 scene。
- Modify: `src/components/ai/EvidenceDetailArtifact.tsx`
  - 普通 evidence 详情隐藏 provider 过程字段。

### Tests

- Modify: `tests/agent-taskplan-routing.test.ts`
- Modify: `tests/web-evidence-broker.test.ts`
- Modify: `tests/management-center-contract.test.ts`
- Modify/Create: `tests/harness-architecture-contract.test.ts`
- Modify: `tests/inline-ai.test.ts`
- Modify: `tests/use-assistant-confirmations.test.tsx`
- Modify: `src-tauri/tests/agent_permission_boundaries.rs`
- Modify: `src-tauri/tests/agent_task_policy.rs`
- Modify: `src-tauri/tests/agent_task_runtime.rs`

### Docs

- Modify: `ARCHITECTURE.md`
- Modify: `ROADMAP.md`
- Modify: `docs/ipc-api-reference.md`
- Modify: `docs/design-system.md`
- Modify: `docs/README.md`

---

## Task 1: RED Tests for Router Semantics

**Purpose:** 先证明当前 bug：fresh fact 被升格为 research；选区编辑和显式研究边界需要稳定。

**Files:**

- Modify: `tests/agent-taskplan-routing.test.ts`

- [ ] **Step 1: Import `ContextReference` type**

Add near existing imports:

```ts
import type { ContextReference } from "@/types/ai";
```

- [ ] **Step 2: Add selection fixture**

Add below `read()` helper:

```ts
function selectionReference(filePath = "/notes/a.md"): ContextReference {
  return {
    id: "selection-1",
    kind: "selection",
    filePath,
    contentHash: "sha256-selection-base",
    utf8Range: { start: 0, end: 12 },
    editorRange: { from: 1, to: 7 },
    excerpt: "你好，世界。",
    headingPath: "正文",
    stale: false,
  };
}
```

- [ ] **Step 3: Add fresh fact short-answer test**

Add inside `describe("regression: ordinary chat must not be promoted by open note or fresh words", ...)`:

```ts
it("routes fresh external fact questions to short answer with web evidence", () => {
  const plan = buildAssistantTaskPlan({
    message: "最新的刑法是哪一年修订的？",
    hasSelection: false,
    notePath: null,
    explicitScope: false,
    contextReferences: [],
    webAuthorized: true,
  });

  expect(plan.intent).toBe("chat");
  expect(plan.intent).not.toBe("research");
  expect(plan.webMode).toBe("brokered");
  expect(plan.executionMode).toBe("direct_answer");
  expect(plan.outputMode).toBe("markdown_message");
  expect(plan.artifactPlan).toEqual([]);
  expect(plan.sourceHints).toContain("web:fresh_required");
});
```

Expected before implementation: FAIL because current `assistant-taskplan.ts` returns `researchPlan(input)` for fresh web evidence.

- [ ] **Step 4: Add web-disabled behavior test**

```ts
it("keeps fresh fact questions direct when web is disabled", () => {
  const plan = buildAssistantTaskPlan({
    message: "最新的刑法是哪一年修订的？",
    hasSelection: false,
    notePath: null,
    explicitScope: false,
    contextReferences: [],
    webAuthorized: false,
  });

  expect(plan.intent).toBe("chat");
  expect(plan.webMode).toBe("disabled");
  expect(plan.requiresClarification).toBe(false);
  expect(plan.artifactPlan).toEqual([]);
});
```

- [ ] **Step 5: Add explicit research positive test**

```ts
it("promotes explicit multi-source legal research to research artifacts", () => {
  const plan = buildAssistantTaskPlan({
    message: "请联网研究综述中国刑法历次修订，并对比多个来源。",
    hasSelection: false,
    notePath: null,
    explicitScope: false,
    contextReferences: [],
    webAuthorized: true,
  });

  expect(plan.intent).toBe("research");
  expect(plan.executionMode).toBe("structured_task");
  expect(plan.outputMode).toBe("artifact_backed_message");
  expect(plan.artifactPlan).toEqual([
    expect.objectContaining({ kind: "evidence_sources" }),
  ]);
});
```

- [ ] **Step 6: Add selected text translation test**

```ts
it("keeps selected-text translation on rewrite confirmation path", () => {
  const plan = buildAssistantTaskPlan({
    message: "翻译成英文",
    hasSelection: true,
    notePath: "/notes/a.md",
    explicitScope: false,
    contextReferences: [selectionReference()],
    webAuthorized: false,
  });

  expect(plan.intent).toBe("rewrite_selection");
  expect(plan.modelSlot).toBe("writer");
  expect(plan.executionMode).toBe("patch_proposal");
  expect(plan.outputMode).toBe("confirmation_required");
  expect(plan.webMode).toBe("disabled");
});
```

- [ ] **Step 7: Run RED**

Run:

```bash
npm run test -- tests/agent-taskplan-routing.test.ts
```

Expected: fresh fact short-answer test fails; other existing tests should remain informative.

---

## Task 2: Add Transitional TaskPlan Fields

**Purpose:** 减少 intent 过载，为 policy 和 UI 提供清晰事实字段。

**Files:**

- Modify: `src/types/ai.ts`
- Modify: `src-tauri/src/ai_types/mod.rs`
- Modify: `src/lib/assistant-taskplan.ts`

- [ ] **Step 1: Add TS types**

In `src/types/ai.ts`, after `TaskPlanConfidence`, add:

```ts
export type EvidenceNeed = "none" | "fresh_web" | "multi_source_research";
export type ContextNeed =
  | "none"
  | "current_reference"
  | "vault_search"
  | "long_document";
export type OperationKind =
  | "answer"
  | "patch"
  | "create"
  | "organize"
  | "diagnose";
export type OutputShape = "chat" | "confirmation" | "artifact" | "diagnostic";
```

- [ ] **Step 2: Extend TS TaskPlan**

In `TaskPlan`, add optional fields after `confidence`:

```ts
  evidenceNeed?: EvidenceNeed;
  contextNeed?: ContextNeed;
  operationKind?: OperationKind;
  outputShape?: OutputShape;
```

- [ ] **Step 3: Extend Rust DTO**

In `src-tauri/src/ai_types/mod.rs`, locate `TaskPlanSummary` / equivalent TaskPlan wire struct and add optional fields using serde defaults:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub evidence_need: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub context_need: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub operation_kind: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub output_shape: Option<String>,
```

- [ ] **Step 4: Update `basePlan` return shape**

In `src/lib/assistant-taskplan.ts`, extend `basePlan` defaults:

```ts
    evidenceNeed: "none",
    contextNeed:
      input.hasSelection || contextReferences.length > 0
        ? "current_reference"
        : "none",
    operationKind: "answer",
    outputShape: "chat",
```

Also update the `Pick<TaskPlan, ...>` type in `basePlan` to include these four keys.

- [ ] **Step 5: Update plan helper override type**

Add the four keys to the omitted default keys in `plan()` so callers can override them safely.

- [ ] **Step 6: Verify types**

Run:

```bash
npm run typecheck
```

Expected: no TS errors from new optional fields.

---

## Task 3: Split Fresh Web Short Answer From Research

**Purpose:** 修复“最新刑法”被研究化的核心 bug。

**Files:**

- Modify: `src/lib/assistant-taskplan.ts`
- Test: `tests/agent-taskplan-routing.test.ts`

- [ ] **Step 1: Add `freshWebShortAnswerPlan`**

Add after `chatPlan`:

```ts
function freshWebShortAnswerPlan(input: BuildAssistantTaskPlanInput): TaskPlan {
  return plan(input, {
    intent: "chat",
    confidence: "medium",
    retrievalMode: "none",
    modelSlot: "fast",
    executionMode: "direct_answer",
    outputMode: "markdown_message",
    artifactPlan: [],
    evidenceNeed: "fresh_web",
    contextNeed: "none",
    operationKind: "answer",
    outputShape: "chat",
  });
}
```

- [ ] **Step 2: Replace fresh web branch**

Change:

```ts
if (input.webAuthorized && needsFreshWebEvidence(message)) {
  return researchPlan(input);
}
```

To:

```ts
if (input.webAuthorized && needsFreshWebEvidence(message)) {
  return freshWebShortAnswerPlan(input);
}
```

- [ ] **Step 3: Make research plan explicit**

In `researchPlan`, set:

```ts
    evidenceNeed: "multi_source_research",
    contextNeed: input.explicitScope ? "vault_search" : "none",
    operationKind: "diagnose",
    outputShape: "artifact",
```

- [ ] **Step 4: Make writer plans explicit**

In `writerPlan`, set:

```ts
    evidenceNeed: "none",
    contextNeed: "current_reference",
    operationKind: intent === "rewrite_selection" ? "patch" : "create",
    outputShape: intent === "rewrite_selection" ? "confirmation" : "chat",
```

- [ ] **Step 5: Run GREEN**

Run:

```bash
npm run test -- tests/agent-taskplan-routing.test.ts
npm run typecheck
```

Expected: fresh fact test passes; explicit research still returns `research`; selected translation remains `rewrite_selection`.

---

## Task 4: Remove MiniMax From Broker Candidate Path

**Purpose:** 让联网证据链路符合 MCP -> DDG 目标态。

**Files:**

- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Modify: `tests/web-evidence-broker.test.ts`

- [ ] **Step 1: Add source contract test**

In `tests/web-evidence-broker.test.ts`, add:

```ts
it("does not use MiniMax as a web evidence backend", () => {
  const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");

  expect(broker).toContain("SearchProviderCandidate::Mcp");
  expect(broker).toContain("WebSearchEffectiveBackend::Duckduckgo");
  expect(broker).not.toContain("WebSearchEffectiveBackend::Minimax,");
  expect(broker).not.toContain("MINIMAX_CREDENTIAL_SERVICE");
});
```

- [ ] **Step 2: Update Rust broker test**

In `web_evidence_broker.rs`, replace `search_provider_candidates_include_minimax_only_when_configured` with:

```rust
#[test]
fn search_provider_candidates_ignore_minimax_credentials() {
    let db = Database::open_in_memory().unwrap();
    crate::credentials::mark_api_key_configured(
        &db,
        crate::credentials::MINIMAX_CREDENTIAL_SERVICE,
    )
    .unwrap();

    let candidates = search_provider_candidates(&db);

    assert_eq!(
        candidates,
        vec![SearchProviderCandidate::Native(
            WebSearchEffectiveBackend::Duckduckgo
        )]
    );
}
```

Expected before implementation: FAIL because current code still pushes MiniMax when credential exists.

- [ ] **Step 3: Remove imports**

Change:

```rust
use crate::credentials::{self, MINIMAX_CREDENTIAL_SERVICE};
```

To remove both if unused:

```rust

```

No replacement import is needed if MiniMax credential lookup is gone.

- [ ] **Step 4: Remove MiniMax candidate construction**

Delete from `search_provider_candidates`:

```rust
    if credentials::api_key_configured(db, MINIMAX_CREDENTIAL_SERVICE).unwrap_or(false) {
        candidates.push(SearchProviderCandidate::Native(
            WebSearchEffectiveBackend::Minimax,
        ));
    }
```

Keep:

```rust
    candidates.push(SearchProviderCandidate::Native(
        WebSearchEffectiveBackend::Duckduckgo,
    ));
    candidates.truncate(2);
```

- [ ] **Step 5: Run broker tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml web_evidence_broker --lib
npm run test -- tests/web-evidence-broker.test.ts
```

Expected: provider candidates are MCP first and DDG second; MiniMax credential no longer changes web evidence order.

---

## Task 5: Update Management Center Networking Semantics

**Purpose:** UI 不再把 MiniMax 称为联网后端，MCP/DDG 状态可解释。

**Files:**

- Modify: `src/components/settings/ManagementCenterPanel.tsx`
- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `tests/management-center-contract.test.ts`

- [ ] **Step 1: Add UI negative tests**

In `tests/management-center-contract.test.ts`, update contracts so the web evidence section asserts:

```ts
expect(center).toContain("McpProfilesPanel");
expect(center).not.toContain("MinimaxSearchSection");
expect(center).not.toContain("native-provider-card-minimax");
expect(mcpPanel).toContain("DuckDuckGo");
expect(mcpPanel).not.toContain("MiniMax 和 DuckDuckGo");
```

- [ ] **Step 2: Remove MiniMax search section from web detail**

In `ManagementCenterPanel.tsx`, remove:

```ts
import { MinimaxSearchSection } from "./MinimaxSearchSection";
```

And remove `<MinimaxSearchSection open={open} />` from the `web-search` detail branch.

- [ ] **Step 3: Update overview label**

Replace logic that maps `status?.searchApi.effectiveBackend === "minimax"` to MiniMax. Use fixed native fallback copy:

```ts
const nativeSearchBackend = "DuckDuckGo / 原生托底";
```

Summary copy should read:

```ts
const label =
  mcpProviderCount > 0
    ? `MCP 提供方 ${mcpProviderCount} 个，原生托底 ${nativeSearchBackend}`
    : `未配置 MCP 提供方，原生托底 ${nativeSearchBackend}`;
```

- [ ] **Step 4: Update MCP panel copy**

In `McpProfilesPanel.tsx`, replace:

```tsx
MiniMax 和 DuckDuckGo 仍作为原生候选兜底
```

With:

```tsx
DuckDuckGo 作为内置原生托底；MiniMax 仅作为普通模型服务，不参与联网证据调度。
```

- [ ] **Step 5: Remove deprecated MiniMax web-search component after references are gone**

After Step 2 proves `ManagementCenterPanel.tsx` no longer imports or renders it, remove the orphan source file:

```text
src/components/settings/MinimaxSearchSection.tsx
```

Then run:

```bash
rg -n "MinimaxSearchSection|native-provider-card-minimax|MiniMax 优先" src tests
```

Expected: only negative tests mention `MinimaxSearchSection`; no source file presents MiniMax as a web evidence backend.

- [ ] **Step 6: Run UI contract tests**

Run:

```bash
npm run test -- tests/management-center-contract.test.ts
```

Expected: no web evidence UI calls MiniMax backend; MCP provider UI still renders and DDG fallback is visible.

---

## Task 6: Keep MCP Provider UI Presets Single-Entry

**Purpose:** 去掉重复入口，只保留“添加 MCP 提供方”和卡片内“快速预设”下拉。

**Files:**

- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `src/components/ai/skills/McpProfileCard.tsx`
- Modify: `tests/management-center-contract.test.ts`

- [ ] **Step 1: Add contract assertions**

In `tests/management-center-contract.test.ts`:

```ts
expect(panel).not.toContain("MCP_PROVIDER_PRESETS.map");
expect(card).not.toContain("MCP_PROVIDER_PRESETS.slice(0, 6)");
expect(card).toContain("快速预设");
expect(card).toContain("自定义 MCP 服务");
expect(card).toContain("AnySearch");
expect(card).toContain("Jina Reader");
expect(card).toContain("Firecrawl");
expect(card).toContain("Tavily");
expect(card).toContain("Brave Search");
expect(card).toContain("SearXNG");
```

- [ ] **Step 2: Remove top preset cards**

In `McpProfilesPanel.tsx`, delete the block that renders `MCP_PROVIDER_PRESETS.map(...)` before provider cards.

- [ ] **Step 3: Remove shortcut button matrix**

In `McpProfileCard.tsx`, delete the block that renders `MCP_PROVIDER_PRESETS.slice(0, 6).map(...)` as buttons next to the select.

- [ ] **Step 4: Update empty state copy**

Use:

```tsx
点击添加 MCP 提供方后，可选择预设或自定义服务。
```

- [ ] **Step 5: Run tests**

```bash
npm run test -- tests/management-center-contract.test.ts
```

Expected: presets are still available via one dropdown only.

---

## Task 7: Lock MCP Capability Boundary

**Purpose:** MCP 能用于 web evidence，但不能扩张成任意 LLM tool platform。

**Files:**

- Modify: `src-tauri/src/ai_runtime/capability_resolver.rs`
- Modify: `src-tauri/src/ai_runtime/tool_policy.rs`
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
- Modify/Create: `src-tauri/tests/agent_permission_boundaries.rs`
- Modify/Create: `tests/harness-architecture-contract.test.ts`

- [ ] **Step 1: Add Rust negative capability tests**

Create or extend `src-tauri/tests/agent_permission_boundaries.rs`:

```rust
#[test]
fn mcp_raw_tools_do_not_enter_permission_profile() {
    let forbidden = [
        "mcp.raw_tool_call",
        "process.run_readonly",
        "process.run_mutating",
        "secret.use_named",
        "vault.write_file",
    ];

    for capability in forbidden {
        assert!(
            crate::ai_runtime::capability_resolver::resolve_required_capability_for_test(capability)
                .is_none(),
            "{capability} must not be resolver-supported"
        );
    }
}
```

If no `resolve_required_capability_for_test` exists, add a `#[cfg(test)] pub(crate)` helper in `capability_resolver.rs` that wraps the real resolver without exposing new production API.

- [ ] **Step 2: Keep resolver allowlist minimal**

Ensure `capability_resolver.rs` accepts only:

```rust
"web.search" | "web.fetch"
```

for MCP web provider mappings.

- [ ] **Step 3: Add source contract**

In `tests/harness-architecture-contract.test.ts`, assert no frontend/bridge code converts `tools/list` directly into model-visible tools:

```ts
const host = read("src-tauri/src/ai_runtime/mcp_host_runtime.rs");
const policy = read("src-tauri/src/ai_runtime/tool_policy.rs");
expect(host).toContain("tools/list");
expect(policy).not.toContain("mcp.raw_tool_call");
```

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml capability_resolver --lib
cargo test --manifest-path src-tauri/Cargo.toml --test agent_permission_boundaries
npm run test -- tests/harness-architecture-contract.test.ts
```

Expected: MCP mapped web providers remain usable; arbitrary MCP tools cannot become ordinary model tools.

---

## Task 8: Preserve Editor Patch and Native Vault Write Safety

**Purpose:** 确保文档编辑区和 AI harness 顺畅合流，但写入仍可确认、可回滚、可隔离。这里的目标不是新增一套 MCP 文件写入，而是让选区 AI、悬浮聊天和右侧会话都复用 native vault 写入边界。

**Files:**

- Modify: `src/hooks/useInlineAi.ts`
- Modify: `src/hooks/useAiBubbleSelection.ts`
- Modify: `src/components/ai/hooks/useAssistantTasks.ts`
- Modify: `src/components/ai/ToolConfirmDialog.tsx`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/markdown.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/vault.rs`
- Modify: `tests/inline-ai.test.ts`
- Modify: `tests/tool-confirm-dialog.test.tsx`
- Modify: `src-tauri/tests/agent_vault_tools.rs`

- [ ] **Step 1: Add selected-text workflow test**

In `tests/inline-ai.test.ts`, add a test under `describe("useInlineAi with mocked IPC", ...)` that starts a translate/rewrite request with an active selection and inspects the mocked `assistantExecute` payload:

```ts
it("sends selected text as a bounded context reference and asks for confirmation", async () => {
  await act(async () => {
    await api.runInlineAction({
      action: "translate",
      selectedText: "你好，世界。",
      notePath: "/notes/a.md",
      selectionRange: { from: 1, to: 7 },
      contentHash: "sha256-selection-base",
    });
  });

  const request = assistantExecute.mock.calls.at(-1)?.[0];
  expect(request.contextReferences[0]).toMatchObject({
    kind: "selection",
    filePath: "/notes/a.md",
    contentHash: "sha256-selection-base",
    editorRange: { from: 1, to: 7 },
  });
  expect(request.contextReferences[0].excerpt).toContain("你好");
  expect(request.userMessage).not.toContain("完整长选区正文");
  expect(request.taskPlan.intent).toBe("rewrite_selection");
  expect(request.taskPlan.operationKind).toBe("patch");
  expect(request.taskPlan.outputMode).toBe("confirmation_required");
});
```

Expected before implementation if missing: FAIL because the request either lacks `contextReferences`, lacks `contentHash`, or routes selection work through a generic chat path.

- [ ] **Step 2: Add backend hash mismatch rejection test**

In `src-tauri/tests/agent_vault_tools.rs`, add:

```rust
#[tokio::test]
async fn markdown_patch_rejects_hash_mismatch_without_writing() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/test.md", "# Test\nHello world");

    let result = dispatch_tool(
        &state,
        &ctx(Some("notes/test.md")),
        "replace_selection",
        &serde_json::json!({
            "target_path": "notes/test.md",
            "replacement": "Hi",
            "base_content_hash": "sha256-stale",
            "range": {"start": 7, "end": 12},
            "original_text": "Hello",
            "risk_level": "medium"
        }),
    )
    .await;

    assert!(!result.success);
    let content = std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
    assert_eq!(content, "# Test\nHello world");
}
```

- [ ] **Step 3: Add out-of-vault rejection test**

In `src-tauri/tests/agent_vault_tools.rs`, add a write attempt with `target_path: "../outside.md"` and assert `!result.success` plus no file created outside `state.vault_path()`.

- [ ] **Step 4: Add confirmation surface contract**

In `tests/tool-confirm-dialog.test.tsx`, assert write tools show target path, risk level, base hash / stale warning, and never auto-dispatch on render:

```ts
expect(container.textContent).toContain("notes/a.md");
expect(container.textContent).toContain("需要确认");
expect(confirmHandler).not.toHaveBeenCalled();
```

- [ ] **Step 5: Keep write tools confirmation-gated in catalog**

In Rust tests, assert these tools require confirmation:

```rust
for name in ["replace_selection", "vault_create_note", "vault_rename_move", "vault_delete_to_trash"] {
    let entry = catalog_find(name).expect(name);
    assert!(entry.requires_confirmation, "{name} must require confirmation");
}
```

- [ ] **Step 6: Run tests**

```bash
npm run test -- tests/inline-ai.test.ts tests/tool-confirm-dialog.test.tsx tests/use-assistant-confirmations.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml --test agent_vault_tools
cargo test --manifest-path src-tauri/Cargo.toml tool_dispatch --lib
```

Expected: editor write operations remain native, scoped, confirmed and reversible; no MCP file tool is needed or exposed.

---

## Task 9: Session Lifecycle and Confirmation Isolation

**Purpose:** 确保架构收口不破坏 AI 会话隔离、保存、恢复和取消。路由、provider choice 和 pending confirmation 都是 request/session 绑定状态，不能漂移到另一个会话或当前打开的另一篇文档。

**Files:**

- Modify: `src/components/ai/hooks/useAssistantTasks.ts`
- Modify: `src/components/ai/hooks/useAssistantConfirmations.ts`
- Modify: `src-tauri/src/ai_runtime/agent_task.rs`
- Modify: `src-tauri/src/ai_harness/harness/run.rs`
- Modify: `tests/use-assistant-confirmations.test.tsx`
- Modify: `src-tauri/tests/agent_task_runtime.rs`

- [ ] **Step 1: Add no-cross-session frontend test**

In `tests/use-assistant-confirmations.test.tsx`, build this sequence:

```ts
it("does not confirm a pending write after switching sessions", async () => {
  const sessionA = 101;
  const sessionB = 202;
  const executeTool = vi.fn();

  const harness = renderConfirmationsHarness({
    activeSessionId: sessionA,
    executeTool,
  });
  harness.addPendingConfirmation({
    sessionId: sessionA,
    requestId: "req-a",
    taskId: "task-a",
    toolCallId: "tool-a",
    toolName: "replace_selection",
    targetPath: "notes/a.md",
  });

  harness.switchSession(sessionB);
  await harness.confirm("tool-a");

  expect(executeTool).not.toHaveBeenCalled();
  expect(harness.text()).toContain("确认已失效");
});
```

If the current test harness has different helper names, keep the same assertions: session mismatch must stop dispatch before IPC/backend write.

- [ ] **Step 2: Add backend resume guard test**

In `src-tauri/tests/agent_task_runtime.rs`, add a test that creates two sessions and one paused task in session A, then calls `AgentTaskRuntime::resume_preflight` or equivalent with session B:

```rust
#[test]
fn resume_preflight_rejects_session_mismatch() {
    let db = Database::open_in_memory().unwrap();
    let session_a = SessionManager::ensure(&db, AiScene::DraftingAssist, None).unwrap();
    let session_b = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(&db, CreateTaskInput {
        request_id: "req-session-a".into(),
        session_id: session_a,
        kind: AgentTaskKind::Complex,
        user_input: "rewrite selected text".into(),
        budget_policy: serde_json::json!({"scope_packet_hash":"scope-a"}),
    }).unwrap();

    let err = AgentTaskRuntime::resume_preflight(&db, AgentTaskResumePreflight {
        task_id: task_id.clone(),
        session_id: session_b,
        request_id: "req-session-a".into(),
        scope_packet_hash: Some("scope-a".into()),
        model_slot: Some("writer".into()),
    }).unwrap_err();

    assert!(err.to_string().contains("session"));
}
```

Adapt field names to the existing `AgentTaskResumePreflight` struct; do not weaken the assertion to a source-string check.

- [ ] **Step 3: Add safe trace metadata test**

Extend `checkpoint_rejects_full_context_and_secret_shaped_fields` or add a sibling test asserting these fields are rejected in task checkpoints/trace metadata:

```rust
serde_json::json!({
  "task_plan": {"intent":"chat", "evidenceNeed":"fresh_web"},
  "policy": {"web":"enabled"},
  "provider_choice": {"kind":"mcp", "config_hash":"sha256-ok"},
  "full_prompt": "must not persist",
  "selected_text": "must not persist",
  "headers": {"Authorization":"Bearer secret"}
})
```

Expected: safe summaries are allowed only after removing the forbidden fields; forbidden fields cause a validation error.

- [ ] **Step 4: Confirm cancel remains local**

Add or update a frontend/backend test so canceling request A does not clear session B messages, pending confirmations, or recoverable task state. Assert by id counts, not by broad UI text.

- [ ] **Step 5: Run tests**

```bash
npm run test -- tests/use-assistant-confirmations.test.tsx tests/use-assistant-run.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --test agent_task_runtime
cargo test --manifest-path src-tauri/Cargo.toml ai_harness::harness::run --lib
```

Expected: pending confirmations, resume and cancel are session-scoped and request-scoped; no stale confirmation can write into the currently active document.

---

## Task 10: Cache, Audit and Evidence Detail Privacy

**Purpose:** 保证联网和 LLM 过程可审计，但不泄露原文、secret 或 provider raw details。缓存只服务当前安全边界内的性能和可恢复性，不做跨会话 prompt-response 复用。

**Files:**

- Modify: `src-tauri/src/ai_runtime/tool_audit.rs`
- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Modify: `src-tauri/src/llm/search_web.rs`
- Modify: `src-tauri/src/llm/fetch_web_page.rs`
- Modify: `src-tauri/tests/context_cache.rs`
- Modify: `src/components/ai/EvidenceDetailArtifact.tsx`
- Modify: `tests/evidence-detail-artifact.test.tsx`
- Modify/Create: `tests/harness-architecture-contract.test.ts`

- [ ] **Step 1: Add audit privacy test**

In the Rust audit tests, record a tool event with sensitive-looking arguments and assert the persisted `arguments_summary` contains only safe classification fields:

```rust
#[test]
fn tool_audit_redacts_queries_urls_headers_and_page_body() {
    let db = Database::open_in_memory().unwrap();
    record_audit(&db, &ToolAuditInput {
        request_id: "req-privacy".into(),
        harness_round: 1,
        tool_name: "web_search".into(),
        arguments: serde_json::json!({
            "query": "完整用户问题不应保存",
            "url": "https://example.com/private/path?token=secret",
            "headers": {"Authorization": "Bearer secret"},
            "page_body": "完整网页正文不应保存"
        }),
        success: false,
        failure_class: Some("provider_auth_missing".into()),
    }).unwrap();

    let row = query_by_request(&db, "req-privacy").unwrap().remove(0);
    assert!(row.arguments_summary.contains("provider_auth_missing"));
    assert!(!row.arguments_summary.contains("完整用户问题"));
    assert!(!row.arguments_summary.contains("token=secret"));
    assert!(!row.arguments_summary.contains("Authorization"));
    assert!(!row.arguments_summary.contains("完整网页正文"));
}
```

- [ ] **Step 2: Add cache key isolation test**

In `src-tauri/tests/context_cache.rs` or the web cache test module, assert cache keys differ across vault/provider/config/broker version:

```rust
assert_ne!(key("vault-a", "mcp:anysearch", "cfg-1", "broker-v1"), key("vault-b", "mcp:anysearch", "cfg-1", "broker-v1"));
assert_ne!(key("vault-a", "mcp:anysearch", "cfg-1", "broker-v1"), key("vault-a", "duckduckgo", "cfg-1", "broker-v1"));
assert_ne!(key("vault-a", "mcp:anysearch", "cfg-1", "broker-v1"), key("vault-a", "mcp:anysearch", "cfg-2", "broker-v1"));
assert_ne!(key("vault-a", "mcp:anysearch", "cfg-1", "broker-v1"), key("vault-a", "mcp:anysearch", "cfg-1", "broker-v2"));
```

The exact helper can wrap the existing cache key type; the invariant is the test’s point.

- [ ] **Step 3: Add evidence detail DTO/UI privacy test**

In `tests/evidence-detail-artifact.test.tsx`, extend the web evidence fixture with hidden audit fields if the DTO currently exposes them, then assert they are not rendered:

```ts
const sensitiveWebEvidence = {
  ...webEvidence,
  providerId: "mcp.anysearch.primary",
  providerKind: "mcp",
  rawResultHash: "sha256-raw-result",
  extractionMethod: "provider_raw_extract",
  fromCache: true,
  fetchBackend: "mcp.web_fetch",
} as unknown as SessionEvidenceDetailRecord;

expect(container.textContent).toContain("Official Web");
expect(container.textContent).toContain("example.com");
expect(container.textContent).not.toContain("mcp.anysearch.primary");
expect(container.textContent).not.toContain("provider_raw_extract");
expect(container.textContent).not.toContain("sha256-raw-result");
expect(container.textContent).not.toContain("fetch backend");
```

- [ ] **Step 4: Add source contract for no prompt-response cache**

In `tests/harness-architecture-contract.test.ts`, assert production code does not define a cross-session prompt/response cache table or API names:

```ts
const migrations = readAll("src-tauri/migrations");
const sources = readAll("src-tauri/src");
expect(migrations).not.toMatch(/prompt_response_cache|llm_response_cache/);
expect(sources).not.toMatch(
  /save_full_prompt|cachePromptResponse|prompt_response_cache/,
);
```

- [ ] **Step 5: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml tool_audit --lib
cargo test --manifest-path src-tauri/Cargo.toml search_web --lib
cargo test --manifest-path src-tauri/Cargo.toml fetch_web_page --lib
cargo test --manifest-path src-tauri/Cargo.toml --test context_cache
npm run test -- tests/evidence-detail-artifact.test.tsx tests/harness-architecture-contract.test.ts
```

Expected: evidence remains useful to users; diagnostic/audit details stay out of ordinary UI; cache keys cannot cross vault/provider/config/broker boundaries.

---

## Task 11: Documentation Sync

**Purpose:** 文档必须解释真实目标态，不能继续传播 MiniMax 联网后端或 MCP 工具平台心智。

**Files:**

- Modify: `ARCHITECTURE.md`
- Modify: `ROADMAP.md`
- Modify: `docs/ipc-api-reference.md`
- Modify: `docs/design-system.md`
- Modify: `docs/README.md`

- [ ] **Step 1: Update architecture chain**

Document:

```text
Intent Router -> TaskPlan -> Task Policy -> Capability Gate -> Provider Layer -> Dispatch/Permission
```

- [ ] **Step 2: Update networking docs**

State:

- MCP web providers are primary。
- DuckDuckGo is native fallback。
- MiniMax is a normal LLM provider, not web evidence backend。
- MCP tools are not auto-exposed to LLM。

- [ ] **Step 3: Update UI design docs**

Document low-interference routing visibility, evidence detail boundaries, MCP diagnostics and editor selection confirmation UX.

- [ ] **Step 4: Run docs search**

```bash
rg -n "MiniMax.*联网后端|MCP > MiniMax|SearchProviderCandidate::Minimax|智能场景路由|MCP tools.*自动" ROADMAP.md ARCHITECTURE.md docs src tests
```

Expected: remaining hits are historical/superseded docs, negative tests, or explicit removal notes.

---

## Task 12: Full Quality Gate and Requirement Audit

**Purpose:** 不靠感觉收工，用命令和需求对照证明完成。

- [ ] **Step 1: Frontend gate**

Run:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
```

Expected: all pass.

- [ ] **Step 2: Rust gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: all pass.

- [ ] **Step 3: Final architecture search**

Run:

```bash
rg -n "SearchProviderCandidate::Minimax|FetchProviderCandidate::Minimax|MCP > MiniMax|MiniMax.*web evidence|legacyScene.*primary|RESEARCH_KEYWORDS|mcp_runtime_capability_call|secret\.use_named|process\.run_" src src-tauri tests docs ROADMAP.md ARCHITECTURE.md
```

Expected: remaining hits are negative tests, historical/superseded docs, native internal implementation not model-visible, or explicit removal notes.

- [ ] **Step 4: Manual acceptance scenarios**

Verify in the app:

1. Ask “最新的刑法是哪一年修订的？” with web on: short answer, web evidence, no research artifact。
2. Ask same with web off: short answer, no web prompt/clarification。
3. Ask “写一份中国刑法修订历史研究综述”: research path。
4. Configure AnySearch MCP: diagnostics pass and provider is shown as actual web provider。
5. Disable AnySearch: DDG fallback visible。
6. Select text and request translation: confirmation card, patch only selection。
7. Switch sessions while confirmation pending: no cross-session write。

- [ ] **Step 5: Requirement audit**

Produce final evidence table against `docs/superpowers/specs/2026-07-01-iris-ai-harness-architecture-design.md`:

```text
Router: tests and source refs
Policy: tests and source refs
MCP: tests and source refs
Broker: tests and source refs
Editor: tests and source refs
Session lifecycle: tests and source refs
Cache/audit: tests and source refs
Docs: changed files and search result
Quality gates: command output summary
```

Only after every line has evidence can the goal be called complete.

---

## Execution Order

1. Task 1 -> Task 3: fix the user-visible research misrouting first。
2. Task 4 -> Task 6: complete MCP/DDG web evidence target and UI cleanup。
3. Task 7 -> Task 10: harden capability, editor writes, sessions and privacy。
4. Task 11 -> Task 12: docs, full gates and final requirement audit。

## Commit Guidance

Do not commit unless explicitly asked. If committing, use Chinese Conventional Commit messages, for example:

```bash
git commit -m "fix(ai): 修正联网短答和 MCP 证据调度边界"
```
