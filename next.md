# Iris 后续深拆与性能重构任务清单

本文记录截至 2026-06-11 尚未完成的深拆、Rust AI runtime 架构重塑、性能深改与收尾验证工作。当前事实以代码实现为准；本文件是执行清单，不替代 `ROADMAP.md`、`ARCHITECTURE.md`、`AGENTS.md` 或各类项目守则。

## 当前基线

- 版本基线：`v1.1.0`。
- 已完成入口瘦身：`src/App.tsx`、`src/components/ai/UnifiedAssistantPanel.tsx` 已是 thin facade。
- 已完成 AI 面板首批真实拆分：
  - `AssistantTaskSurfaces`
  - `useAssistantContextScope`
  - `useAssistantConfirmations`
  - `useAssistantConversation`
- 当前仍未达最终目标：
  - `src/components/ai/UnifiedAssistantPanel.impl.tsx` 仍约 1200 行，目标约 500 行以内。
  - `src/App.impl.tsx` 仍需继续拆到只保留桌面壳层编排，目标约 800 行以内。
  - Rust `*_impl.rs` 中仍有多个超大实现文件。
- 已完成 Rust 小步拆分：
  - `retrieval_broker::query_hash` 已拆到 `src-tauri/src/ai_runtime/retrieval_broker/query_hash.rs`。
  - `retrieval_broker_impl.rs` 已降至 700 行以内。

## P0：前端 AI 面板剩余深拆

### 目标

将 `UnifiedAssistantPanel.impl.tsx` 降到约 500 行以内，只保留顶层编排、布局、组件接线和极少量状态组合。

### 待做

1. 拆出 `useAssistantTasks`
   - 接管 `executeKnowledgeChat`、`runKnowledgeChat`、`runWriting`、`runCitation`、`runOrganize`、`runChapter`、`runDocumentCheck`、`runResearch`、`send`。
   - 保持现有 `assistantExecute` IPC 参数、返回处理、错误提示、状态流转不变。
   - 保留 `getNoteContent: () => string` 按需读取，不重新引入整篇 markdown render prop。

2. 拆出 task artifact state reducer
   - 统一管理 `writingPatches`、`citationResult`、`organizeSuggestions`、`researchResult`、`docSummary`、`docIssues`、`lastError`。
   - 将 `clearTaskSurfaces`、patch accept/reject/copy、organize accept/toggle 收敛到明确边界。

3. 拆出 research control hook
   - 接管 `listenResearchProgress`、`abortResearch`、`handleGenerateResearchNote`、`handleExpandResearchDetail`。
   - 保持 `research-focus`、`research-detail-panel` 等测试选择器不变。

4. 消息渲染隔离与虚拟化
   - 给长会话消息列表引入现有 `@tanstack/react-virtual`。
   - 保持 `ConversationSurface` memo 化。
   - 确保 streaming token 更新不触发 patch/citation/research artifact 区域重算。

### 必补测试

- `useAssistantTasks`：
  - writing/citation/organize/chapter/document/research/chat 的状态流转。
  - `parseDocumentChapters` 只在 chapter/document 任务执行时调用。
  - task 失败时 system error message 与 `AssistantActionState` 一致。
- 渲染性能：
  - 长消息列表滚动/追加 token 不重算 artifact surface。
  - artifact state 变化不重建 composer/mention candidates。
- 契约：
  - `UnifiedAssistantPanel.impl.tsx` 行数阈值逐步收紧：先 1300，再 900，最终 500。

## P0：App 壳层继续拆分

### 目标

`App.impl.tsx` 只保留桌面壳层状态组合与 JSX 布局；业务动作、副作用和桥接逻辑迁入 hooks。

### 待做

1. AI sidecar 桥接 hook
   - selection quote
   - prefill message
   - chrome snapshot
   - web search toggle bridge

2. 编辑器动作 hook
   - save
   - lock/unlock
   - undo/redo
   - slash command
   - send selection to AI

3. 生命周期持久化 hook
   - active/inactive tab flush
   - before-close save
   - version snapshot 调度

