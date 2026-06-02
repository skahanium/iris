# Iris AI Harness 现代化整合计划

## Summary

目标是把现有 `harness + workflow + tool + frontend assistant UI` 从“双轨拼装”整理成一个统一、健康、可演进的 AI Runtime 与交互体系：**后端以 harness 作为统一执行内核，workflow 变成 typed task/adapters，tool 变成单一契约的能力层；前端以统一任务状态、证据、工具确认、artifact 展示来承接所有 AI 能力**。

采用分阶段迁移，保持现有功能不变，不一次性推倒重写。最终用户看到的是一个更顺、更可信的 AI 助手：知道自己在检索、等待确认、生成报告、产出补丁、引用了哪些证据；开发者看到的是一套一致的 request/result/tool/trace/artifact 契约。

## Key Changes

### 1. 统一后端运行模型

- 保留 `assistant_execute` 作为前端唯一入口，内部新增统一 `HarnessTaskRequest` / `HarnessTaskResult`。
- `chat` / `knowledge` 继续走 agent loop。
- `writing` / `citation` / `organize` / `research` / `chapter` / `document` 先通过 adapter 接入统一任务模型，保留现有核心能力。
- 所有任务统一输出 `HarnessArtifact`，例如 `message`、`patches`、`citation_report`、`research_report`、`document_check`、`tool_confirmation`。

### 2. Harness 内核职责收束

- 将 `run_harness` 内部拆成阶段：context、planning、tool execution、confirmation、reflection、final streaming、archive。
- 保持 `run_harness` 外部调用兼容，但内部不再把 prompt、工具、checkpoint、trace、final streaming 全塞在一个大函数里。
- 所有 task adapter 都复用统一 trace、session、evidence、model gateway、artifact 输出规则。

### 3. Tool 体系单一事实源

- 合并 `ToolRegistry` 和 `tool_dispatch` 的概念：暴露给模型的工具必须有真实 handler。
- 未实现的写入工具不再暴露给模型。
- 写入类能力统一产出 `PatchProposal` 或 artifact，不直接修改 `.md`。
- `web_search` 继续只读自动执行，`fetch_web_page` 和写入/设置类工具继续要求用户确认。
- `ai_list_tools` 只返回真实可执行工具。

### 4. Confirmation 闭环修复

- harness 遇到需要确认的工具时保存 checkpoint，返回 `pending_confirmation`。
- 用户批准、修改、拒绝后，`tool_confirm` 记录确认结果。
- `harness_resume` 读取 checkpoint，把 tool result 追加回原 messages，然后继续原 run。
- 拒绝工具时，模型收到 rejected result 并生成替代回答。
- 前端不再依赖孤立的 `ai:tool_result`，而是以恢复后的最终 run result 为准。

### 5. Context / Evidence / Trace 统一

- `ContextPacket` 继续作为唯一证据结构。
- 新增内部 `EvidenceLedger`，统一处理本地证据、web 证据、工具新增证据的去重、排序、压缩、citation label 稳定。
- `context_assemble` 明确为“预览/计划”，正式执行由后端重新校验 packet IDs。
- `ai_traces` 成为所有 AI task 的统一 trace，不只服务 chat harness。
- checkpoint 只保存恢复所需状态，不保存完整笔记正文。

## Frontend / UI Changes

### 1. 统一 Assistant 任务状态

- 将前端 AI 状态收束为统一状态机：`idle`、`assembling_context`、`awaiting_plan_approval`、`running`、`awaiting_tool_confirmation`、`streaming_final`、`completed`、`error`、`aborted`。
- `UnifiedAssistantPanel` 不再按每个 intent 分散维护状态，而是基于统一 `AssistantRunState` 渲染。
- 顶部状态、副标题、底栏状态、按钮禁用逻辑全部从同一个 run state 派生。

### 2. 统一结果展示：Message + Artifacts

- 对话正文仍在 `ConversationSurface` 中显示。
- 写作补丁、引用检查、研究报告、文档检查、整理建议统一作为 `artifact panel` 展示。
- 每个 artifact 有统一字段：标题、状态、来源 task、证据数量、可执行操作。
- 现有 `PatchPreview`、`CitationCheckView`、`ResearchFocusView`、`DocumentCheckArtifacts` 保留，但接入统一 artifact 数据层。

### 3. 工具活动可视化

