# Iris Agent Task Runtime 长任务重构方案

> 本文档用于指导后续 AI 或人工开发者继续完善 Iris Agent Task Runtime。施工时必须遵守根目录 `AGENTS.md`：Tauri 2.x + Rust + React 19 技术栈不变，测试先行，不引入 `unsafe`，不泄露 API Key、笔记正文、完整 prompt、完整对话或工具原始结果。

## 1. 总目标与完成口径

### 1.1 重构目标

Iris 的 AI 执行主路径需要从旧的“四场景 + harness loop”架构，重构为以 `AgentTaskRuntime` 为核心的通用任务运行时：

- 简单对话走轻量 task，不展示复杂任务 UI，也不明显变慢。
- 复杂任务走可持久化、可暂停、可恢复、可审计的 task runtime。
- 长对话、深度研究、文档检查、写作辅助等任务不能因为单段轮次或 token 耗尽直接失败。
- 删除会话、清空 AI 缓存、切换 vault 时，task / event / checkpoint 生命周期必须一致清理。
- task 表不能变成第二份会话历史，只保存摘要、状态、引用、权限与恢复所需的最小结构化信息。
- 旧 `AiScene` 只能作为迁移期兼容字段读取，不能继续驱动新执行策略。

### 1.2 用户选择的完成口径

用户明确偏向“彻底清理”口径，并要求下一重点优先是“长任务恢复”：

- 最终不能长期维护新旧两套 AI 执行系统。
- 不为了兼容旧抽象而保留无意义技术债。
- 可以保留必要的数据迁移读取能力，但旧 `AiScene` / scene router / exemplar_learning 不能继续作为架构中心。
- 下一阶段优先补齐长任务恢复能力，再继续清理后端 scene 驱动策略。

### 1.3 当前完成度估算

按“彻底清理 + 长任务真正可恢复”的口径，当前整体完成约 **35%-40%**。

已完成的是基础骨架和主入口接入；未完成的是任务恢复执行器、后端去 scene 策略化、UI 恢复入口和旧结构彻底拆除。

如果只按“可用、不直接失败”的短期口径，当前约 **50%-55%**，但这不是最终目标。

## 2. 当前进度

### 2.1 已完成

- 新增 `AgentTaskRuntime` 持久化骨架：
  - `agent_tasks`
  - `agent_task_steps`
  - `agent_task_events`
  - migration `032_agent_tasks.sql` / `032_agent_tasks.down.sql`
- task 生命周期已有基础状态：
  - `running`
  - `awaiting_confirmation`
  - `paused_budget`
  - `completed`
  - `failed_safe`
  - `aborted`
- task 与 AI session 绑定，删除 session 后 task/step/event 级联删除已有测试覆盖。
- checkpoint 已加入安全校验：
  - 禁止保存 `api_key`、`token`、`secret`、`password`、`prompt`、`messages`、`content`、`note_body`、`tool_calls`、`tool_results` 等敏感或全文字段。
  - 限制 checkpoint 字符串、数组和对象大小。
- `ai_send_message` / harness 主聊天路径已创建 task。
- `assistant_execute` 非 chat 工作流已创建 task。
- SkillHub 直接安装确认路径已创建 task。
- harness 已区分 finish reason：
  - `completed`
  - `awaiting_confirmation`
  - `budget_exhausted`
  - `round_limit`
- 当预算或轮次耗尽时，主路径会进入 `paused_budget`，而不是直接把“未能在限定轮次内完成”当作普通失败。
- 前端已停止把 `exemplar_learning` 暴露为用户可选主场景。
- 前端主路径命名已从 `resolveAiSceneForIntent` 收敛为 `legacySceneHintForAssistantIntent`，明确其只是兼容提示。
- 全量验证曾通过：
  - `npm run typecheck`
  - `npm run lint`
  - `npm run format:check`
  - `npm --prefix D:\Iris\.worktrees\agent-task-runtime run test`
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - `git diff --check`

### 2.2 当前残留

以下内容仍是主要差距，不能误判为完成：

- `AgentTaskRuntime` 目前更像“状态记录器”，还不是完整执行调度器。
- `paused_budget` 只表示安全暂停，尚未有真正的 resume plan / continuation executor。
- 后端仍大量由 `AiScene` 驱动策略：
  - `scene_router::resolve_scene`
  - `SceneProfile`
  - `slot_for_scene`
  - harness round/token budget
  - tool policy scene affinity
  - prompt/persona scene focus
  - context planner scene 分支
  - skills scene ranking
  - session key 仍含 scene 维度