4. overlay/action dispatch hook
   - command palette action routing
   - overlay open/close state
   - cross-panel keyboard commands

### 必补测试

- 切换 tab 时 active/inactive flush 顺序不变。
- 关闭窗口前保存仍覆盖 dirty active tab。
- command palette 与 overlay action 不改变现有快捷键语义。
- `App.impl.tsx` 行数阈值逐步收紧，最终约 800 行以内。

## P0：Rust AI Runtime 架构重塑

### 总原则

- 不新增 IPC 命令。
- 不改数据库 schema。
- 不改 LLM provider wire shape。
- 保持旧 public import path 可用，例如：
  - `crate::ai_runtime::model_gateway::*`
  - `crate::ai_runtime::skills::*`
  - `crate::ai_runtime::tool_dispatch::dispatch_tool`
  - `crate::ai_runtime::retrieval_broker::hybrid_retrieve`

### `model_gateway_impl.rs`

当前仍是最大风险点之一。继续拆为：

- `model_gateway/messages.rs`
  - `repair_tool_api_messages`
  - `remove_orphan_tool_messages`
  - `insert_missing_tool_result_stubs`
  - `messages_for_api`
- `model_gateway/body.rs`
  - `GatewayRequest`
  - `build_chat_completions_body`
  - provider body options
- `model_gateway/usage.rs`
  - token usage parsing
- `model_gateway/streaming.rs`
  - SSE event parse
  - stream emit
- `model_gateway/http_backend.rs`
  - HTTP request
  - error formatting
- `model_gateway/prompts.rs`
  - drafting/citation/system prompt helpers
- `model_gateway/abort.rs`
  - request abort registry

必补 Rust tests：

- tool message repair 结果不变。
- body construction JSON 完全兼容。
- usage parsing 兼容 OpenAI/DeepSeek style。
- stream event parse 覆盖 content/tool_calls/usage/error。

### `skills_impl.rs`

继续拆为：

- `skills/model.rs`
- `skills/frontmatter.rs`
- `skills/path.rs`
- `skills/scan.rs`
- `skills/validation.rs`
- `skills/activation.rs`
- `skills/prompt.rs`
- `skills/resources.rs`
- `skills/legacy.rs`

边界要求：

- 安装流程继续由 `skill_install_service` 编排。
- `skills` 模块只保留解析、校验、读写、扫描、prompt 注入和资源读取能力。

必补 Rust tests：

- scan metadata 不读取 instruction body。
- activation ranking 稳定。
- `inject_into_prompt` 截断和排序稳定。
- resource path traversal/symlink escape 仍拒绝。

### `tool_dispatch_impl.rs`

拆为按工具域分组：

- `tool_dispatch/markdown.rs`
- `tool_dispatch/search.rs`
- `tool_dispatch/web.rs`
- `tool_dispatch/note.rs`
- `tool_dispatch/memory.rs`
- `tool_dispatch/schedule.rs`
- `tool_dispatch/skills.rs`

顶层只保留：

- `DISPATCHABLE_TOOL_NAMES`
- `ToolDispatchContext`
- `dispatch_tool`
- `dispatch_tool_with_retry`
- retry/error policy

必补 Rust tests：

- catalog 和 dispatch handler 仍一一对应。
- confirmation policy 不变。
- read/write 工具权限不回退。
- `fetch_web_page`、`skills_install`、memory/schedule 工具均可分发。

### `tool_catalog_impl.rs`

拆为：

- `tool_catalog/read.rs`
- `tool_catalog/write.rs`
- `tool_catalog/root.rs`
- `tool_catalog/skills.rs`
- `tool_catalog/web.rs`
- `tool_catalog/groups.rs`

中央模块只负责汇总、查询、去重和测试。

### `retrieval_broker_impl.rs`

已完成 `query_hash` 拆分；继续拆：

- `retrieval_broker/fts.rs`
- `retrieval_broker/vector.rs`
- `retrieval_broker/graph.rs`
- `retrieval_broker/exact.rs`
- `retrieval_broker/template.rs`
- `retrieval_broker/rank.rs`

