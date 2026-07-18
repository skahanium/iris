# 进度日志

## 2026-07-13

- 完成文档差距审计并保存审计报告。
- 修复新 Run event channel 为 `assistant:run_event`，补充并运行 IPC 测试。
- 修复 normal session 以 opaque key 解析 legacy-bound row；添加并运行 Rust 回归测试。
- 新 direct Run 改为 `CapabilityRouteRequirements`，并保留 legacy Chat 路由兼容测试。
- 实现涉密会话 CEF v2 与旧 CEF 惰性迁移：移除 `document_path` 持久化，补齐 Turn/Run/Event/Evidence，临时 CEF 解密验证及原子替换。
- 验证：`classified_session` Rust 8/8 通过；分类会话前端契约 15/15 通过；相关领域测试 29/29 通过；`npm run typecheck` 通过。
- 下一步：建立并切换 `assistant_session_*`，随后删除旧 session/thread IPC 及前端调用。- 新增 `assistant_session_list/load/rename/delete/retract`：按显式 `AssistantSessionRef.domain` 路由到 normal SQLite 或 classified CEF；普通 API 只传 opaque key，涉密 API 不创建普通库镜像。
- 新增 normal repository 的 opaque-key 历史管理回归测试；新增 classified suffix retract 的生命周期同步测试。
- 追加验证：统一会话 IPC 契约 5/5 通过，Assistant command Rust 测试 3/3 通过，`npm run typecheck` 通过。- 定向格式与补丁检查：6 个本轮 Rust 文件 `rustfmt --check` 通过，`git diff --check` 无输出。
- 本轮未执行全仓质量门禁；原因是目标体系仍处于切换中，旧执行链、最终迁移和全量验证尚未完成。- 将 `RunIntake` 的最小 envelope 替换为确定性 `resolve_envelope`：安全域、用户 local-only / do-not-modify 约束、显式 action、显式 references、小说对话默认、Web 语义、模态、材料角色和 capability 需求均在不读取 scene 或 active editor state 的情况下解析。
- 修复 `reject_change`：冻结计划被拒绝后以单一事务将确认标记为 rejected、Run 恢复为 running，并写入唯一 resumed 事件；重复拒绝幂等。
- 验证：完整 `run_intake_tests` 11/11 通过；本轮 Rust 格式检查与 `git diff --check` 通过。
- 仍未接通 classified Run 的 CEF 生命周期、最终 policy/capability dispatch、旧执行链切换和最终 copy-transform-swap migration；这些仍是发布阻断项。- 新增 CEF-only classified Run repository 基础：accepted 用户消息、Turn、Run 和 accepted Event 全部持久化于涉密 Conversation；client_request_id 会跨 CEF 会话扫描以保持幂等。
- 新增 classified Run get/cancel：按统一 Run 合同回放 CEF 事件，取消使用同一 optimistic state version 规则，绝不触碰普通 SQLite。
- 修复 CEF 索引全局缓存跨 vault 串用：缓存现绑定 vault 路径；完整 classified session 测试 11/11 通过。
- 当前 classified Run 尚未接入普通 `RunEngine`，因为该引擎会向 SQLite 写入；后续必须实现 CEF-only provider dispatch 与终态持久化后才可切换生产入口。

## 2026-07-13 分类域 Run 端到端最小链路

- 新增 `classified_run_engine`：分类域流式 Provider 调用只将最终消息写入 CEF；通用事件只发送 accepted/preparing/running/completed 等无正文生命周期事实。
- `classified_session` 增加 preparing、running、failed、complete 的乐观版本状态写入；完成消息与 completed 事件同一原子 CEF 写入，取消抢先落盘时拒绝补写完成结果。
- `RunIntake::start_classified` 只接受 CEF-only 的离线直答，拒绝当前尚未具备分类能力/策略支持的 Web、工具、写入和 durable 请求。
- `assistant_run_start/control/get` 已按 SecurityDomain 分流；分类取消同时请求中止 Provider。
- 通过：分类域 Run 组 5 项、分类 CEF 接收测试、真实 mock Provider 分类执行测试。尚未完成统一 Policy、能力路由、前端切换、旧链路删除、SQLite 最终迁移、评测和全量质量门禁。

## 2026-07-13 统一 Run 策略接入（进行中）

- `PolicyDecisionEngine` 增加纯 `RunPolicyRequest` / `RunPolicyDecision`：离线或分类域 Web、分类域 MCP/普通 vault/evidence 能力、Answer 的写入能力，以及显式引用的 read/send_to_model deny 都会返回稳定 permission denial。
- `CapabilityId` 增加只读标识访问器；Intake 的直答 envelope 确定性声明 `model.text`，分类 Web 在 CEF 接收前被拒绝。
- normal Run Repository 可从已持久化的 `envelope_json` 和脱敏 explicit reference metadata 重建策略输入，不读取用户正文、编辑器状态、scene 或临时 UI 参数。
- RunEngine 增加拒绝决策的持久化门禁原语：先写 `permission_denied`，再安全终止，测试证明不会进入 Provider dispatch。
- 发现阻断项：六项文档能力策略尚无持久化表或 Repository；现有 `agent_permission_grants` 是 legacy task/grant 模型，不能复用为文档矩阵。后续先添加 up/down migration 和策略配置 Repository，再将 normal/classified Provider dispatch 接入真实策略源；不使用 `allow_all` 作为假接入。

