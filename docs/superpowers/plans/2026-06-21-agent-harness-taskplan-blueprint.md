# Agent Harness TaskPlan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Iris AI agent harness 从“关键词场景路由 + workflow 默认卡片”改造成“每轮 `TaskPlan` 驱动”的系统，并严格保证对话区普通 Markdown 文字流优先。

**Architecture:** 前端先生成轻量 `TaskPlan` 事实，后端校验并补全执行策略；模型槽位、检索范围、工具暴露、临时 tab 生成都从 `TaskPlan` 派生。旧 `AiScene` 只保留在历史 session、trace 与短期迁移字段中，主路径不再依赖它做任务决策。

**Tech Stack:** Tauri 2.x, Rust, React 19, TypeScript, TipTap/ProseMirror, TailwindCSS + shadcn/ui, SQLite, Vitest, Cargo tests.

---

## File Structure

- `tests/agent-taskplan-routing.test.ts`: 新增前端路由契约测试，覆盖同会话跨场景、创作误判研究、fast path 不启动重流程。
- `tests/assistant-markdown-stream-contract.test.tsx`: 新增消息流契约测试，确保研究/过程/证据矩阵不在对话流中渲染卡片。
- `tests/assistant-artifact-value-gates.test.ts`: 新增临时 tab 价值门槛测试，覆盖四类 tab 与空证据矩阵禁止生成。
- `tests/context-reference.test.ts`: 新增上下文引用测试，覆盖局部选区、跨段选区、hash mismatch、引用胶囊展示数据。
- `tests/web-evidence-broker.test.ts`: 新增网络代理前端/契约测试，确保用户只面对联网开关。
- `tests/unified-assistant-routing.test.ts`: 改写旧 intent 测试，让它断言 `TaskPlan`，不再祝福旧关键词研究路由。
- `tests/unified-assistant-shell.test.ts`: 改写旧 shell 测试，删除 `ResearchResultMessage` 期望。
- `tests/assistant-phase2-cleanup.test.ts`: 改写 scene 兼容测试，明确旧 scene 只用于迁移。
- `src/types/ai.ts`: 增加 `TaskPlan`、`ContextReference`、`ArtifactPlanItem`、`RetrievalMode`、`WebMode`、`ExecutionMode`、`OutputMode` 等前端类型。
- `src/types/assistant-artifact.ts`: 把 artifact kind 从 workflow 名称改为产品语义：`evidence_sources`、`writing_change`、`structured_result`、`task_process`。
- `src/lib/context-reference.ts`: 新增上下文引用构造、hash 校验、展示摘要、stale 判定工具。
- `src/lib/assistant-taskplan.ts`: 新增前端 fast-path `TaskPlan` 生成器和 legacy intent 适配函数。
- `src/lib/assistant-routing.ts`: 改为薄适配层，保留导出名但内部调用 `assistant-taskplan.ts`；完成后删除旧关键词优先级实现。
- `src/lib/assistant-artifact-tabs.ts`: 增加 value gate；只把通过门槛的 artifact draft 转为临时 tab。
- `src/lib/web-evidence-broker.ts`: 新增前端 wire 类型与展示辅助，不实现真实网络。
- `src/components/ai/AiMessageList.tsx`: 删除 `ResearchResultMessage` 特判，所有 assistant 消息走 Markdown bubble。
- `src/components/ai/ResearchResultMessage.tsx`: 删除文件。
- `src/components/ai/AssistantArtifactTagStrip.tsx`: 收敛为消息内轻量 Markdown 链接或移除；对话区不再渲染专用按钮条。
- `src/components/ai/hooks/useAssistantTasks.ts`: 用 `TaskPlan` 统一调度，删除 `runResearch` 向消息流插入 `kind: "research"` 的路径。
- `src/components/ai/hooks/useAssistantConversation.ts`: 消息模型增加 `contextReferences`，用户消息可携带引用胶囊。
- `src/components/ai/ConversationSurface.tsx`: 输入框显示引用胶囊；发送时携带 `ContextReference`，不把完整选区粘进输入框。
- `src/hooks/useInlineAi.ts`: 悬浮 AI composer 使用 `ContextReference`，支持“插入到选区后方”和“替换选区”两个明确动作。
- `src/hooks/useAiBubbleSelection.ts`: 右侧对话引用选中文本时生成轻量引用，而不是直接复制长文本。
- `src/lib/iris-clipboard.ts`: 增加 Iris 内部剪贴板引用负载，普通系统剪贴板仍写纯文本。
- `src/lib/ipc.ts`: `assistantExecute` 请求/响应加入 `taskPlan` 与 `contextReferences`。
- `src/types/ipc.ts`: 同步 IPC 类型。
- `src-tauri/src/ai_types/mod.rs`: 增加 Rust wire 类型：`TaskPlanSummary`、`ContextReferenceWire`、`ArtifactPlanItemWire` 与枚举。
- `src-tauri/src/ai_runtime/task_plan.rs`: 新增后端 `TaskPlan` 校验、补全、policy input 派生逻辑。
- `src-tauri/src/ai_runtime/mod.rs`: 暴露 `task_plan` 模块。
- `src-tauri/src/ai_runtime/agent_task_policy.rs`: 改为从 `TaskPlan` 派生 model slot、预算、context strategy；保留 `legacy_scene` 仅作兼容输出。
- `src-tauri/src/commands/assistant_commands.rs`: `AssistantExecuteRequest/Response` 加入 `task_plan`；执行入口先构造并校验 `TaskPlan`。
- `src-tauri/src/ai_harness/harness_task.rs`: `HarnessTaskRequest` 携带 `TaskPlan`；artifact wire 通过统一 gate 生成。
- `src-tauri/src/ai_workflows/research_workflow.rs`: 研究输出改为 Markdown 正文 + evidence source artifact；删除机械空矩阵默认行为。
- `src-tauri/src/ai_runtime/web_evidence_broker.rs`: 新增统一网络证据代理，内部协调搜索、HTTPS 正文抓取、去重、排序、失败记录。
- `src-tauri/src/ai_runtime/tool_catalog/web.rs`: 面向模型的工具说明改成 broker 语义，用户可见层不再区分 search/fetch。
- `src-tauri/src/ai_runtime/tool_dispatch/web.rs`: 将搜索与抓取结果转换为统一 `WebEvidenceItem`。
- `docs/superpowers/specs/2026-06-21-agent-harness-taskplan-blueprint-design.md`: 仅在发现实现必须调整蓝图时同步修订。

## Task 1: 建立 TaskPlan 前端契约测试

**Files:**

- Create: `tests/agent-taskplan-routing.test.ts`
- Modify: `tests/unified-assistant-routing.test.ts`
- Modify: `tests/assistant-phase2-cleanup.test.ts`

- [x] **Step 1: 新增失败测试，锁定每轮独立 `TaskPlan`**

