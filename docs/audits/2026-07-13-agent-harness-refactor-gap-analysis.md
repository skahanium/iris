# Agent Harness Refactor 差距审计

> 审计日期：2026-07-13  
> 验收基线：`docs/agent-harness-refactor/` 全部规范  
> 结论：**未达到发布或完成条件。** 当前分支新增了统一 Run 的基础合同、Repository 和部分命令，但新旧执行系统仍并存，且数据迁移、领域行为、涉密会话和发布验证均未满足规范。
>
> **2026-07-18 事实更新**：前端主发送路径已切换至 `assistantRunStart`（`useAssistantRun.ts`、`UnifiedAssistantPanel.impl.tsx`、`useInlineAi.ts`）。`useAssistantTasks` 与 `assistant_execute` 已删除。阶段 8「前端未切换」在发送入口层面**已部分完成**；旧 scene/TaskPlan 类型与数据表迁移仍待阶段 9–10。

## 已有基础

- `run_contract.rs`、`run_intake.rs`、`run_engine.rs`、`agent_run_repository.rs` 已建立 Run、事件和普通域持久化基础。
- `assistant_run_start`、`assistant_run_control`、`assistant_run_get` 及 `assistant:run_event` 已在后端、类型和 IPC 包装层定义。
- `047_agent_run_foundation`、`048_agent_run_confirmations` 已新增 Run 表和确认表。
- 前端事件 reducer 的重复、乱序、缺口和重连测试已存在并通过（17 个 focused tests）。

以上仅证明基础设施存在；不证明产品已切换。

## 阶段差距矩阵

| 阶段                   | 规范退出条件                                                     | 当前证据                                                                                                                                                      | 结论     | 必须完成的工作                                                                                      |
| ---------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------- |
| 0 基线                 | 旧入口、迁移 fixture、性能基线可自动复现                         | `tests/ai-agent-stage0-contract.test.ts` 仍断言旧链路存在；没有覆盖目标规格要求的旧库/旧 CEF fixture 与自动 SLO 基线                                          | 未完成   | 用真实 fixture 覆盖普通/涉密旧数据、旧任务、旧证据与性能指标，并移除将旧链路当作正确行为的最终测试  |
| 1 合同                 | Rust/TS 枚举一一对应，状态机与 reducer 可重放                    | 新 Run 合同与 reducer 存在；旧 `AgentIntent`、`AiScene`、`TaskPlan` 仍为主要生产类型                                                                          | 部分完成 | 固化新合同的跨端测试，并消除新合同向旧 intent/scene 的适配依赖                                      |
| 2 数据                 | copy-transform-swap、up/down/up、旧历史可读、无旧状态表          | 047 为显式 additive migration；`sessions.scene/note_path`、`evidence_packets`、`ai_traces`、`writing_states`、`research_states`、`deliberation_states` 仍存在 | 未完成   | 实现最终迁移、下迁移、普通数据库 fixture 和 CEF 惰性迁移；删除旧表/列与旧 checkpoint                |
| 3 策略                 | 唯一 PolicyDecisionEngine、六项文档能力、冻结计划二次校验        | `PolicyDecisionEngine` 与 `FrozenChangePlan` 已新增；旧 `ToolPolicy`、permission preflight、tool confirmation 仍独立使用 scene/request_id                     | 部分完成 | 统一所有 dispatcher、grant、文档 scope、涉密和 Web 判断，禁止旧路径放宽授权                         |
| 4 Run Engine           | 所有执行共用状态机，accepted 先持久化，单调用直答可取消          | 新直答引擎已存在；`ai_harness/harness/run.rs`、`agent_task.rs`、`trace.rs` 仍是并行状态机和持久化                                                             | 部分完成 | 将工具轮、确认、暂停/恢复、终态事务迁到 Run Engine，删除第二状态机                                  |
| 5 Envelope/Context/Web | 无场景路由、无 active note、Web evidence 统一入账本              | 旧 `context_assemble`、scene 路由、`note_path` 和 active-note 相关 payload 仍存在；前端未接新入口                                                             | 未完成   | 仅以对话、显式引用、动作快照组装上下文；接入 web preferred/required 与证据引用                      |
| 6 Skills/MCP/Provider  | prompt-only skill、类型化 Web MCP、关键路径无扫描、受控 failover | Skills、MCP 与 provider 仍保留 scene/legacy routing，MCP/skill 与旧 tool catalog 并行                                                                         | 未完成   | 移除 scene rerank 和运行时探测，限制 MCP 为 Web adapter，补齐 provider/mapping 合同测试             |
| 7 领域 Executor        | 公文/工作分析/小说在统一 Run 中运行；无 Research executor        | `ai_workflows/research_workflow.rs`、`research_commands.rs`、`research_state.rs` 和研究 UI 仍在；writing/organize/citation/document 各有 IPC                  | 未完成   | 将可复用算法改为无生命周期 executor/capability；彻底删除 research 产品路径                          |
| 8 前端切换             | 发送仅调用 `assistant_run_start`；单 reducer；无 editor 绑定     | **2026-07-18**：`useAssistantRun`/`assistantRunStart` 已是主发送入口；`useAssistantTasks` 已删除。仍残留 scene/TaskPlan 类型与旧数据表。                      | 部分完成 | 将 event delta 全面接入消息 UI，删除 TaskPlan/scene 兼容层与旧会话数据依赖                          |
| 9 切断旧链路           | 无旧 IPC、无双写、无研究 executor、无兼容 facade                 | `src-tauri/src/lib.rs` 仍注册全部禁止 IPC；`src/lib/ipc.ts`、`src/types/ai.ts` 和组件仍调用/声明它们                                                          | 未完成   | 一次性删除 Rust command、Tauri 注册、IPC wrapper/类型、hooks、workflow、runtime、迁移旧表和测试残留 |
| 10 硬化交付            | 安全/故障/性能/长会话/E2E/全质量门禁全部通过，文档改为事实       | 文档仍标记“目标规格，尚未实施”；评测集、目标 SLO、完整 E2E 和全门禁没有完成证据                                                                               | 未完成   | 建立完整评测与故障注入，运行所有门禁，更新架构/IPC/安全/ROADMAP/CHANGELOG 为已验证事实              |