目标：

- `hybrid_retrieve` 与 `hybrid_retrieve_cached` API 不变。
- 父 impl 保持 700 行以内，并继续下降到约 300 行以内。

## P1：性能深改

### 前端

1. 长会话虚拟化
   - 使用现有 `@tanstack/react-virtual`。
   - 维持引用点击、消息选择、撤回、quote-to-input 功能。

2. streaming token 批量刷新
   - 继续由节流 buffer 驱动。
   - 增加回归测试，确保高频 token 不导致 artifact 面板重渲染。

3. AI task 状态隔离
   - message stream、artifact state、composer input、mention candidates 分离。
   - 用 render count 或 contract test 防止未来退化。

4. App 壳层重渲染控制
   - tab body、status bar、AI chrome snapshot、vault navigator 分离 memo 边界。
   - 重点减少 markdown 输入引起的非编辑器区域重渲染。

### Rust

1. context assembly cache
   - 当前已有基础缓存，继续完善 key 和失效策略。
   - key 必须包含 scene、note_path、query、scope、provider context strategy、input budget。
   - `context_assemble` 与 `ai_send_message` 必须复用同一路径，避免预览/发送双构建。

2. cache 上限治理
   - `PacketCache` 和 context cache 必须有 max entries 和 TTL。
   - `ai_cache_clear`、runtime clear、knowledge reindex 后失效。

3. clone 减量
   - 优先检查 context packets、tool calls、message API body 构造。
   - 所有优化必须有 benchmark 或回归测试保护。

4. benchmark
   - 扩展 `src-tauri/benches/ai_benchmarks.rs`：
     - 大 skill 集合 prompt 注入。
     - 长 tool history body/messages 构造。
     - mixed retrieval packet rank/dedup。
     - 大文本 guardrails 检测。
     - context cache hit/miss。

## P1：文档同步

待代码拆分继续推进后更新：

- `ARCHITECTURE.md`
  - AI runtime 模块图。
  - 前端 AI 面板 hook 边界。
  - context cache 生命周期。
- `docs/README.md`
  - 新增/修正文档索引。
  - 标明 `next.md` 是临时后续执行清单，不是路线图事实源。
- `docs/audits/2026-06-11-project-review-v1.1.0.md`
  - 追加拆分进度。
  - 记录已通过的质量门禁和仍未完成事项。
- `CHANGELOG.md`
  - 只记录用户可理解的工程质量改善，不夸大为产品功能完成。

## P2：最终验收门槛

### 行数目标

- `src/components/ai/UnifiedAssistantPanel.impl.tsx`：约 500 行以内。
- `src/App.impl.tsx`：约 800 行以内。
- Rust AI runtime 核心 impl 文件：原则上单文件不超过 700 行；后续目标约 300-500 行。

### 行为目标

- 不新增数据库 schema。
- 不新增用户可见 IPC 命令。
- 不改变 LLM provider wire shape。
- 不改变 `.md` 文件权威数据原则。
- 不改变现有 data-testid、关键快捷键和 UI 流程，除非另有明确需求和测试。

### 验证命令

每个阶段至少运行：

```bash
npm.cmd run lint
npm.cmd run format:check
npm.cmd run typecheck
npm.cmd run test
cargo fmt --manifest-path D:\Iris\src-tauri\Cargo.toml --all -- --check
cargo clippy --manifest-path D:\Iris\src-tauri\Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path D:\Iris\src-tauri\Cargo.toml
npm.cmd --prefix D:\Iris run audit:rust
```

性能专项：

```bash
cargo bench --manifest-path D:\Iris\src-tauri\Cargo.toml --bench ai_benchmarks
```

### 完成定义

- 所有 P0 项完成并通过完整质量门禁，才可声明“深拆主线完成”。
- P1 性能深改必须以 benchmark 或回归测试证明，不接受只凭代码观感声明性能提升。
- 文档必须同步到事实源；冲突处以代码和测试为准。