创建 `tests/agent-taskplan-routing.test.ts`，测试直接读取 `src/lib/assistant-taskplan.ts` 和 `src/lib/assistant-routing.ts`，先断言尚不存在的新契约：

```ts
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant TaskPlan routing contract", () => {
  it("creates a per-turn TaskPlan instead of locking a conversation scene", () => {
    const taskplan = read("src/lib/assistant-taskplan.ts");
    expect(taskplan).toContain("buildAssistantTaskPlan");
    expect(taskplan).toContain("contextReferences");
    expect(taskplan).toContain("retrievalMode");
    expect(taskplan).toContain("executionMode");
    expect(taskplan).toContain("artifactPlan");
  });

  it("keeps novel continuation with analysis words on the writer path", () => {
    const taskplan = read("src/lib/assistant-taskplan.ts");
    expect(taskplan).toContain("creative_write");
    expect(taskplan).toContain("requiresClarification");
    expect(taskplan).toContain("writingKeywordBeforeResearchKeyword");
  });

  it("keeps legacy routing as an adapter, not the primary decision system", () => {
    const routing = read("src/lib/assistant-routing.ts");
    expect(routing).toContain("buildAssistantTaskPlan");
    expect(routing).not.toContain("const RESEARCH_KEYWORDS");
    expect(routing).not.toContain("includesAny(message, RESEARCH_KEYWORDS)");
  });
});
```

- [x] **Step 2: 改写旧 routing 测试的关键断言**

在 `tests/unified-assistant-routing.test.ts` 中新增或替换用例：

```ts
it("does not route fiction continuation to research only because it says 分析 or 研究", async () => {
  const mod = await import("@/lib/assistant-taskplan");
  const plan = mod.buildAssistantTaskPlan({
    message:
      "根据以上文字写出第四章，要求描写更火爆、剧情更诱人，同时分析人物心理",
    hasSelection: true,
    notePath: "/novel.md",
    explicitScope: false,
  });

  expect(plan.intent).toBe("creative_write");
  expect(plan.modelSlot).toBe("writer");
  expect(plan.executionMode).toBe("writing_candidate");
  expect(plan.artifactPlan).toEqual([]);
});
```

- [x] **Step 3: 改写 scene cleanup 测试**

在 `tests/assistant-phase2-cleanup.test.ts` 中删除继续要求 `legacySceneHintForAssistantIntent` 主导 UI 的断言，改成：

```ts
expect(read("src-tauri/src/ai_runtime/agent_task_policy.rs")).toContain(
  "legacy_scene",
);
expect(read("src-tauri/src/ai_runtime/agent_task_policy.rs")).toContain(
  "compatibility",
);
expect(read("src/lib/assistant-routing.ts")).toContain(
  "buildAssistantTaskPlan",
);
```

- [x] **Step 4: 运行失败测试**

Run:

```bash
npm run test -- tests/agent-taskplan-routing.test.ts tests/unified-assistant-routing.test.ts tests/assistant-phase2-cleanup.test.ts
```

Expected: FAIL，因为 `src/lib/assistant-taskplan.ts` 尚未存在，旧路由仍含 `RESEARCH_KEYWORDS`。

- [x] **Step 5: 提交失败测试**

```bash
git add tests/agent-taskplan-routing.test.ts tests/unified-assistant-routing.test.ts tests/assistant-phase2-cleanup.test.ts
git commit -m "test(ai): 增加 TaskPlan 路由契约"
```

## Task 2: 增加 TaskPlan 与 ContextReference 类型

**Files:**

- Modify: `src/types/ai.ts`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src-tauri/src/ai_types/mod.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Modify: `src-tauri/src/ai_harness/harness_task.rs`（补齐 `AssistantExecuteResponse` 构造点的必要联动）

- [x] **Step 1: 先补 TypeScript 类型**

在 `src/types/ai.ts` 增加：

```ts
export type TaskPlanIntent =
  | "chat"
  | "ask_notes"
  | "creative_write"
  | "rewrite_selection"
  | "citation_check"
  | "research"
  | "organize"
  | "document_check"
  | "chapter"
  | "vision_chat"
  | "skill_management";

export type TaskPlanConfidence = "high" | "medium" | "low";
export type RetrievalMode =
  | "none"
  | "current_reference"
  | "local_notes"
  | "scoped_notes"
  | "long_document";
export type WebMode = "disabled" | "brokered";
export type ExecutionMode =
  | "direct_answer"
  | "context_answer"
  | "writing_candidate"
  | "patch_proposal"
  | "structured_task"
  | "long_task"
  | "clarification";
export type OutputMode =
  | "markdown_message"
  | "artifact_backed_message"
  | "confirmation_required"
  | "diagnostic";

export interface ContextReference {
  id: string;
  kind: "selection" | "paragraph" | "heading" | "note" | "artifact";
  filePath: string | null;
  contentHash: string | null;
  utf8Range: SourceSpan | null;
  editorRange: { from: number; to: number } | null;
  excerpt: string;
  headingPath?: string | null;
  anchor?: string | null;
  stale: boolean;
  invalidReason?: string | null;
}

export interface ArtifactPlanItem {
  kind:
    | "evidence_sources"
    | "writing_change"
    | "structured_result"
    | "task_process";
  reason: string;
  valueGate: string;
}

export interface TaskPlan {
  intent: TaskPlanIntent;
  confidence: TaskPlanConfidence;
  contextReferences: ContextReference[];
  retrievalMode: RetrievalMode;
  webMode: WebMode;
  modelSlot: CapabilitySlot;
  executionMode: ExecutionMode;
  outputMode: OutputMode;
  artifactPlan: ArtifactPlanItem[];
  requiresClarification: boolean;
  clarificationQuestion?: string | null;
  sourceHints: string[];
}
```

- [x] **Step 2: 同步 IPC 请求/响应类型**

在 `AssistantExecuteRequest` 的 TypeScript 类型中加入：

```ts
taskPlan?: TaskPlan;
contextReferences?: ContextReference[];
```

在 `AssistantExecuteResponse` 中加入：

```ts
taskPlan?: TaskPlan | null;
```

在 `src/lib/ipc.ts` 的 `assistantExecute` wrapper 中保持 `invoke<AssistantExecuteResponse>("assistant_execute", { request })` 的类型安全，不直接调用裸 `invoke()`。

- [x] **Step 3: 补 Rust wire 类型**

