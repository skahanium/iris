# 01 - 核心概念

## Object

Object 是 Iris 识别和管理的用户可见知识对象。

它可以是：

- Note：笔记
- Task：任务
- Project：项目
- Person：人物
- Source：资料
- Web：网页
- Attachment：附件
- Record：通用记录

Object 不是 UI，也不等同于文件。它是一个稳定身份，带有类型、核心字段、扩展字段和可选正文来源。

示例：

```text
Object: Iris 多维表设计
kind: note
title: Iris 多维表设计
source_path: 产品/Iris 多维表设计.md
status: 设计中
priority: 高
```

## Property

Property 是 Object 的字段。

字段分为两类：

- Core Properties：Iris 内置核心字段，语义稳定，Agent 可理解。
- Custom Properties：用户或 Agent proposal 创建的扩展字段，用于多维表、统计和公式。

示例：

```text
Task core properties:
- title
- status
- priority
- due_date
- project
- source_note

Task custom properties:
- 预计工时
- 技术风险
- 剩余天数 formula
```

## Collection

Collection 是一组 Object 的组织方式。

Iris 2.0 采用双层模型：

1. Type Collection：按对象类型组织，字段一致，性能强。
2. Topic Collection：按主题、项目、研究方向组织，可以混合多种对象。

示例：

```text
Type Collection: Task
包含所有 Task 对象，适合 Grid / Board / 统计 / 公式。

Topic Collection: Iris 2.0 规划
包含 Note、Task、Source、Web、Attachment，适合作为上下文集合。
```

## View

View 是 Collection 的可保存查询、布局和统计配置。

Iris 只保留三种核心 View：

- Grid：结构化表格、多维表、公式、统计、批量编辑。
- Board：按字段分组的卡片流。
- List：轻量对象流、任务清单、Topic Collection 上下文。

Calendar、Timeline、Gallery、Task View 不作为独立一等 View 类型，而是 View preset 或 layout mode。

## Formula

Formula 是 Custom Property 的一种。Iris 使用安全 DSL，而不是 JavaScript、Excel 兼容层或 Notion 公式照搬。

公式必须：

- 强类型。
- 无副作用。
- 可解析依赖。
- 可估算成本。
- 可缓存结果。
- 可增量重算。

## Proposal

Proposal 是 Agent 或批量操作生成的结构化变更建议。

Agent 不能直接写入 Object / Collection / View。所有写入都必须先成为 proposal，经用户确认后由 Rust 后端执行。

Proposal 包含：

- 变更类型。
- 风险等级。
- diff。
- 影响对象数量。
- 公式 / 统计重算成本。
- 回滚或恢复方式。

## MCP

MCP 不是 Iris 内部 Object 系统协议。

MCP 只作为外部 web evidence provider，用于 `web.search` / `web.fetch`。Object / Collection / View 与 Agent 对接走 Iris 原生 Tool / Capability / Proposal 协议。
