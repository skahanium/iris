# IPC、事件与数据迁移规格

## 1. 新执行 IPC

保留 Session 历史管理 IPC，但执行面收敛为三个命令和一个事件通道。

### `assistant_run_start`

请求：

```ts
interface AssistantRunStartRequest {
  clientRequestId: string;
  session?: AssistantSessionRef;
  message: string;
  contentParts?: ContentPart[];
  explicitReferences: ContextReference[];
  explicitAction?: {
    effect: "answer" | "draft" | "apply";
    target?: ExplicitTarget;
    selectionSnapshot?: SelectionSnapshot;
  };
  webEnabled: boolean;
  securityDomain: "normal" | "classified";
}
```

禁止字段：`scene`、`intent`、`agentIntent`、`notePath`、`currentDocument`、`workflow`。

响应必须快速返回：

```ts
interface AssistantRunAccepted {
  runId: string;
  turnId: string;
  session: AssistantSessionRef;
  state: "accepted";
  stateVersion: number;
}
```

普通域和涉密域统一使用不暴露数据库主键的引用：

```ts
interface AssistantSessionRef {
  domain: "normal" | "classified";
  sessionKey: string;
}
```

普通域 `sessionKey` 使用现有稳定 key，内部再解析 SQLite 整数外键；涉密域使用 thread UUID。不得用 `number | string` 猜测安全域。

### `assistant_run_control`

```ts
type RunControlAction =
  | { type: "approve_change"; confirmationId: string; planHash: string }
  | { type: "reject_change"; confirmationId: string }
  | { type: "resume" }
  | { type: "cancel" };

interface AssistantRunControlRequest {
  session: AssistantSessionRef;
  runId: string;
  expectedStateVersion: number;
  action: RunControlAction;
}
```

修改变更内容不是对旧 confirmation 的 `modify`；前端应提交新的用户消息或结构化修订，Harness 生成新计划和新 confirmation。

### `assistant_run_get`

请求携带 `AssistantSessionRef + runId`，返回 Run 快照、已持久化事件、最终消息 ID、待确认摘要和可安全展示的错误恢复信息。不得返回完整 prompt、原始 checkpoint 或敏感工具载荷。

### `assistant:run_event`

使用 [03-lifecycle-and-evidence.md](./03-lifecycle-and-evidence.md) 定义的统一外壳。旧的多套 request/task/harness 事件在前端切换后删除。

## 2. Session 与非执行 IPC

AI Session 公共接口统一为 `assistant_session_list/load/rename/delete/retract`，请求必须携带 `AssistantSessionRef` 或明确 domain；内部根据 domain 路由到 SQLite 或 CEF Repository。不得把涉密会话加载后转换成普通域 DTO 再缓存。

继续保留的非执行产品能力：

- 新 `assistant_session_*` 的 list/load/rename/delete/retract。
- Evidence list/detail。
- Skills 草稿、确认、启用和列表。
- Web evidence provider 配置与诊断。
- Prompt profile 设置。
- 检索和索引的独立产品能力。

这些接口不得创建或恢复 Agent Run。

## 3. 删除的旧入口

同一次切换删除 Rust command、Tauri 注册、TypeScript wrapper、类型和前端调用：

```text
context_assemble
assistant_execute
ai_send_message
writing_execute
research_execute
organize_execute
citation_check
chapter_writing_execute
document_check_execute
agent_task_resume
harness_resume
agent_task_abort
harness_abort
tool_confirm
session_list / session_load / session_rename / session_delete / session_retract
classified_ai_thread_list / load / save / delete
```

确认、恢复和取消统一进入 `assistant_run_control`；普通和涉密历史统一进入 `assistant_session_*`。领域算法可以保留为内部函数，但不得保留独立 IPC 或第二套持久化。

## 4. 目标普通域数据模型

### `sessions`

保留：`id`、`session_key`、`vault_id`、`title`、`retention_policy`、时间戳。删除 `scene`、`note_path`。

### `session_messages`

增加或统一：

```text
turn_id
role
content / content_parts
tool_calls (仅标准化展示数据)
explicit_references_json
evidence_refs_json
citation_map_json
content_hash
timestamps
```

删除重复的 `evidence_packets` 正文存储。

### `agent_runs`

由 `agent_tasks` 迁移并统一 trace：

```text
run_id PRIMARY KEY
client_request_id UNIQUE
session_id / turn_id
status / state_version
effect / effort / security_domain / risk
envelope_json
goal_summary
budget_policy_json
provider_route_summary_json
stage_metrics_json
token_input / token_output
error_code / safe_error_message
created_at / updated_at / completed_at
```

### `agent_run_steps`

保留 step sequence、kind、status、安全输入输出摘要、版本化 resume state、evidence IDs 和时间戳。

### `agent_run_events`

保存 run_id、event_seq、state_version、event_type、安全 payload 和时间戳；`(run_id, event_seq)` 唯一。

### `session_evidence`

在现有账本上增加 origin_run_id、资料角色、stale 状态和仅供 Web 引用的 bounded excerpt。普通本地正文不复制。

### 删除或合并

- `ai_traces` 合并到 `agent_runs`。
- `deliberation_states` 删除，占位验证改为 Run/Step 状态。
- `writing_states` 删除，必要摘要进入版本化 executor resume state。
- `research_states` 删除且不提供替代 executor。

## 5. 涉密会话格式

升级 CEF 内 JSON Schema：

- 删除 `document_path` 绑定。
- 增加 logical turn/run/event/evidence 结构。
- 所有 explicit reference 都是单条消息或单个 Run 的属性。
- 不在普通 SQLite 创建镜像 Run 或 Evidence。

旧文件在保险库解锁并读取时惰性迁移：解密 → 校验旧 Schema → 转换 → 写临时 CEF → 校验可解密 → 原子替换。迁移失败时保留原文件且只读展示安全错误。

## 6. 普通数据库迁移顺序

迁移必须使用 copy-transform-swap，并在单个事务中完成：

1. 创建目标表和约束。
2. 迁移 Session，丢弃 scene/note_path 绑定但保留消息和标题。
3. 为旧消息生成 turn_id，将可安全转换的 evidence packets 注册到账本并改成 ID 引用。
4. 将已完成 Agent Task 转成 completed Run，合并匹配 request_id 的 trace 指标。
5. 将 running/paused/awaiting 旧任务转成 `cancelled_legacy`，清除不安全 checkpoint，保留安全摘要和事件。
6. 将旧 Writing/Research/Deliberation 结果作为历史完成事件或消息元数据保留；不得保留可执行状态。
7. 重建索引、外键和唯一约束。
8. 验证行数、孤儿引用、终态和 evidence 引用后替换旧表。
9. 删除旧表和不再使用的列。

新代码只写新表，不允许双写。

## 7. Down Migration

Down 脚本必须能恢复旧版可读取的 Session、消息和已完成结果 Schema，但不承诺恢复已被安全取消的运行任务。Down 验证至少覆盖：

- 消息数量和顺序不变。
- Session 标题和正文不丢失。
- completed Run 可降级为旧 completed task/trace 摘要。
- evidence 引用可降级为安全展示元数据。
- 外键检查通过。

## 8. 前端切换

- `useAssistantTasks` 拆成薄的 run controller、event reducer 和 presentation hooks。
- 删除前端 TaskPlan intent/scene 推导和旧 workflow switch。
- Session state 不再接收 active note path。
- Inline AI 显式构造 `explicitAction` 和 snapshot。
- UI 先完成新事件 reducer 的重放/乱序测试，再接真实 IPC。
- 新后端接通后一次性删除旧 hooks、旧 resume 分流和兼容类型。