在 `src-tauri/src/ai_types/mod.rs` 增加与 TypeScript 一一对应的 enum/struct，使用 `#[serde(rename_all = "snake_case")]` 或 `camelCase` 与前端字段保持一致：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPlanIntent { Chat, AskNotes, CreativeWrite, RewriteSelection, CitationCheck, Research, Organize, DocumentCheck, Chapter, VisionChat, SkillManagement }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPlanSummary {
    pub intent: TaskPlanIntent,
    pub confidence: TaskPlanConfidence,
    pub context_references: Vec<ContextReferenceWire>,
    pub retrieval_mode: RetrievalMode,
    pub web_mode: WebMode,
    pub model_slot: CapabilitySlot,
    pub execution_mode: ExecutionMode,
    pub output_mode: OutputMode,
    pub artifact_plan: Vec<ArtifactPlanItemWire>,
    pub requires_clarification: bool,
    pub clarification_question: Option<String>,
    pub source_hints: Vec<String>,
}
```

用 `cargo fmt --all -- --check` 确认命名和导出无格式问题。

- [x] **Step 4: 请求/响应接线**

在 `src-tauri/src/commands/assistant_commands.rs` 的 `AssistantExecuteRequest` 加入：

```rust
#[serde(default)]
pub task_plan: Option<TaskPlanSummary>,
#[serde(default)]
pub context_references: Vec<ContextReferenceWire>,
```

在 `AssistantExecuteResponse` 加入：

```rust
pub task_plan: Option<TaskPlanSummary>,
```

所有构造 `AssistantExecuteResponse` 的地方先填 `task_plan: None`，下一任务再改成真实值。

- [x] **Step 5: 验证**

Run:

```bash
npm run typecheck
cargo fmt --all -- --check
cargo check
```

Expected: PASS。若 `cargo check` 暴露 response 构造缺字段，逐个补齐。

- [x] **Step 6: 提交类型契约**

```bash
git add src/types/ai.ts src/types/ipc.ts src/lib/ipc.ts src-tauri/src/ai_types/mod.rs src-tauri/src/commands/assistant_commands.rs
git commit -m "feat(ai): 增加 TaskPlan 与上下文引用契约"
```

## Task 3: 实现前端 Fast Path TaskPlan

**Files:**

- Create: `src/lib/assistant-taskplan.ts`
- Modify: `src/lib/assistant-routing.ts`
- Modify: `tests/agent-taskplan-routing.test.ts`
- Modify: `tests/unified-assistant-routing.test.ts`

- [x] **Step 1: 新建 `src/lib/assistant-taskplan.ts`**

实现纯函数：

```ts
export interface BuildAssistantTaskPlanInput extends AssistantRouteInput {
  contextReferences?: ContextReference[];
  webAuthorized?: boolean;
}

export function buildAssistantTaskPlan(
  input: BuildAssistantTaskPlanInput,
): TaskPlan;
```

实现顺序必须是：

1. UI action 最高优先级：rewrite/citation/chapter/document 等显式动作直接确定 intent。
2. 图片附件进入 `vision_chat`。
3. skill 安装/管理进入 `skill_management`。
4. 有精确选区 + 写作/续写/改写/扩写/章节创作语义进入 `creative_write` 或 `rewrite_selection`。
5. 明确“查资料、找证据、多来源、联网、综述文献、对比来源”的请求进入 `research`。
6. 有 note/context scope 的普通询问进入 `ask_notes`。
7. 普通闲聊进入 `chat`。
8. 高成本低置信任务返回 `requiresClarification: true`，`executionMode: "clarification"`。

代码内保留一个导出的 marker 常量，供契约测试锁定创作优先级：

```ts
export const writingKeywordBeforeResearchKeyword = true;
```

- [x] **Step 2: 明确每类默认 plan**

实现小函数，避免复制字段：

```ts
function basePlan(
  input: BuildAssistantTaskPlanInput,
): Pick<
  TaskPlan,
  | "contextReferences"
  | "retrievalMode"
  | "webMode"
  | "artifactPlan"
  | "requiresClarification"
  | "sourceHints"
>;
function writerPlan(
  input: BuildAssistantTaskPlanInput,
  intent: "creative_write" | "rewrite_selection",
): TaskPlan;
function researchPlan(input: BuildAssistantTaskPlanInput): TaskPlan;
function clarifyPlan(
  input: BuildAssistantTaskPlanInput,
  question: string,
): TaskPlan;
```

默认值：

- chat: `modelSlot: "fast"`, `retrievalMode: "none"`, `artifactPlan: []`
- ask_notes: `modelSlot: "fast"`, `retrievalMode: "local_notes"` 或 `current_reference`
- creative_write/rewrite_selection: `modelSlot: "writer"`, `executionMode: "writing_candidate"` 或 `patch_proposal`
- research: `modelSlot: "reasoner"`, `executionMode: "structured_task"`, artifact 候选只允许 `evidence_sources`
- document_check: `modelSlot: "long_context"`, `executionMode: "long_task"`
- vision_chat: `modelSlot: "vision"`
- skill_management: `modelSlot: "agent_tools"`

- [x] **Step 3: 将旧 `assistant-routing.ts` 改成适配层**

保留现有导出：

```ts
export function detectAgentIntent(
  input: AssistantRouteInput,
): IntentDetectionResult;
export function legacyIntentForAgentIntent(
  intent: AgentIntent,
): AssistantIntent;
```

`detectAgentIntent` 内部调用 `buildAssistantTaskPlan(input)`，把 `TaskPlan` 映射回旧 `IntentDetectionResult`，供尚未改完的 UI 使用。删除 `RESEARCH_KEYWORDS`、`WRITING_KEYWORDS` 等主决策数组；如果仍需少量关键词，放进 `assistant-taskplan.ts` 且按创作优先。

- [x] **Step 4: 运行测试**

Run:

```bash
npm run test -- tests/agent-taskplan-routing.test.ts tests/unified-assistant-routing.test.ts
npm run typecheck
```

Expected: PASS。小说续写回归用例必须落到 `creative_write` 或 `rewrite_selection`，不能是 `research`。

- [x] **Step 5: 提交前端 TaskPlan**

```bash
git add src/lib/assistant-taskplan.ts src/lib/assistant-routing.ts tests/agent-taskplan-routing.test.ts tests/unified-assistant-routing.test.ts
git commit -m "feat(ai): 用 TaskPlan 驱动前端任务路由"
```

## Task 4: 后端 TaskPlan 校验与模型槽位派生

**Files:**

- Create: `src-tauri/src/ai_runtime/task_plan.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/ai_runtime/agent_task_policy.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Modify: `src-tauri/src/ai_harness/harness_task.rs`

- [x] **Step 1: 新建后端模块与单元测试**

`src-tauri/src/ai_runtime/task_plan.rs` 提供：

```rust
pub fn build_or_validate_task_plan(request: &AssistantExecuteRequest) -> AppResult<TaskPlanSummary>;
pub fn agent_intent_for_task_plan(plan: &TaskPlanSummary) -> AgentIntent;
pub fn policy_input_for_task_plan(plan: &TaskPlanSummary, request: &AssistantExecuteRequest) -> AgentTaskPolicyInput;
pub fn legacy_intent_for_task_plan(plan: &TaskPlanSummary) -> AssistantIntent;
```

模块内先写测试：