## 2026-07-13 文档策略持久化来源（进行中）

- 新增 `049_document_capability_policies` 与 down 脚本：scope 仅 vault/folder/document，capability 仅六项矩阵，decision 仅 allow/deny，未知配置无法落库。
- 迁移注册到严格 up/down 序列；验证 migration up/down、CHECK 约束和表删除通过。
- 新增 `document_policy_repository`，从该表加载 vault/folder/document 规则到唯一 `PolicyDecisionEngine`；未知持久化值安全失败，测试验证 folder deny 与 document override。
- 尚未把该真实 Repository 接到 normal/classified 的生产 dispatch；此步骤必须在下一轮完成后才可声称策略门已生效。

## 2026-07-13 normal Provider dispatch 的真实策略门

- `assistant_commands::spawn_normal_direct_run` 现在从 accepted Run 的持久化 Envelope/显式引用重建策略请求，加载 `document_capability_policies`，并在 Provider route/credential hydrate 前调用 `RunEngine::enforce_policy_before_dispatch_with_sink`。
- 持久化 document `send_to_model=deny` 的 normal Run 测试通过，证明真实策略 Repository 的拒绝结果可达 Provider 路由前边界。
- 分类域不能读取普通 SQLite 策略表；仍需实现 CEF 内的分类文档策略来源和同等的 `permission_denied` CEF 事件，才可声称双域策略门完成。

## 2026-07-13 分类域 CEF 文档策略来源（进行中）

- 新增 `classified_document_policy_repository`：使用 `.classified/.iris-ai/document-policies.cef` 原子加密写入/读取 vault/folder/document 六能力规则，未知或损坏规则安全失败。
- 验证分类 document `send_to_model=deny` 从 CEF 加载后生效；该模块不引用普通 SQLite。
- 后续必须将 CEF policy request 与 `classified_run_engine` 的 Provider dispatch 相连，并以 CEF Run Event 持久化 `permission_denied`；尚未完成前不称分类策略门已接入生产。

## 2026-07-13 双域真实策略门（进行中）

- normal Provider dispatch：从 SQLite accepted Run 重建策略输入，加载 `document_capability_policies`，拒绝时在 Provider route/hydrate 前写 `permission_denied` 和安全终态。
- classified Provider dispatch：从 CEF Run 重建策略输入，加载 `document-policies.cef`，拒绝时 CEF 持久化 `permission_denied → preparing → failed`，测试证明 Provider 调用为零。
- 聚焦策略/CEF 测试通过。完整 Rust library 测试执行到约 880/1195 项时工具输出被截断，未作为“全量通过”记录；最终验收必须重新取得完整无错误证据。

## 2026-07-13 前端与旧链路依赖审查

- **2026-07-18 更新**：`useAssistantTasks` 与 `assistant_execute` 已删除；`useAssistantRun`/`assistantRunStart` 为主发送入口；`useInlineAi` 已走 `assistantRunStart`。
- 仍待清理：TaskPlan/scene 类型残留、旧数据表与领域 executor 未完全迁入 Run Engine。
- 现有新 Run 仅具备 direct answer，不能无损承接 writing/citation/organize/chapter/document 与 explicit action。根据重构规范，必须先将领域算法转为无生命周期 executor/capability 并接到 Run Engine，然后删除旧数据依赖；不得提前删 UI 造成能力退化。

## 2026-07-13 — Run 显式引用上下文装配

- 新增 ai_runtime::run_context：仅从指定 Run 已持久化的用户消息与 explicit_references_json 组装临时 Provider prompt；不接收客户端 excerpt，不读取当前编辑器状态、旧 scene 或未引用文件。
- 每个引用在正常域中均经过用户笔记路径校验、严格 UTF-8 读取、持久化内容 hash 校验和 UTF-8 range 校验；.iris、.classified、过期/失效或已变更引用均在 Provider 调度前稳定失败。
- 正常域 assistant_run_start 的直接执行链现已在 policy gate 后、Provider route 前装配该上下文；没有显式引用的普通对话仍可在未打开 vault 时执行。涉密 CEF 直接链保持不经过 normal DB/context assembler。
- 测试先行：实现前因缺少 RunContextAssembler 产生预期编译失败；实现后 4 条 run_context_tests 全部通过，覆盖显式引用白名单、未引用文件隔离、涉密路径拒绝、hash 变更阻断和无 vault 直接问答。
- 已运行 cargo fmt、cargo fmt -- --check 与 git diff --check；尚未运行完整质量门禁，且整体重构仍未完成。

## 2026-07-13 — 显式资料证据账本接入