- `exemplar_learning` 仍存在于后端类型、旧配置、部分工具 affinity、skills legacy trigger、示例 corpora 和历史测试输入中。
- task 状态尚未有完整 UI：
  - 无 task 列表
  - 无暂停任务恢复入口
  - 无 task 事件审计视图
  - 无“继续执行”用户流程
- vault 切换、会话清空、AI cache clear 对 task 清理的覆盖还不完整。
- task runtime 尚未做并行子任务边界：
  - 无 bounded concurrency 策略
  - 无 child task / parent task 关系
  - 无恢复前权限重校验的统一入口

## 3. 后续施工任务规划

### Phase A：长任务恢复闭环

目标：让 `paused_budget` 从“记录状态”变成“可恢复继续执行”的产品能力。

任务：

- 设计 `AgentTaskResumePlan`：
  - 保存 task id、request id、session id、agent intent、legacy scene hint、vault scope hash、selected packet ids、evidence ledger summary、last safe step、budget policy、required permissions。
  - 禁止保存完整 user message、完整 note body、完整 prompt、完整 tool call/result。
- 扩展 checkpoint 结构：
  - 允许保存摘要型 `continuation_goal`。
  - 允许保存引用型 `evidence_refs`。
  - 允许保存 `next_action`，例如 `continue_research`、`continue_context_gathering`、`finalize_answer`。
  - 允许保存 `remaining_budget_hint`，但不依赖它作为安全边界。
- 新增 task resume IPC：
  - `agent_task_resume(task_id)`
  - `agent_task_abort(task_id)`
  - `agent_task_get(task_id)`
  - `agent_task_list(session_id?, status?)`
- resume 前必须重新校验：
  - session 仍存在。
  - vault 未切换，或 task scope 仍在当前 vault 内。
  - note path / selected packet ids 仍可访问。
  - skill 仍存在且仍启用。
  - tool permission 未失效。
  - 当前模型仍满足任务所需能力。
- 对 `BudgetExhausted` / `RoundLimit` 进入统一恢复路径：
  - 不能再次从头跑完整任务。
  - 必须从最近安全 step 和 evidence summary 继续。
  - 如果恢复条件失效，状态转为 `paused_recoverable`，给出安全、短消息，不写入敏感错误细节。
- 为恢复路径增加测试：
  - budget pause 后能查到 task。
  - resume 复用 task id，不新建无关 task。
  - resume 前权限失败时不会调用模型。
  - resume checkpoint 不含 prompt/messages/note body/tool results。
  - 删除 session 后 resume 返回 task not found。

验收：

- 复杂研究任务耗尽预算后，前端能显示“可继续”。
- 点击继续后能接着生成，而不是要求用户缩小问题重试。
- task 表仍不存全文或敏感字段。

### Phase B：后端策略从 SceneProfile 迁到 Task Policy

目标：旧 scene 不再决定预算、模型槽位、工具集、prompt/persona 和 context planner。

任务：

- 新增 `AgentTaskPolicy` 或同等结构，输入为：
  - `AgentIntent`
  - task kind
  - scope
  - web authorization
  - attachment capability
  - write permission requirement
  - research/depth requirement
- 用 task policy 替代 `SceneProfile` 的核心字段：
  - autonomy level
  - max rounds
  - max tool calls per round
  - default/max token budget
  - model capability slot
  - context strategy
- 将 harness planning 改为按 policy：
  - `resolve_max_rounds(policy, override)`
  - `resolve_token_budget(policy, override)`
  - `max_fetch_per_round(policy)`
- 将 LLM routing 从 `resolve_for_scene(scene)` 迁到 `resolve_for_task(policy)`。
- 将 prompt/persona 从 `scene_focus` 迁到 `intent_focus` / `task_focus`。
- 将 context planner 从 scene 分支迁到 intent + scope 分支。
- 保留 `legacy_scene_hint` 仅用于读取旧配置和旧 session，不参与新策略选择。

验收：

- 新执行主路径不调用 `scene_router::resolve_scene`。
- `AiScene::ExemplarLearning` 不再影响任何新任务策略。
- 旧配置能迁移读取，但保存时不再写出旧四场景完整结构。

### Phase C：工具与 Skills 从 scene affinity 迁到 capability affinity