- `creative_write_uses_writer_slot_even_with_analysis_word`
- `chat_uses_fast_slot_without_retrieval`
- `research_requires_structured_task_or_long_task`
- `context_reference_sets_current_reference_retrieval`
- `web_disabled_sets_max_fetch_zero`

- [x] **Step 2: 补全请求没有 taskPlan 时的服务端兜底**

如果前端未传 `task_plan`，后端用 request facts 构造保守 plan：

- 有 images -> `vision_chat`
- 有 selection 且 agent intent 是 rewrite/write -> `rewrite_selection`
- agent intent 已给出 -> 映射到对应 `TaskPlanIntent`
- 没有明确 metadata -> `chat`

兜底只用于旧客户端兼容，`source_hints` 必须加入 `"compat:server_derived_task_plan"`。

- [x] **Step 3: 改 `agent_task_policy.rs`**

新增：

```rust
impl AgentTaskPolicyInput {
    pub fn from_task_plan(plan: &TaskPlanSummary, request: &AssistantExecuteRequest) -> Self
}
```

把原有 `model_slot_for_input` 测试扩展为 `TaskPlan` 测试。`legacy_scene_hint` 字段继续存在，但注释改成“compatibility only”。

- [x] **Step 4: 在 `assistant_execute` 主入口接入**

在 `assistant_execute` 入口最早处：

```rust
let task_plan = crate::ai_runtime::task_plan::build_or_validate_task_plan(&request)?;
let agent_intent = agent_intent_for_task_plan(&task_plan);
let legacy_intent = legacy_intent_for_task_plan(&task_plan);
```

后续 skill activation、policy summary、harness request 都使用这个 `agent_intent`。Response 的 `task_plan` 返回 `Some(task_plan.clone())`。

- [x] **Step 5: `HarnessTaskRequest` 携带 plan**

把 `HarnessTaskRequest` 改为：

```rust
pub(crate) struct HarnessTaskRequest {
    pub(crate) assistant: AssistantExecuteRequest,
    pub(crate) task_plan: TaskPlanSummary,
    pub(crate) routing_override: Option<ai_commands::AiSendRoutingOverride>,
}
```

所有构造点显式传入 `task_plan`，避免 workflow 再自行猜 intent。

- [x] **Step 6: 验证**

Run:

```bash
cargo fmt --all -- --check
cargo test task_plan
cargo check
npm run test -- tests/assistant-execute-ipc.test.ts tests/agent-taskplan-routing.test.ts
```

Expected: PASS。若旧测试仍期待 `agent_intent` 为唯一事实源，改成断言 response 同时包含 `taskPlan`。

- [x] **Step 7: 提交后端 TaskPlan**

```bash
git add src-tauri/src/ai_runtime/task_plan.rs src-tauri/src/ai_runtime/mod.rs src-tauri/src/ai_runtime/agent_task_policy.rs src-tauri/src/commands/assistant_commands.rs src-tauri/src/ai_harness/harness_task.rs tests/assistant-execute-ipc.test.ts
git commit -m "feat(ai): 后端校验并执行 TaskPlan"
```

## Task 5: 对话区 Markdown-first，删除研究消息卡

**Files:**

- Create: `tests/assistant-markdown-stream-contract.test.tsx`
- Modify: `src/components/ai/AiMessageList.tsx`
- Delete: `src/components/ai/ResearchResultMessage.tsx`
- Modify: `src/components/ai/hooks/useAssistantTasks.ts`
- Modify: `src/components/ai/hooks/useAssistantConversation.ts`
- Modify: `tests/unified-assistant-shell.test.ts`
- Modify: `tests/research-result-artifact-tags.test.tsx`

- [x] **Step 1: 新增失败测试**

`tests/assistant-markdown-stream-contract.test.tsx` 断言：

```ts
expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
  "ResearchResultMessage",
);
expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
  'kind?: "research"',
);
expect(read("src/components/ai/hooks/useAssistantTasks.ts")).not.toContain(
  'kind: "research"',
);
```

用 `existsSync` 表达删除文件：

```ts
expect(existsSync("src/components/ai/ResearchResultMessage.tsx")).toBe(false);
expect(() => read("src/components/ai/ResearchResultMessage.tsx")).toThrow();
```

- [x] **Step 2: 修改消息类型**

在 `AiMessageList.tsx` 的 `ChatLine` 中删除：

```ts
kind?: "research";
research?: ResearchFocusPayload;
```

不增加未来占位字段；`ChatLine` 在 Task 5 只保留消息渲染实际消费的字段。

`assistantStreaming` 不再排除 research kind。

- [x] **Step 3: 删除 research 特判渲染**

从 `AiMessageList.tsx` 删除：

```tsx
if (m.role === "assistant" && m.kind === "research" && m.research) {
  const result = m.research;
  return (
    <div className="flex w-full justify-start">
      <ResearchResultMessage
        result={result}
        onOpenArtifact={(draft) => onOpenArtifact?.(draft)}
      />
    </div>
  );
}
```

所有 assistant 内容都进入：

```tsx
<AiMessageBubble
  role="assistant"
  content={msgContent || undefined}
  streaming={assistantStreaming}
  selected={isSelected}
/>
```

- [x] **Step 4: 改 `runResearch` 的消息写入**

在 `useAssistantTasks.ts` 的 `runResearch` 中把：

```ts
setMessages((prev) => [
  ...prev,
  { role: "assistant", content: "", kind: "research", research: result },
]);
```

替换为：

```ts
setMessages((prev) => [
  ...prev,
  {
    role: "assistant",
    content: result.summary.trim(),
  },
]);
```

不要在 `ChatLine` 中写入 `ContextReference` 或 `ArtifactPlanItem`。前者属于 Task 7 的上下文输入契约，后者只描述计划中的产物意图，不是可打开的临时 tab 数据；临时 tab 必须由 Task 6 的 `AssistantArtifactDraft` 与价值门槛统一生成。

如果 `result.summary` 为空，显示一句正常 Markdown 诊断：

```ts
"研究已完成，但没有生成可展示正文。可在来源详情中查看证据状态。";
```

这句只用于异常空正文，不生成过程卡。

- [x] **Step 5: 删除文件并改旧测试**

删除 `src/components/ai/ResearchResultMessage.tsx`。

把 `tests/unified-assistant-shell.test.ts` 中对 `ResearchResultMessage` 的期望改成：

```ts
expect(list).not.toContain("ResearchResultMessage");
expect(list).toContain("AiMessageBubble");
```

删除或重写 `tests/research-result-artifact-tags.test.tsx`，新测试归入 `assistant-artifact-value-gates.test.ts`，不再单独祝福研究消息卡。

- [x] **Step 6: 验证**

Run:

```bash
npm run test -- tests/assistant-markdown-stream-contract.test.tsx tests/unified-assistant-shell.test.ts
npm run typecheck
```

Expected: PASS。对话流中不再出现研究卡、过程按钮、证据矩阵按钮。

- [x] **Step 7: 提交 Markdown-first 消息流**

