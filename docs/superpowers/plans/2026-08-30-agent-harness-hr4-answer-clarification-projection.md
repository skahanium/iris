# Agent Harness HR-4：回答、澄清、错误与投影实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让普通 Agent Run 以自然正文、受控来源组和正常会话延续完成；只让高风险或历史严格合同使用现有结构化终局；让错误提示与同 Run 前端投影都以产品语义可靠收敛。

**Architecture:** 不新建状态机、表、IPC 或 Provider 分支。`ExecutionEnvelope` 继续冻结授权、证据义务和风险；`NormalRunService` 只依据既有 `web_reason` 和历史 `fresh_fact` 决定是否附加既有 `submit_final_answer`。普通 `WebRequired` 继续使用 HR-3 通用 `AgentToolLoop`，由 `RunEngine` 已有的 current-Run evidence gate 和 `SourceGroupFallback` 持久化自然正文。普通澄清就是完成的 assistant 消息，下一条用户消息创建新 Run 并从普通会话历史装配上下文。前端继续由唯一 `useAssistantConversationProjection` 的同 Run upsert 投影。

**Tech Stack:** Rust、Tokio、SQLite、React 19、Vitest、既有 Agent Run/ToolLoop/Provenance/Presentation 模块；不新增依赖、迁移、IPC、Provider 或领域实体。

## 全局约束

- 不创建 worktree；只在当前 `branch-v1.3.0` 工作区施工。
- 不连接真实 Provider；全部后端行为只用既有 headless LLM/MCP fixture。
- 不修改旧 Run 持久化格式，不自动重放旧 `AwaitingInput`、失败或已完成 Run。
- 不为电影、天气等领域增加分支。新 Run 的结构化终局只保留 `HighStakesCurrentFact`；非空历史 `fresh_fact` 保留既有兼容适配器。未来 CitationCheck 必须接入同一选择函数，不能在引擎复制解释器。
- 普通 `WebRequired` 仍必须有本 Run Web evidence 才能完成；区别仅是不要求内部结构化提交工具。
- `ProvenancePolicy` 仍是唯一来源语法与归属解释器；自然回答只消费既有受控来源组。

## 已完成的开工审计

- [x] 对照 `agent-harness/01`、`03`、`04`、`05` 和附录 B：HR-4 是“普通严格联网仍自然”，不是放松 current-Run evidence gate。
- [x] 追踪 `NormalRunService → AgentToolLoop → RunEngine → Provenance`：所有 `CurrentRunWeb` 无条件加入 `submit_final_answer`，令普通回答发生协议修复/失败；`RunEngine` 已支持无该工具时的 source-group fallback。
- [x] 追踪 `RunIntake`、会话装配和 `useAssistantConversationProjection`：新普通 Run 已不应进入 `AwaitingInput`；前端已有按 `run_id` upsert、终态回填、迟到事件隔离，缺少命名回归。
- [x] 追踪错误文案：`FinalizationProtocolInvalid` 仍向用户暴露内部“模型/协议”语义，需替换成可行动的产品语义，错误码保留为诊断。

### Task 1：测试先行，锁定自然终局与严格边界

**Files:** `src-tauri/src/ai_runtime/normal_run_service_tests.rs`、`src-tauri/src/ai_runtime/run_engine/finalization.rs`（测试模块）。

- [x] 将 `hr1_ordinary_research_reply_still_requires_structured_finalization` 从 `should_panic` 占位改成正式 green 回归：普通显式联网 Run 的工具面含 `web_search`、不含 `submit_final_answer`；搜索后自然正文完成，只持久化一条 assistant 消息和受控来源组；无第三次协议修复请求。
- [x] 新增高风险“当前法律建议”工具面回归，证明仍暴露既有 `submit_final_answer`，而不是删除严格合同。
- [x] 将 failure-message 测试扩展为四类产品语义：证据不足、能力不可用、回答未完成、用户材料无效；内部 error code 只留 durable diagnostic。
- [x] 先运行改变行为的普通最终化与自然澄清回归并确认 RED；随后以高风险边界回归防止实现退化为删除严格合同。

### Task 2：以既有 Envelope 收口终局工具选择

**Files:** `src-tauri/src/ai_runtime/normal_run_service.rs`、`src-tauri/src/ai_runtime/normal_run_service_tests.rs`。

- [x] 在 `normal_run_service.rs` 增加最小私有 `requires_structured_finalization(context)`，只消费冻结 `web_reason` 与 `fresh_fact`：`HighStakesCurrentFact` 或非空历史领域 policy 为真，普通 `WebRequired` 为假。
- [x] 用该函数替换无条件 `CurrentRunWeb` 加工具；不改变能力、预算、Web evidence requirement、Provenance 或 source group，不改历史非空 `fresh_fact` 适配器。
- [x] 运行 Task 1 命名测试、`webpreferred_movie_research_uses_generic_web_evidence_without_city_or_domain_tools` 和 `uncalibrated_web_answer_does_not_display_model_authored_precise_marker`，确认 green 且不依赖全局 evidence ID。

### Task 3：自然澄清、下一轮承接与 Run-local 投影

**Files:** `src-tauri/src/ai_runtime/normal_run_service_tests.rs`、`tests/use-assistant-run-transcript.test.tsx`。

- [x] 用 headless 模型让“附近电影院今晚有什么场次”返回自然地点追问；断言当前 Run `Completed`、无 `pending_input`、仅一条 assistant 持久化消息。
- [x] 同 session 接受“深圳”这一新 Run，装配 context 并断言上一轮问题和 assistant 追问均可读，证明自然澄清不会冻结/重放旧 Run。
- [x] 把现有终态覆盖用例命名为 HR-4 合同，并添加无同 Run user 行的迟到 completed/content 事件不污染当前会话的 Vitest 回归。
- [x] 先运行命名 Rust/Vitest 测试确认 RED；投影原有同 Run upsert/终态分支已满足合同，因此未新增第二个 state store。

### Task 4：错误语义与事实源同步

**Files:** `src-tauri/src/ai_runtime/run_engine/finalization.rs`、`ARCHITECTURE.md`、`agent-harness/02-current-state-and-debt.md`、`agent-harness/05-implementation-roadmap.md`、`agent-harness/06-evaluation-performance-and-acceptance.md`、`agent-harness/appendices/A-status-and-test-traceability.md`、必要时 `docs/README.md`。

- [x] 只改 `safe_failure_message(FinalizationProtocolInvalid)` 的可见文本为“本次回答未完成必要的来源校验，请重试”；不改变分类、状态、验证或稳定错误码。
- [x] 所有代码/测试 green 后，按命名回归同步 HR-4 实现事实、验收矩阵和追踪；明确历史 `AwaitingInput` 仅兼容读取，不把未来 CitationCheck UI 或 HR-5 写成已实现。

### Task 5：完整自检、提交与推送

- [x] 运行相关 Rust 模块、`tests/use-assistant-run-transcript.test.tsx`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`npm run lint`、`npm run format:check`、`npm run typecheck`、`npm run docs:check` 与 `git diff --check`；不运行真实 Provider 或无关全量评测。
- [x] 逐项自审：普通自然终局、严格边界、自然澄清/新 Run 承接、同 Run 唯一投影、终态恢复不重放、迟到事件隔离、四类错误语义、无领域/Provider 特判、无新增状态/IPC。
- [x] 检查 staged diff，以 `refactor(ai): 完成 Harness HR-4 回答与投影收敛`（或同等中文 Conventional Commit）提交并推送 `branch-v1.3.0`。
