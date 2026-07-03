# 02 - 总体架构

## 分层

Iris 2.0 候选架构分为七层：

```text
Document Layer
  -> Object Layer
  -> Collection Layer
  -> Property Layer
  -> Formula / Compute Layer
  -> View Layer
  -> Agent Proposal Layer
  -> Permission / Audit Layer
```

这些层不互相替代。Markdown 仍管理正文，Object 管结构化身份，View 管展示，Agent 管建议，Permission / Audit 管安全边界。

## Document Layer

负责：

- Markdown 文件正文。
- 媒体 / PDF / 附件文件。
- 可读网页缓存。
- 编辑器 Markdown round-trip。

不负责：

- 多维表真实数据。
- 字段定义。
- 公式结果。
- View 配置。

## Object Layer

负责：

- 用户可见对象身份。
- 内置对象类型。
- Object 与 Markdown / 文件 / 网页 / 附件的绑定。
- Object 生命周期。

不负责：

- Grid / Board / List 渲染。
- 公式求值。
- Agent 权限判断。

## Collection Layer

负责：

- Type Collection。
- Topic Collection。
- Collection membership。
- 静态成员与动态查询的边界（待讨论）。

不负责：

- 具体视图布局。
- 公式执行。

## Property Layer

负责：

- 字段定义。
- 字段类型。
- 字段值。
- Core Properties 与 Custom Properties。
- 字段索引策略。

不负责：

- 公式 DSL 解释。
- View 聚合展示。

## Formula / Compute Layer

负责：

- Iris DSL parse / typecheck。
- 依赖图。
- dirty 标记。
- 公式结果缓存。
- 统计与 rollup。
- 后台重算队列。

不负责：

- Agent 是否可以写入。
- UI 如何展示表格。

## View Layer

负责：

- Grid / Board / List。
- filter / sort / group_by。
- visible properties。
- layout mode。
- aggregation 配置。
- Markdown embedded view 引用。

原则：View 不拥有数据。

## Agent Proposal Layer

负责：

- Agent 查询 Object / Collection / View。
- 生成 object / property / formula / view proposal。
- 生成批量变更 diff。
- 等待用户确认。

不负责：

- 绕过确认直接写数据库。
- 执行裸 SQL。

## Permission / Audit Layer

负责：

- 工具暴露策略。
- 写入确认。
- 风险分级。
- 审计日志。
- 回滚入口。

## 与现有架构的关系

现有文件索引、搜索、AI runtime、ToolPolicy、PermissionDecision、MCP web evidence provider 都可以保留。

新增 Object 系统应作为主线能力进入 Rust 后端和类型安全 IPC，而不是作为第三方插件、MCP tool 或 Skills 能力注入。