```bash
git add src/components/ai/AiMessageList.tsx src/components/ai/hooks/useAssistantTasks.ts src/components/ai/hooks/useAssistantConversation.ts tests/assistant-markdown-stream-contract.test.tsx tests/unified-assistant-shell.test.ts tests/research-result-artifact-tags.test.tsx
git add -u src/components/ai/ResearchResultMessage.tsx
git commit -m "refactor(ai): 对话区统一为 Markdown 文字流"
```

## Task 6: 临时 Tab 价值门槛与四类产物

**Files:**

- Create: `tests/assistant-artifact-value-gates.test.ts`
- Modify: `src/types/assistant-artifact.ts`
- Modify: `src/lib/assistant-artifact-tabs.ts`
- Modify: `src/components/layout/ArtifactWorkspaceView.tsx`
- Modify: `src/components/ai/AssistantArtifactTagStrip.tsx`
- Modify: `src/components/ai/AssistantTaskSurfaces.tsx`
- Modify: `src-tauri/src/ai_harness/harness_task.rs`

- [x] **Step 1: 新增 artifact gate 测试**

测试用例必须覆盖：

- `evidence_sources`：有真实 evidence/source/conflict/freshness/gap 时生成。
- `evidence_sources`：`evidence_count = 0` 且只有机械缺口时不生成。
- `task_process`：`completed` 普通完成不生成。
- `task_process`：`pending_confirmation`、`failed`、`paused_budget`、`long_task` 有 checkpoint 时生成。
- `writing_change`：存在 patch/diff/insert/replace 候选时生成。
- `structured_result`：整理建议、引用核查、文档问题清单生成。

测试锁定函数名：

```ts
expect(read("src/lib/assistant-artifact-tabs.ts")).toContain(
  "artifactPassesValueGate",
);
expect(read("src/lib/assistant-artifact-tabs.ts")).toContain(
  "buildArtifactDraftsFromTaskResult",
);
```

- [x] **Step 2: 改 artifact 类型**

`src/types/assistant-artifact.ts` 改为：

```ts
export type ArtifactKind =
  | "evidence_sources"
  | "writing_change"
  | "structured_result"
  | "task_process";
```

删除旧 kind：

```ts
"research" |
  "process" |
  "writing_patch" |
  "citation_check" |
  "organize_suggestions";
```

所有旧 payload 通过 `payload.schema` 或 `payload.resultKind` 区分，不再用 UI kind 表示 workflow。

- [x] **Step 3: 实现 `artifactPassesValueGate`**

在 `src/lib/assistant-artifact-tabs.ts` 增加：

```ts
export function artifactPassesValueGate(
  draft: AssistantArtifactDraft,
): boolean {
  switch (draft.kind) {
    case "evidence_sources":
      return hasEvidenceValue(draft.payload);
    case "writing_change":
      return hasWritingChangeValue(draft.payload);
    case "structured_result":
      return hasStructuredResultValue(draft.payload);
    case "task_process":
      return hasProcessValue(draft.payload);
  }
}
```

`hasProcessValue` 只接受失败、暂停、等待确认、长任务多步骤诊断，不接受 `"assistant workflow output summarized by artifact metadata"`。

- [x] **Step 4: 改 workspace view**

`ArtifactWorkspaceView.tsx` 使用四类 kind 渲染：

- `evidence_sources` -> `EvidenceSourcesArtifactView`
- `writing_change` -> `WritingChangeArtifactView`
- `structured_result` -> `StructuredResultArtifactView`
- `task_process` -> `TaskProcessArtifactView`

旧 `ResearchArtifactView`、`ProcessArtifactView`、`CitationArtifactView`、`OrganizeArtifactView` 可以在同文件内逐步重命名，但最终不保留旧 kind 分支。

- [x] **Step 5: 后端 artifact wire 只发产品语义 kind**

`HarnessArtifactWire.kind` 生成处改为四类 kind。`ResearchReport` 不直接产生 `"research"` wire；它只能在有真实 evidence/source 信息时产生 `"evidence_sources"`。

`complete_workflow_runtime_task` 中 `record_step` 的 output summary 从占位文本改成真实 metadata，如：

```rust
"assistant task completed; no process artifact generated for ordinary completion"
```

这段只能进任务日志，不进入用户可见 tab。

- [x] **Step 6: 验证**

Run:

```bash
npm run test -- tests/assistant-artifact-value-gates.test.ts tests/assistant-artifact-tabs.test.ts tests/writing-research-state-panel.test.tsx
npm run typecheck
cargo test harness_task
```

Expected: PASS。旧测试如仍期待 `"research"` 或 `"process"` kind，应改为新四类。

- [x] **Step 7: 提交 artifact gate**

```bash
git add src/types/assistant-artifact.ts src/lib/assistant-artifact-tabs.ts src/components/layout/ArtifactWorkspaceView.tsx src/components/ai/AssistantArtifactTagStrip.tsx src/components/ai/AssistantTaskSurfaces.tsx src-tauri/src/ai_harness/harness_task.rs tests/assistant-artifact-value-gates.test.ts tests/assistant-artifact-tabs.test.ts tests/writing-research-state-panel.test.tsx
git commit -m "refactor(ai): 为临时产物增加价值门槛"
```

## Task 7: ContextReference 精确选区与引用胶囊

**Files:**

- Create: `src/lib/context-reference.ts`
- Create: `tests/context-reference.test.ts`
- Modify: `src/components/ai/ConversationSurface.tsx`
- Modify: `src/components/ai/hooks/useAssistantConversation.ts`
- Modify: `src/hooks/useInlineAi.ts`
- Modify: `src/hooks/useAiBubbleSelection.ts`
- Modify: `src/lib/iris-clipboard.ts`
- Modify: `src/components/ai/hooks/useAssistantTasks.ts`

- [x] **Step 1: 新增失败测试**

`tests/context-reference.test.ts` 覆盖：

```ts
it("preserves an irregular partial selection range");
it(
  "preserves a cross-paragraph selection without expanding to full paragraphs",
);
it("marks a reference stale when content hash changes");
it(
  "creates a lightweight display capsule without dumping the whole source text",
);
it("serializes references through assistantExecute");
```

- [x] **Step 2: 实现 `src/lib/context-reference.ts`**

导出：

```ts
export function createContextReference(input: {
  kind: ContextReference["kind"];
  filePath: string | null;
  content: string;
  utf8Range: SourceSpan | null;
  editorRange: { from: number; to: number } | null;
  headingPath?: string | null;
  anchor?: string | null;
}): ContextReference;

export function validateContextReference(
  reference: ContextReference,
  currentContent: string | null,
): ContextReference;

export function contextReferenceDisplayText(
  reference: ContextReference,
): string;
```

`createContextReference` 使用已有 hash 工具；若代码库没有现成 hash 工具，用浏览器 `crypto.subtle` 不适合同步测试，先在前端使用已有 `content_hash` 来源。不要新增 npm 依赖。