- `useHarnessActivity` 升级为统一 run activity hook，支持所有 workflow/harness task。
- 前端展示清晰的活动时间线：
  - 正在组装上下文
  - 正在本地检索
  - 正在联网搜索
  - 等待抓取网页确认
  - 正在生成最终回答
  - 已生成补丁/报告
- 自动完成的只读工具默认折叠；失败、等待确认、用户拒绝、子 agent 结果保持可见。

### 4. 工具确认交互优化

- `ToolConfirmDialog` 改为“确认后继续”的明确体验。
- 用户批准/修改/拒绝后，UI 显示“继续执行中”，并调用 `harness_resume`。
- 拒绝工具不会让任务看起来失败，而是显示“已拒绝，正在生成替代回答”。
- `fetch_web_page` 确认框展示 URL、域名、抓取原因、最大字符数和安全提示。
- 写入类确认框不直接展示 raw JSON，统一展示 patch diff 或结构化摘要。

### 5. 证据 UI 整理

- `ContextPacketDrawer` 成为所有 AI task 的统一证据抽屉。
- 证据分组显示：本地笔记、派生索引、法规条款、网页搜索、网页正文。
- citation 点击继续跳转到证据抽屉，并高亮对应 packet。
- web 证据显示来源等级、URL、backend、是否来自缓存。
- 预览阶段的证据和正式执行后的证据要有视觉区分，避免用户误解。

### 6. 执行计划 UI 明确化

- `ExecutionPlanPreview` 保留，但文案明确为“执行前预览”。
- 用户批准计划后，正式 run 使用后端校验后的 packet IDs。
- 如果正式执行证据和预览不同，UI 显示“证据已刷新”提示，而不是静默变化。
- 单步普通对话不展示计划，避免打扰。

### 7. 前端代码组织

- `UnifiedAssistantPanel` 瘦身，把状态机、run orchestration、artifact mapping 分离到 hooks/lib：
  - `useAssistantRun`
  - `useAssistantArtifacts`
  - `useAssistantActivity`
  - `mapHarnessResultToArtifacts`
- UI 基础组件仍留在 `components/ui/`，业务展示留在 `components/ai/`。
- 不新增前端框架或状态管理库。

## Public API / Interface Changes

- 前端 `assistant_execute` 请求保持兼容。
- 后端响应内部统一为 `HarnessTaskResult`，再映射回现有 `AssistantExecuteResponse`。
- 新增统一 artifact wire shape，但旧 `kind` payload 在迁移期保留。
- `tool_confirm` 行为改为记录确认结果并驱动 resume。
- `harness_resume` 成为确认后继续执行和异常恢复的正式通道。
- `ai:harness_trace` 扩展为所有 AI task 的 activity event；旧监听兼容保留一段迁移期。

## Test Plan

- Rust 单元测试：
  - 每个暴露工具都有 handler。
  - scene/tool/autonomy/web authorization 过滤正确。
  - pending confirmation 保存 checkpoint，批准/修改/拒绝后能恢复继续。
  - workflow adapter 能输出统一 artifact。
  - EvidenceLedger 去重、排序、citation label 稳定。
  - trace 状态覆盖 chat、writing、citation、research、document。

- 前端单元测试：
  - `useAssistantRun` 状态流转正确。
  - `mapHarnessResultToArtifacts` 能映射 chat、patch、citation、research、document。
  - pending confirmation 后 UI 进入继续执行状态。
  - 拒绝工具后不显示为任务失败。
  - evidence drawer 能分组显示本地和 web 证据。
  - completed read-only tools 默认折叠，pending/failed tools 可见。

- E2E：
  - knowledge chat: 本地检索 -> final answer。
  - web enabled: web_search -> final answer。
  - fetch_web_page: 弹确认 -> 批准 -> resume -> final answer。
  - fetch_web_page rejected: 拒绝 -> 替代回答。
  - writing: selection -> patch artifact -> apply patch。
  - citation/document/research: 保持现有 UI 能力且接入统一 activity/artifact。
  - web disabled: UI 和后端均不暴露联网工具。

## Assumptions

- 采用分阶段迁移，保持现有能力不变。
- 后端先建立统一 contract，前端同步接入统一状态和 artifact 层。
- `.md` 仍是笔记权威来源，任何写入都必须通过用户确认和 patch 校验。
- 不新增技术栈，不引入新的前端状态管理库。
- 确定性 workflow 不立即改成自由 agent；先作为 typed adapters 接入 harness 体系。