目标：工具可见性和 Skills 激活不再依赖 scene allowlist。

任务：

- 为工具目录引入 capability affinity：
  - `read_notes`
  - `search_notes`
  - `write_notes`
  - `patch_document`
  - `web_fetch`
  - `research_synthesis`
  - `skill_management`
  - `vault_organize`
- 将 `scene_affinity` 改为 legacy 字段。
- 工具策略按 task policy + permission preflight 决定：
  - 读工具默认可用但受 vault scope 限制。
  - 写工具必须用户确认。
  - Web 工具必须 web authorization。
  - Skills 请求的高风险能力默认 blocked 或需确认。
- Skills 激活从 `rank_skills_for_scene` 迁到 `rank_skills_for_task`：
  - 结合显式技能提及、capability、intent、用户消息摘要。
  - 旧 `trigger.scenes` 只作为迁移兼容信号。
- 删除或迁移 `exemplar_learning` 专属工具 affinity。

验收：

- 新工具策略测试不再以 scene 为主断言。
- Skills 面板仍可显示当前可用 Skills，但其内部依据是 capability / intent。
- 旧格式 Skill 能读取，但新格式 Skill 不需要 scene。

### Phase D：Session 与数据生命周期收口

目标：task、session、trace、cache 生命周期一致，避免状态残留和数据安全边界漂移。

任务：

- 梳理所有清理入口：
  - 删除单个 session。
  - 清空全部 AI sessions。
  - 清空 AI cache。
  - 切换 vault。
  - 删除 vault 或重设 vault。
- 明确级联策略：
  - session 删除必须删除 task/step/event。
  - AI cache clear 必须清理 paused/running task 的恢复状态，或标记为 `aborted`。
  - vault 切换后不可恢复旧 vault task。
- 增加数据库测试：
  - session delete cascade。
  - cache clear cleanup。
  - vault mismatch resume denial。
  - checkpoint 不复制 note body。
- 增加前端测试：
  - 删除会话后不显示 orphan task。
  - paused task 恢复按钮在 vault mismatch 时显示不可恢复状态。

验收：

- 无 orphan task/event/checkpoint。
- 任何恢复操作都先校验 scope/vault/session/permission。
- 清理操作不会误删用户 `.md` 笔记。

### Phase E：前端任务 UI 与用户体验

目标：长任务状态对用户可理解，但简单对话不被复杂 UI 打扰。

任务：

- 在 assistant 面板加入轻量任务状态：
  - running
  - awaiting confirmation
  - paused by budget
  - recoverable pause
  - completed
  - failed safely
- 对简单对话：
  - 不展示复杂任务列表。
  - 只保留必要状态和错误恢复。
- 对复杂任务：
  - 展示“继续”“中止”“查看进度摘要”。
  - 不展示 checkpoint 原始 JSON。
  - 进度只展示摘要、步骤名、引用数量、权限等待。
- 将现有 audit drawer 与 task events 合并或建立跳转关系。
- 前端类型同步：
  - `AgentTaskDto`
  - `AgentTaskStepDto`
  - `AgentTaskEventDto`
  - `AgentTaskStatus`
- 所有 IPC 必须通过 `src/lib/ipc.ts` 类型安全封装。

验收：

- 用户能从 paused task 继续复杂任务。
- 简单对话界面没有“任务系统”负担。
- UI 不显示 prompt、笔记全文、API key、工具原始结果。

### Phase F：旧 Scene / Harness 清理

目标：彻底完成用户要求的“不要长期维护新旧两套 AI 执行系统”。

任务：

- 删除 `scene_router` 作为主策略模块。
- 删除或迁移 `SceneProfile`。
- 删除 `AiScene::ExemplarLearning` 的新路径影响。
- 将 `AiScene` 重命名或降级为 `LegacyAiScene`。
- 移除前端 `AI_SCENES` 对旧四场景的配置依赖。
- 迁移 LLM routing 配置：
  - 旧 `routing.scenes` 读取后转换为 capability/task routing。
  - 新保存格式不再写完整 scene map。
- 更新测试：
  - 删除只为旧 scene 抽象存在的测试。
  - 新增 task policy / capability routing 测试。
- 清理文档：
  - ROADMAP 中 AI Runtime 描述改为 Agent Task Runtime。
  - ARCHITECTURE 中不再描述四场景主架构。
  - 保留迁移说明，不做长期兼容承诺。

验收：