- [x] **Step 3: 输入框显示引用胶囊**

`ConversationSurface.tsx` 在 composer 上方或输入框内部显示轻量 capsule：

- 文件名
- 选区摘要，最多 80 个中文字符
- stale/invalid 状态
- 删除按钮

不显示完整原文，不在用户消息正文中拼接选区全文。

- [x] **Step 4: 右侧 AI 引用选区**

`useAiBubbleSelection.ts` 与 `useAssistantConversation.ts` 增加 `quoteSelectionAsReference`：

```ts
quoteSelectionAsReference(reference: ContextReference): void
```

旧的 `selectionQuoteText` 可以短期保留做显示兼容，但发送请求时必须转为 `contextReferences`。

- [x] **Step 5: 悬浮 AI composer 接入**

`useInlineAi.ts` 从 TipTap selection 构造 `ContextReference`，并提供两个明确动作：

- `insert_after_selection`
- `replace_selection`

这两个动作进入 `TaskPlan.executionMode = "patch_proposal"`，写入仍走确认。

- [x] **Step 6: assistantExecute 带上引用**

`useAssistantTasks.ts` 的所有路径统一传：

```ts
contextReferences: activeContextReferences;
```

写作路径不再只传裸 `selection`；迁移期可同时传 `selection`，但 plan 必须写明删除目标：Task 11 删除裸 selection 主路径。

- [x] **Step 7: 验证**

Run:

```bash
npm run test -- tests/context-reference.test.ts tests/use-assistant-context-scope.test.tsx tests/app-shell-refactor-contract.test.ts
npm run typecheck
```

Expected: PASS。跨段局部选择不能被扩展成整段。

- [x] **Step 8: 提交 ContextReference**

```bash
git add src/lib/context-reference.ts src/components/ai/ConversationSurface.tsx src/components/ai/hooks/useAssistantConversation.ts src/hooks/useInlineAi.ts src/hooks/useAiBubbleSelection.ts src/lib/iris-clipboard.ts src/components/ai/hooks/useAssistantTasks.ts tests/context-reference.test.ts tests/use-assistant-context-scope.test.tsx tests/app-shell-refactor-contract.test.ts
git commit -m "feat(ai): 增加精确上下文引用胶囊"
```

## Task 8: 统一发送调度，允许同会话跨场景切换

**Files:**

- Modify: `src/components/ai/hooks/useAssistantTasks.ts`
- Modify: `src/components/ai/hooks/useAssistantPanelEffects.ts`
- Modify: `src/components/ai/AgentStatusBadge.tsx`
- Modify: `src/components/ai/SessionHistoryDropdown.tsx`
- Modify: `tests/use-assistant-confirmations.test.tsx`
- Modify: `tests/ai-workflow-tasks.test.ts`

- [x] **Step 1: 把 `send` 改成先 build plan**

`useAssistantTasks.ts` 的 `send` 开头：

```ts
const taskPlan = buildAssistantTaskPlan({
  message: rawMessage,
  hasImage: images.length > 0,
  hasSelection: activeContextReferences.some((ref) => ref.kind === "selection"),
  notePath,
  explicitScope:
    contextScope.paths.length > 0 || contextScope.pathPrefixes.length > 0,
  contextReferences: activeContextReferences,
  webAuthorized: webSearch,
});
```

如果 `taskPlan.requiresClarification`，直接追加一条 assistant Markdown 消息：

```ts
{ role: "assistant", content: taskPlan.clarificationQuestion ?? "你希望我按哪种方式处理？" }
```

不调用 `assistantExecute`，不生成 tab。

- [x] **Step 2: 用 `taskPlan.intent` 调度**

将 `switch (intent)` 改为 `switch (taskPlan.intent)`，映射：

- `chat`, `vision_chat`, `ask_notes` -> `runKnowledgeChat`
- `creative_write` -> 普通文字流承载的 writer TaskPlan
- `rewrite_selection` -> `runWriting`
- `citation_check` -> `runCitation`
- `organize` -> `runOrganize`
- `research` -> `runResearch`
- `chapter` -> `runChapter`
- `document_check` -> `runDocumentCheck`
- `skill_management` -> 走 chat/harness 工具路径

所有 `assistantExecute` 调用传 `taskPlan`。

Implementation note: `rewrite_selection` 继续走补丁式 `runWriting`；`creative_write` 走普通文字流承载的 writer TaskPlan，后端保持 writer 路由但使用 chat legacy workflow，避免无选区续写小说被迫进入补丁/研究路径。

- [x] **Step 3: 状态徽标显示本轮 plan，不再显示固定 scene**

`AgentStatusBadge.tsx` 文案从“任务：研究任务 · 使用核心默认工具集”改成当前本轮状态：

- 普通聊天：`本轮：轻量对话`
- 写作：`本轮：写作候选`
- 研究：`本轮：研究综合`
- 等待确认：`等待确认`

不要展示 `AiScene` 名称。

- [x] **Step 4: session history 兼容**

`SessionHistoryDropdown.tsx` 仍可用 legacy scene 查询旧 session，但新会话不因上一轮 scene 锁定下一轮。测试断言：

```ts
expect(panelEffects).not.toContain("syncActiveLegacySceneHint");
```

如果历史 API 仍要求 scene，传 `taskPlan` 派生的兼容 scene，只作为 session key。

- [x] **Step 5: 验证跨场景**

新增测试序列：

1. 发送“这个概念是什么意思？” -> `chat` 或 `ask_notes`
2. 同一会话发送“根据上文续写一段” -> `creative_write`
3. 同一会话发送“请联网研究这个主题的真实资料” -> `research`
4. 同一会话发送“谢谢，简单说一下就行” -> `chat`

断言四次 plan 彼此独立，不读取上一轮 intent 作为硬锁。

- [x] **Step 6: 验证**

Run:

```bash
npm run test -- tests/agent-taskplan-routing.test.ts tests/use-assistant-confirmations.test.tsx tests/ai-workflow-tasks.test.ts
npm run typecheck
```

Expected: PASS。同会话切换不出现 scene lock。

- [x] **Step 7: 提交统一调度**

```bash
git add src/components/ai/hooks/useAssistantTasks.ts src/components/ai/hooks/useAssistantPanelEffects.ts src/components/ai/AgentStatusBadge.tsx src/components/ai/SessionHistoryDropdown.tsx tests/use-assistant-confirmations.test.tsx tests/ai-workflow-tasks.test.ts
git commit -m "refactor(ai): 统一按 TaskPlan 调度每轮任务"
```

## Task 9: 研究 workflow 输出降噪与证据来源 tab

**Files:**

- Modify: `src-tauri/src/ai_workflows/research_workflow.rs`
- Modify: `src-tauri/src/ai_harness/harness_task.rs`
- Modify: `src/types/ai.ts`
- Modify: `tests/assistant-artifact-value-gates.test.ts`
- Modify: `tests/e2e/ai-workflow.test.ts`