## 直接阻断发布的残留

以下由 `rg` 定位，任一存在均符合 `09-verification-and-rollout.md` 的发布阻断条件：

- 旧执行 IPC：`assistant_execute`、`context_assemble`、`ai_send_message`、`writing_execute`、`research_execute`、`organize_execute`、`citation_check`、`chapter_writing_execute`、`document_check_execute`、`agent_task_resume`、`harness_resume`、`agent_task_abort`、`harness_abort`、`tool_confirm`。
- 旧 Session 与涉密 thread IPC：`session_*`、`classified_ai_thread_*`。
- 旧领域/状态：`ai_workflows/research_workflow.rs`、`research_state.rs`、`writing_state.rs`、`trace.rs`、`agent_task.rs`、`task_plan.rs`。
- 旧数据事实：`sessions.scene`、`sessions.note_path`、`session_messages.evidence_packets`、`ai_traces`、`writing_states`、`research_states`、`deliberation_states`。
- 旧前端路由：`useAssistantTasks.ts`、`useAssistantHarnessResume.ts`、`assistant-taskplan.ts`、`assistant-scene.ts`、`assistant-routing.ts`。

## 已执行验证

- `npm.cmd --prefix D:\Iris run test -- tests/assistant-run-events.test.ts tests/assistant-run-ipc.test.ts`：通过，17 tests。
- Rust focused test 首次因 Windows TLS 证书凭据与缺失 Cargo cache 无法解析依赖；已完成受控依赖下载，基线编译仍在进行，尚不可作为通过证据。

## 施工顺序

严格遵循 `08-implementation-plan.md`：先补齐阶段 0–4 的数据、策略和唯一 Run Engine 退出条件，再完成阶段 5–7 的上下文、能力和领域 executor；只有阶段 8 前端完成新协议接入后，才在同一切换窗口实施阶段 9 的删除与最终数据迁移；最后执行阶段 10 的评测、故障、安全、性能、E2E 和全质量门禁。