- Run context 现保存所属 session 和首条消息序号；每份通过路径/hash/range 校验的显式资料都以 metadata 形式注册到 session_evidence，正文只保留在短暂 Provider prompt 中。
- 正常域流式 Run 新增带 evidence ID 的最终执行入口；完成事务把这些 ID 绑定到最终消息，仍不将资料正文写入 Run、Event 或 checkpoint。
- assistant_run_start 正常域在 policy/context 之后、Provider route 之前注册证据；账本持久化失败会以 PersistenceFailed 在调度前结束 Run。CEF classified 链未触及 normal SQLite。
- 新增证据账本契约用例；run_context_tests 当前 5/5 通过。流式 Run 回归用例 2/2 通过；已运行 cargo fmt -- --check 与 git diff --check。

## 2026-07-13 — Evidence ID 最终消息绑定验证

- 新增流式 Run 端到端单元测试：先登记同一 Run 所属的 local evidence，再以带 evidence ID 的 Provider 执行入口完成；断言最终 assistant session_messages.evidence_refs_json 等于该稳定 ID 列表。
- 该测试通过，证明正常域显式资料不是仅被登记，而是被同一 Run 的最终消息以 ID 绑定；正文不进入该关联字段。
- 当前仅完成这个后端闭环的定向验证，前端发送入口与旧 Harness 删除仍未完成。

## 2026-07-13 — 直接 Provider 请求的注入边界

- normal Run 的 direct_gateway_request 现发送固定 system 边界与独立 user data：显式参考资料被声明为不可信数据，不能改变权限、工具、上下文边界或系统指令。
- 定向测试确认 system/user 两层消息分离，并保持 tool-free streaming 请求。

## 2026-07-13 — Research 专用后端入口撤销（第一步）

- 从 Tauri command 注册、commands 模块公开入口和 Harness dispatch 中移除 research_execute/research_abort 等 Research 专用入口。
- Harness 不再导入或调用 research_commands，不再创建 ResearchReport artifact、research result 字段或专用 evidence wire；Research legacy intent 被临时归入普通 chat dispatch，避免创建 ResearchState 或多轮研究 executor。
- 仍待删除 research_workflow、research_state、旧 AgentIntent/TaskPlan 路由和前端 Research 控制；因此此阶段不代表 Research 删除完成。

## 2026-07-13：移除独立 Research 执行器与工件链路

- 删除 Rust 侧 `research_commands`、`research_state`、`research_workflow` 模块及其 Tauri 命令注册；旧迁移中的历史表保留到最终增量迁移阶段处理。
- 删除前端 `useResearchControl`、Research IPC 包装、`ai:research_progress` 事件、Research 结果/状态类型、`evidence_sources` 工件类型和工作区渲染。
- 明确研究类请求暂经普通对话 Run 路径处理，不再创建独立执行器、检查点、状态或工件；新的会话证据详情能力不受影响。
- 调整旧 TaskPlan 兼容测试，使其断言直接回答且不产生 Research 工件；该兼容层将在下一阶段整体删除，而非长期保留。
- 验证：`assistant-run-ipc`、`assistant-artifact-value-gates`、`agent-taskplan-routing`、`writing-research-state-panel` 共 43 项通过；`e2e/ai-workflow` 13 项通过；`npm run typecheck` 通过。

## 2026-07-13：统一会话历史 UI 切片

- `SessionHistoryDropdown` 改为只用 `assistantSessionList/load/rename/delete`；不再按普通数值 session 与涉密 thread 走两条历史 API 分支。
- 历史选择返回 `AssistantSessionRef`，`useAssistantConversation` 新增并持有该 opaque 引用；已加载的统一会话撤回优先调用 `assistantSessionRetract`。
- Header 与面板已向下传递该引用。旧发送/任务链仍在，下一阶段将以统一 Run client 替换它，随后删除临时显示兼容属性和所有旧会话 API 使用。
- 验证：`tests/session-history-dropdown.test.tsx` 通过；`npm run typecheck` 通过。

## 2026-07-13：前端统一 Run client 基础

- `useAssistantRun` 已从纯 UI 状态映射扩展为统一 Run client：调用 `assistantRunStart`、订阅 `assistant:run_event`、保存 opaque session ref 与 `stateVersion`、通过 `assistantRunControl` 发取消请求。
- 新增测试覆盖 start acknowledgement 与事件驱动版本推进。
- **2026-07-18**：`useAssistantTasks` 已删除；`assistantRunStart` 为生产发送入口。后续需将 event delta 全面接入消息 UI，并删除 TaskPlan/scene 兼容层。
- 验证：`tests/use-assistant-run.test.tsx` 2 项通过；之前的 `npm run typecheck`、会话历史测试保持通过。

## 2026-07-18 技术债深挖与还债启动

- 审计报告：`docs/audits/2026-07-18-tech-debt-deep-dive.md`
- Phase 1–3 **已收口**：删除前 flush、锁内 resolve+write + 真并发测、orphan LLM IPC、涉密单栈、fetch/Tavily、PM round-trip、chunker fence、MCP legacy、`allow(dead_code)` 清零、`AppError::Provider` failover、`config/*.json` preset 单源、`note_title` 收敛、temperature 文档化为固定 `None`
- DeepSeek 报告纠偏：生产 panic 误报；commands 零测试夸大
- Phase 4 仍外：IPC codegen、temperature UI、大文件拆分