- `rg "ExemplarLearning|exemplar_learning|resolve_scene|scene_router"` 只剩迁移、历史文档或明确 legacy 测试。
- 新任务执行不依赖旧 scene 做策略决策。
- 全量前后端测试通过。

## 4. 推荐执行顺序

必须按以下顺序施工，避免做出四不像：

1. **先做 Phase A：长任务恢复闭环。**
   - 这是用户最关心的痛点。
   - 没有恢复闭环，task runtime 只是记录器。
2. **再做 Phase B：Task Policy。**
   - 把预算、模型、工具、prompt 的主策略统一到 task policy。
   - 这是后续删除 scene 的前置条件。
3. **再做 Phase C：工具和 Skills capability 化。**
   - 这是移除 scene affinity 的主要工作量。
4. **再做 Phase D/E：生命周期和 UI。**
   - 先保证后端语义稳定，再做用户可见状态。
5. **最后做 Phase F：旧 scene/harness 清理。**
   - 不要一开始硬删 `AiScene`，否则会破坏配置、session、skills、LLM routing 和大量测试。
   - 但也不要长期保留。每完成一个新策略替代点，就删除对应旧路径。

## 5. 安全与内存边界

后续所有施工必须遵守：

- 不在 task 表保存完整笔记正文。
- 不在 task 表保存完整用户 prompt。
- 不在 task 表保存完整对话 messages。
- 不保存工具原始输入/输出，只保存工具名、状态、摘要、引用 id。
- 不保存 API key、token、authorization header、credential name 之外的秘密。
- 不在日志中输出 checkpoint 原文。
- 不在锁内 await。
- 不引入 `unsafe`。
- 并行子任务必须有界，并且要能中止。
- resume 前必须重新校验权限和 scope，不能相信旧 checkpoint。

## 6. 测试与验收矩阵

每个阶段完成前必须跑：

- Rust：
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
- 前端：
  - `npm run lint`
  - `npm run format:check`
  - `npm run typecheck`
  - `npm run test`
- 额外：
  - `git diff --check`
  - 若改 IPC：同步 `src/types/ipc.ts`、`src/lib/ipc.ts`、相关 Vitest contract test。
  - 若改 DB：新增 migration up/down，并补 migration roundtrip 测试。

关键验收场景：

- 简单对话创建 lightweight task，但响应不变慢、不展示复杂任务 UI。
- 深度研究耗尽预算后进入 paused state，并可继续。
- 工具确认期间 task 进入 awaiting_confirmation。
- 用户拒绝工具后 task 能安全继续或安全失败。
- 删除 session 后 task/step/event 全部消失。
- vault mismatch 时无法 resume。
- checkpoint 安全校验能拒绝敏感字段和超大结构。
- 旧 `exemplar_learning` 配置可读取，但新保存不再写出。

## 7. 给后续 AI 的施工纪律

后续 AI 接手时应先执行这些只读检查：

```powershell
git status --short
rg -n "AgentTaskRuntime|paused_budget|resume|AiScene|ExemplarLearning|exemplar_learning|scene_router|resolve_scene" src-tauri/src src-tauri/tests src tests
rg -n "agent_tasks|agent_task_steps|agent_task_events" src-tauri/src src-tauri/migrations src-tauri/tests
```

施工原则：

- 每个 phase 必须 TDD，先写失败测试。
- 每次只迁移一个策略面，不要同时改 runtime、LLM routing、Skills、UI。
- 不要为了旧测试保留错误抽象；测试应随新架构更新。
- 不要删除用户已有会话读取/删除/清空能力。
- 不要把 task runtime 做成第二套 chat history。
- 如果发现现有代码与本文档冲突，以 `AGENTS.md` 和数据安全原则优先，然后更新本文档。

## 8. 当前最小下一步

下一位施工者建议从这里开始：

1. 为 `agent_task_resume(task_id)` 写失败测试。
2. 定义最小 `AgentTaskResumePlan`，只支持 `paused_budget` 的 chat/research harness continuation。
3. 实现 resume 前校验 session/vault/scope/permission。
4. 让 `paused_budget` 任务能生成下一段回答，而不是返回“缩小问题重试”。
5. 增加前端 `taskId` 透传和“继续”按钮的最小闭环。
6. 跑全量验证。

完成这一步后，整体完成度预计从 **35%-40%** 提升到 **55%-60%**，并且用户最关心的“复杂任务不会因为单段轮次/token 耗尽直接失败”才算真正落地。
