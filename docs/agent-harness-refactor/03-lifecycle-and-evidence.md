# 会话生命周期、Checkpoint 与证据规格

## 1. 逻辑模型

```text
Conversation
└── Turn
    ├── user message
    └── Run
        ├── assistant/tool messages
        ├── Step/Event
        └── Evidence references
```

- Conversation 对应现有 Session，但不包含 scene 或文档绑定。
- Turn 是一次用户输入及其结果的逻辑分组，使用 `turn_id` 关联，不要求新增独立表。
- Run 是唯一执行生命周期；一次 Turn 正常只有一个主 Run。
- Step 是可恢复工作的持久摘要。
- Event 是有序、脱敏、可供 UI 重放的运行事实。
- Evidence 是唯一可引用来源账本。

## 2. 标识符与幂等

- `session_id`：数据库会话标识。
- `turn_id`：稳定 UUID，关联用户消息、Run 和助手结果。
- `run_id`：稳定 UUID，替代 task/request 双主键语义。
- `event_seq`：Run 内严格递增整数。
- `state_version`：每次状态写入递增，用于 control 乐观锁。
- `tool_call_id`：Provider 工具调用标识，必须在 Run 内唯一。
- `evidence_id`：普通域证据账本主键；citation label 只负责展示。

`assistant_run_start` 应支持客户端提供 `client_request_id`，重复提交返回既有 Run，不创建第二次执行。

## 3. 持久化时机

### 接收时

在做检索、路由或模型请求前原子写入：

- 用户消息。
- 显式引用的安全元数据。
- accepted Run。
- 第一个 accepted Event。

### 执行中

- 状态转换、工具开始/完成、确认请求和 Provider 切换写入 Event。
- 只在 durable、paused、awaiting_confirmation 时写 checkpoint。
- 流式 token 不逐 token 入库；使用内存缓冲，在稳定片段或终态保存助手消息。

### 完成时

在同一事务中写入：

- 最终助手消息或结构化产物引用。
- 消息使用的 evidence IDs 和 citation map。
- Run 终态、用量和阶段耗时。
- completed Event。

终态持久化失败时不得发送“已完成”事件。

## 4. 安全 Checkpoint

Checkpoint 必须是版本化、可校验的 resume state，而不是任意 Harness 内存快照。

```json
{
  "schemaVersion": 1,
  "executor": "official_drafting",
  "goalSummary": "起草会议通知",
  "completedStepIds": ["outline"],
  "pendingStepId": "draft",
  "evidenceIds": [12, 18],
  "requiredCapabilities": ["vault.search", "note.propose_patch"],
  "requiredPermissions": [],
  "pendingConfirmationId": null,
  "budgetRemaining": { "modelCalls": 2, "toolCalls": 8 },
  "safeState": {}
}
```

禁止保存：

- API Key、Token、解密密码或 Provider 鉴权头。
- 完整 system/user prompt。
- 笔记正文、完整选区、完整网页或工具原始响应。
- Provider 内部思维链和隐藏推理。
- 可被篡改后直接执行的未校验工具参数。

恢复前必须重新验证 Session、Vault、安全域、显式文档权限、Provider 可用性、能力预算和待确认操作。权限过期时回到 `awaiting_confirmation`，不能沿用旧决定。

## 5. Event 合同

统一事件外壳：

```ts
interface AssistantRunEvent {
  runId: string;
  seq: number;
  stateVersion: number;
  type: RunEventType;
  timestamp: string;
  payload: RunEventPayload;
}
```

首版稳定事件类型：

```text
accepted
stage_changed
content_delta
tool_started
tool_completed
confirmation_required
permission_denied
provider_switched
evidence_registered
paused
resumed
completed
failed
cancelled
```

- Event payload 只能包含 UI 所需摘要和稳定 ID。
- 前端按 `(run_id, seq)` 去重和补序。
- 发现序号缺口时调用 `assistant_run_get` 补齐，不推测缺失事件。
- `content_delta` 可以不持久化全部 token，但完成后的消息必须成为事实源。

## 6. 证据账本

普通域只有 `session_evidence` 是证据事实源。`session_messages`、Run、Step 和 checkpoint 只保存 evidence IDs。

### 本地证据

保存：

- Vault ID、相对路径、标题、heading path。
- 字符或字节区间及内容哈希。
- 资料角色、检索原因、分数和首次使用 Run。

不在证据表复制本地正文。复核时按路径和区间重读并校验哈希；内容变化后标记 stale，禁止静默用新正文替代历史证据。

### Web 证据

保存：

- URL、规范化 URL、域名、标题、获取时间。
- Provider/MCP 标识、提取方法、排名和原始结果哈希。
- 最终回答实际引用的有界摘录，不保存整页。
- 冲突组、失败原因和退役时间。

只有实际支撑回答的摘录进入长期会话账本；搜索候选仍按缓存保留策略清理。

### Citation map

最终助手消息保存结构化映射：

```text
answer span / claim id → one or more evidence ids
```

展示标签可以按 Session 递增，但逻辑引用必须使用 evidence ID，避免标签重排破坏历史。

## 7. Conversation Memory

Conversation summary 只保存：

- 用户目标与偏好。
- 已明确的决定。
- 仍待讨论的问题。
- 对话中明确引用过的资料 ID 摘要。

禁止保存或推断“当前文档”。编辑器切换不得触发 memory 更新。

## 8. 删除与保留

- 删除 Session 时级联删除普通域消息、Run、Step、Event 和 session evidence。
- Web 搜索缓存和页面缓存按独立策略清理，不能被当作历史证据事实源。
- 删除单条消息时，只有无其他消息引用的证据才可退役或删除。
- Classified Conversation 使用同一逻辑模型，但全部内容保存在 CEF 加密文件中。
- 普通 SQLite、日志、事件和崩溃报告不得保存涉密路径、正文或证据摘录。