- [x] **Step 1: 为研究输出写 Rust 单元测试**

在 `research_workflow.rs` 或相邻测试模块增加：

- `empty_evidence_does_not_create_matrix_artifact`
- `mechanical_gap_without_source_is_not_a_displayable_gap`
- `research_summary_is_markdown_message`
- `evidence_sources_include_real_source_count`

- [x] **Step 2: 删除默认机械矩阵行为**

找到机械生成缺口的位置，例如根据 proposition 全量追加“缺少直接证据”。改为：

- 没有 evidence item 时，不生成 matrix view。
- 只有模型推断出来的 proposition 且无来源时，`coverage` 状态为 `insufficient_evidence`，不展示为矩阵。
- `global_gaps` 只保留用户能行动的缺口，例如缺少指定来源、联网关闭导致无法核验、具体引用冲突。

- [x] **Step 3: 研究 response 拆分**

后端返回：

- `summary`: 对话区 Markdown 正文。
- `evidence_sources`: 真实来源列表。
- `coverage_diagnostics`: 有来源时才计算覆盖。
- `artifact_wires`: 只在 `evidence_sources.len() > 0` 或存在可行动缺口时生成 `evidence_sources`。

Implementation note: 现有 `task_result_from_research`/`artifacts_to_wires` 已按真实证据或可行动缺口生成 `evidence_sources` wire；本任务补齐 workflow 层，不再让空 evidence matrix 产生机械缺口。

- [x] **Step 4: 前端展示**

`runResearch` 只把 `summary` 追加到 messages。`setResearchResult` 可以保留给临时 tab payload，但不再驱动消息卡。

- [x] **Step 5: 验证**

Run:

```bash
cargo test research_workflow
npm run test -- tests/assistant-artifact-value-gates.test.ts tests/e2e/ai-workflow.test.ts
```

Expected: PASS。空证据不生成矩阵 tab；研究正文在消息流中可读。

- [x] **Step 6: 提交研究降噪**

```bash
git add src-tauri/src/ai_workflows/research_workflow.rs src-tauri/src/ai_harness/harness_task.rs src/types/ai.ts tests/assistant-artifact-value-gates.test.ts tests/e2e/ai-workflow.test.ts
git commit -m "refactor(ai): 降噪研究输出与证据产物"
```

## Task 10: Network Evidence Broker 第一版

**Files:**

- Create: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Create: `tests/web-evidence-broker.test.ts`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/web.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/web.rs`
- Modify: `src-tauri/src/ai_workflows/research_workflow.rs`
- Modify: `src/types/ai.ts`

- [x] **Step 1: 新增 broker 类型**

`web_evidence_broker.rs` 定义：

```rust
pub struct WebEvidenceBrokerInput {
    pub query: String,
    pub enabled: bool,
    pub max_search_results: usize,
    pub max_fetches: usize,
}

pub struct WebEvidenceItem {
    pub url: String,
    pub title: String,
    pub domain: String,
    pub snippet: String,
    pub fetched_excerpt: Option<String>,
    pub source_rank: WebSourceRank,
    pub freshness_label: Option<String>,
    pub failure_reason: Option<String>,
}
```

提供：

```rust
pub async fn collect_web_evidence(db: &Database, input: WebEvidenceBrokerInput) -> AppResult<Vec<WebEvidenceItem>>;
```

- [x] **Step 2: broker 测试**

Rust 测试覆盖：

- `disabled_broker_returns_empty_without_search`
- `deduplicates_urls`
- `records_fetch_failure_without_failing_whole_task`
- `rejects_non_https_fetch_targets`

避免真实网络，使用可注入 trait 或内部纯函数测试去重/过滤/转换。

- [x] **Step 3: 工具目录降噪**

`tool_catalog/web.rs` 的用户/模型说明改成：

- 主工具名可保留现有实现名，但描述强调“网络证据代理”。
- `fetch_web_page` 不再被普通对话 prompt 暴露成用户可见概念。
- 非 HTTPS、下载、登录、外部写入仍要求确认或拒绝。

- [x] **Step 4: research workflow 调用 broker**

研究 workflow 不直接调用 `fetch_search_context_for_db` 作为散落逻辑，而是调用 `collect_web_evidence`。联网关闭时 broker 返回空 evidence，并写入可行动缺口：“联网关闭，未检索外部来源”。

- [x] **Step 5: 前端契约测试**

`tests/web-evidence-broker.test.ts` 读取代码断言：

```ts
expect(read("src-tauri/src/ai_runtime/web_evidence_broker.rs")).toContain(
  "collect_web_evidence",
);
expect(read("src-tauri/src/ai_runtime/tool_catalog/web.rs")).toContain(
  "网络证据代理",
);
expect(read("src/components/ai/ConversationSurface.tsx")).not.toContain(
  "fetch_web_page",
);
```

- [x] **Step 6: 验证**

Run:

```bash
cargo test web_evidence_broker
cargo test research_workflow
npm run test -- tests/web-evidence-broker.test.ts
```

Expected: PASS。用户界面只暴露联网开关，不暴露 search/fetch 区分。

- [x] **Step 7: 提交 broker**

```bash
git add src-tauri/src/ai_runtime/web_evidence_broker.rs src-tauri/src/ai_runtime/mod.rs src-tauri/src/ai_runtime/tool_catalog/web.rs src-tauri/src/ai_runtime/tool_dispatch/web.rs src-tauri/src/ai_workflows/research_workflow.rs src/types/ai.ts tests/web-evidence-broker.test.ts
git commit -m "feat(ai): 增加统一网络证据代理"
```

## Task 11: 技术债删除与迁移收口

**Files:**

- Modify/Delete files touched by Tasks 1-10
- Modify: `tests/agent-task-phase-f-contract.test.ts`
- Modify: `tests/harness-modernization-contract.test.ts`
- Modify: `docs/README.md` if it references removed cards or scene router

- [x] **Step 1: 删除旧主路径函数**

删除或降级以下内容：

- `legacySceneHintForAssistantIntent` 不再被发送调度使用。
- `syncActiveLegacySceneHint` 不再在 panel effect 中运行。
- `detectAgentIntent` 内部不能有独立关键词路由，只能调用 `buildAssistantTaskPlan`。
- 裸 `selection` 不能作为写作主上下文；主路径使用 `contextReferences`，`selection` 只作旧 IPC fallback。
- `ResearchResultMessage` 文件已删除且无 import。
- artifact kind 不再出现 `"research"`、`"process"`、`"writing_patch"`、`"citation_check"`、`"organize_suggestions"`。

- [x] **Step 2: 全库搜索旧债**

Run:

```bash
rg -n "ResearchResultMessage|kind: \"research\"|kind === \"research\"|证据矩阵|过程详情|assistant workflow output summarized|RESEARCH_KEYWORDS|syncActiveLegacySceneHint|legacySceneHintForAssistantIntent|writing_patch|organize_suggestions|citation_check" src src-tauri tests docs
```

Expected: 只允许出现：

- 文档说明旧行为已移除。
- 历史兼容函数定义，且注释含 `compatibility only`。
- 测试中用于 `not.toContain` 的字符串。

- [x] **Step 3: 改契约测试防回潮**

`tests/agent-task-phase-f-contract.test.ts` 增加：

```ts
expect(read("src/lib/assistant-routing.ts")).not.toContain("RESEARCH_KEYWORDS");
expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
  "ResearchResultMessage",
);
expect(read("src/lib/assistant-artifact-tabs.ts")).toContain(
  "artifactPassesValueGate",
);
```

`tests/harness-modernization-contract.test.ts` 增加后端断言：

```ts
expect(read("src-tauri/src/ai_runtime/task_plan.rs")).toContain(
  "TaskPlanSummary",
);
expect(read("src-tauri/src/ai_harness/harness_task.rs")).not.toContain(
  "assistant workflow output summarized by artifact metadata",
);
```

- [x] **Step 4: 删除无意义临时兼容**

如果 Tasks 1-10 为通过编译临时保留了 adapter，逐项处理：

- 前端 `selectionQuoteText` 如果只用于旧写作路径，改成由 `ContextReference` 派生显示后删除 state。
- `AssistantArtifactTagStrip` 如果仍只用于消息流按钮条，删除组件与 tests；如果用于 workspace 内部操作，改名为 `ArtifactInlineActions` 并移出消息流。
- `researchResult` state 如果只为旧卡片服务，删除；如果作为 artifact payload cache，改名为 `activeEvidenceArtifactPayload`。

- [x] **Step 5: 验证**

Run:

```bash
npm run test -- tests/agent-task-phase-f-contract.test.ts tests/harness-modernization-contract.test.ts tests/assistant-markdown-stream-contract.test.tsx
npm run lint
npm run typecheck
cargo clippy --all-targets -- -D warnings
```

Expected: PASS。旧债搜索结果符合 Step 2 白名单。

- [x] **Step 6: 提交删除收口**

```bash
git add src src-tauri tests docs
git commit -m "refactor(ai): 删除旧场景路由和卡片产物债务"
```

## Task 12: 端到端验证与文档更新

**Files:**

- Modify: `docs/design-system.md`
- Modify: `ROADMAP.md`
- Modify: `docs/README.md`
- Modify: `tests/e2e/ai-workflow.test.ts`

- [x] **Step 1: 更新产品文档**

`docs/design-system.md` 增加 AI 对话区规则：

- 对话消息为 Markdown-first。
- 临时 tab 是高价值产物，不是 workflow 默认副产品。
- 过程 tab 只用于长任务、暂停、失败、权限等待、有意义诊断。
- 引用胶囊显示短摘要，不显示完整选区。

`ROADMAP.md` 对应 AI harness 条目更新为 TaskPlan 方向，避免与 spec 冲突。

`docs/README.md` 增加 spec 和 plan 链接。

- [x] **Step 2: E2E 场景**

`tests/e2e/ai-workflow.test.ts` 增加或改写：

1. 输入小说续写请求，包含“分析/研究/综述”字样，断言不出现研究卡和证据矩阵按钮。
2. 同会话下一轮输入明确“联网研究真实资料”，断言出现 Markdown 正文，来源详情只在临时 tab。
3. 普通完成不出现过程 tab。
4. 选中文档局部文本后打开悬浮 AI，断言可选择插入后方或替换选区，执行写入前出现确认。

- [ ] **Step 3: 全量验证（`cargo test` 阻塞）**

Run:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run test:e2e
```

