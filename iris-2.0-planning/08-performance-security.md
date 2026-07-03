# 08 - 性能、安全、审计与回滚

## 性能目标草案

这些是规划目标，不是当前承诺。

```text
10 万 records 的 Collection 可以打开。
1 万行 Grid 首屏 < 200ms。
普通筛选 / 排序 < 100ms。
单 cell 修改到 UI 局部刷新 < 50ms。
1 万行简单公式首次计算 < 300ms。
10 万行导入不阻塞 UI。
Agent 批量 proposal 必须先估算影响范围和重算成本。
```

## 性能机制

- 查询下推到 Rust / SQLite。
- 前端 Grid 行虚拟化，必要时列虚拟化。
- 后端分页 / windowed query。
- 核心字段专门索引。
- 高频扩展字段可索引。
- 公式结果缓存。
- 聚合缓存。
- 后台计算队列。
- UI 局部事件更新。

## 安全原则

- Agent 不执行裸 SQL。
- Formula DSL 不执行任意代码。
- 公式不能联网、读文件、读凭据或调用 Agent。
- API Key 继续只存 OS 凭据管理器。
- Object 写入必须经过 permission / proposal / audit。
- 高风险变更必须有明确回滚方式。

## 审计

每次确认后的写入需要记录：

```text
actor: user|agent
operation
before
after
proposal_id nullable
risk_level
created_at
reversible_by
```

## 回滚

待讨论的回滚层级：

1. 单字段 undo。
2. Proposal 级回滚。
3. Collection 批量操作回滚。
4. 删除进入回收站。
5. Markdown 正文仍使用现有版本系统。

Object 系统不应直接复用 Markdown 版本快照，但可以与版本系统并列展示。

## 导入安全

外部导入必须先进入 preview：

```text
file / web / clipboard / Agent extraction
  -> parse preview
  -> type inference
  -> conflict detection
  -> proposal
  -> confirm
  -> write
```

## MCP 安全边界

MCP 只允许作为外部 web evidence provider，映射到 `web.search` / `web.fetch`。

禁止 MCP 直接写 Iris Object / Collection / View。

## 待讨论

- Object audit 是否有保留期限。
- 是否支持 audit 压缩。
- 批量导入是否需要事务分片。
- 高成本公式是否允许用户强制前台等待。
- 是否需要 Collection 级性能健康面板。
