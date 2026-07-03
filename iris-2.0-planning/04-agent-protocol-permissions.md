# 04 - Agent 协议与权限

## 结论

Object / Collection / View 不走 MCP。

内部对接走 Iris 原生 Tool / Capability / Proposal 协议。MCP 只作为外部 `web.search` / `web.fetch` 证据 provider 边界。

## 原因

Object 系统是 Iris 的核心本地数据层，必须受 Iris 自己的权限、审计、事务、公式预算和回滚约束。如果包装成 MCP，会把内部安全边界外部化，并削弱类型安全和确认机制。

## Agent 对接链路

```text
Agent Runtime
  -> ToolPolicy
  -> PermissionDecision
  -> Object Tools
  -> Proposal Engine
  -> User Confirm
  -> Object / Compute Service
  -> SQLite transaction
  -> Audit log
  -> UI event
```

## Tool 分层

### Read Tools

默认低风险，可按 task policy 暴露。

```text
object.query
object.get
object.resolve
collection.list
collection.get
collection.get_schema
view.query
view.summarize
formula.explain
```

### Proposal Tools

Agent 可调用，但只生成 proposal，不写入事实表。

```text
object.propose_create
object.propose_update
object.propose_bulk_update
object.propose_delete
collection.propose_create
collection.propose_update
property.propose_create
property.propose_change_type
property.propose_delete
formula.propose_create
formula.propose_update
view.propose_create
view.propose_update
```

### Apply Tools

不暴露给模型直接调用。只能由用户确认路径触发。

```text
proposal.preview
proposal.apply_confirmed
proposal.reject
```

## Proposal 格式草案

```json
{
  "proposalId": "prop_123",
  "kind": "object.bulk_update",
  "risk": "medium",
  "summary": "准备更新 24 个任务的状态",
  "changes": [
    {
      "objectId": "task_1",
      "field": "status",
      "before": "todo",
      "after": "in_progress"
    }
  ],
  "computeImpact": {
    "affectedFormulaFields": 2,
    "affectedRecords": 24,
    "estimatedCost": "low"
  },
  "reversibleBy": "audit log / previous values"
}
```

## 权限分级

### 自动读

```text
object.read
collection.read
view.read
formula.read
```

### 低风险确认

```text
view.create
view.update_layout
collection.add_member
object.update_single_field
```

### 中风险确认

```text
object.create
object.bulk_update
property.create
formula.create
formula.update
import.records
```

### 高风险确认

```text
object.delete
object.bulk_delete
property.delete
property.type_change
collection.delete
relation.bulk_rewire
formula.high_cost_update
```

## 禁止项

Agent 不允许：

- 执行裸 SQL。
- 直接写 Object 事实表。
- 绕过 proposal apply。
- 执行任意公式代码。
- 删除无恢复路径的数据。
- 未确认修改 Markdown 正文。
- 通过 MCP 写入 Iris 内部数据库。

## Capability 方向

候选新增 capability affinity：

```text
ReadObjects
QueryCollections
ProposeObjectChanges
ProposeViewChanges
ProposeFormulaChanges
```

候选新增 access level：

```text
ReadObjectIndex
ReadObjectDetail
WriteObjectProposal
WriteObjectSchema
WriteObjectDataInternal
```

`WriteObjectDataInternal` 不暴露给模型，只能由确认后的 proposal executor 使用。

## 与现有 ToolPolicy 的关系

当前 ToolPolicy 已经区分 read、write、network、settings、confirmation。Object tools 应并入该体系，不新建旁路。

Skills 仍然 prompt-only，不能扩大工具面。

## 待讨论

- 用户手动批量操作是否也统一走 proposal。
- 是否支持 session-level permission grant。
- 高风险确认文案和 UI 细节。
- proposal 过期策略。
- audit log 保留期限。