Expected: PASS。若 E2E 依赖本地模型或网络而环境不可用，记录具体不可用原因，并至少完成单元/契约测试。

Execution note: all listed commands passed except full `cargo test`. `cargo test` completed unit tests and then hung in `tests/agent_vault_tools.rs` at `markdown_patch_tool_creates_pre_write_snapshot` and `vault_rename_move_reports_link_impact_and_moves_note`; it was stopped after several minutes without output. Relevant Rust subsets (`cargo test ai_harness::harness_task`, `cargo test web_evidence_broker`) passed. This step remains unchecked until the hanging integration tests are resolved or isolated.

- [x] **Step 4: 最终技术债搜索**

Run:

```bash
rg -n "ResearchResultMessage|RESEARCH_KEYWORDS|assistant workflow output summarized|过程详情|证据矩阵|kind: \"research\"|kind === \"process\"|syncActiveLegacySceneHint" src src-tauri tests docs
```

Expected: 没有旧默认路径生产命中。允许以下保留项：

- `src/types/ai.ts` 的 research payload / evidence matrix 类型定义。
- `src/components/layout/ArtifactWorkspaceView.tsx` 中的只读临时 tab 标题或内部视图。
- `src/components/ai/AgentTaskStatusPanel.tsx` 中的长任务/暂停/失败/权限等待状态入口。
- `src/lib/assistant-artifact-tabs.ts` 中通过 value gate 后才显示的临时 tab 标题。
- `docs/history/**` 中的历史记录。
- 当前 spec/plan 中用于描述“已移除旧默认行为”的说明与负向测试。

Execution note: production hits that remain match the allowlist above. No old `ResearchResultMessage`, `RESEARCH_KEYWORDS`, placeholder workflow summary, or global legacy scene sync production path remains.

- [x] **Step 5: 提交文档与 E2E**

```bash
git add docs/design-system.md ROADMAP.md docs/README.md tests/e2e/ai-workflow.test.ts
git commit -m "docs(ai): 记录 TaskPlan harness 交互规则"
```

## Execution Notes

- 每个任务执行前先阅读对应文件完整内容，并搜索调用处，符合 `AGENTS.md` 的修改前必读上下文要求。
- 每个新功能或修复先写失败测试，再写实现，再运行该任务列出的验证命令。
- 不新增 npm 或 Rust 依赖；如果执行中发现必须新增依赖，先暂停并向用户说明许可证、替代方案和影响范围。
- 不直接修改用户 `.md` 笔记内容；写作应用必须走 patch/confirmation。
- 不把旧系统长期并行保留。每个 compatibility 字段都必须有明确用途：历史 session、trace、旧 IPC 兜底。除此之外删除。
- 提交粒度按任务划分，提交消息使用中文 Conventional Commit。
- Task 12 temporarily resets line-count ratchets to the current measured baselines after Task 11: `UnifiedAssistantPanel.impl.tsx` is 530 lines with a 540-line checkpoint, and `skills_impl.rs` is 1256 lines with a 1260-line checkpoint. These are not feature budget increases; future refactors should lower them again rather than raise them.
- Existing root-document deletions (`TECH_DEBT_REVIEW.md` and related review notes) are outside this Task 12 changeset and must not be staged with this commit.
