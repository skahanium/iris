# 09 - 路线图草案与开放问题

本文不承诺具体版本日期。正式纳入排期前必须同步根目录 `ROADMAP.md`。

## 可能的阶段划分

### Phase 0 - 研究与决策

目标：把本目录草案收敛成正式设计。

产出：

- 最终数据模型。
- 权限矩阵。
- 公式 DSL 范围。
- UI 信息架构。
- 性能目标。
- 测试策略。

### Phase 1 - Object 基础层

目标：不改变 Markdown 使用方式，建立 Note Object 和基础内置类型。

候选能力：

- objects / object_sources。
- Note Object 自动映射。
- Task / Project / Source / Person / Web / Attachment / Record 类型定义。
- 基础只读 IPC。
- 基础 Object query。

### Phase 2 - Collection 与 View 基础

目标：建立 Type Collection / Topic Collection 和 Grid / Board / List 基础。

候选能力：

- Collection CRUD。
- View CRUD。
- Grid 基础浏览。
- List 支持 Topic Collection。
- Board 基础按 status 分组。

### Phase 3 - Property 与多维表能力

目标：Custom Properties 与 Grid 强化。

候选能力：

- 字段定义。
- typed values。
- 筛选、排序、分组。
- 批量编辑 proposal。
- 统计行基础。

### Phase 4 - Formula / Compute

目标：Iris DSL 与本地高速计算。

候选能力：

- DSL parser / typechecker。
- dependency graph。
- formula cache。
- dirty marking。
- background compute jobs。
- formula editor 双入口。

### Phase 5 - Agent Proposal

目标：Agent 安全整理对象。

候选能力：

- object read tools。
- proposal tools。
- confirmation UI。
- audit log。
- Agent 从笔记提取任务 / 资料 / 项目 proposal。

### Phase 6 - Markdown 融合

目标：将 View 无痕嵌入文档。

候选能力：

- `iris-view` fenced block。
- TipTap embedded view atom。
- 失效引用 fallback。
- 可选 object id frontmatter 策略。

### Phase 7 - 性能与高级导入

目标：大表、大导入、复杂统计稳定。

候选能力：

- 大表性能基准。
- 导入 preview。
- 高成本计算调度。
- 聚合缓存。
- 性能诊断。

## 开放问题

### 产品

- 这个能力最终命名为 Collections、知识对象、资料库、还是 Iris Databases？
- 用户入口应该在 Home、管理中心知识库、还是独立 workspace？
- 普通用户是否需要知道 Object 这个词？

### 数据

- Object ID 是否写入 Markdown frontmatter？
- Topic Collection 是否支持动态查询？
- Core Properties 是否允许用户隐藏 / 重命名 / 禁用？
- Custom Property 是否 scoped 到 type 还是 collection？

### Agent

- 用户手动批量操作是否也统一走 proposal？
- proposal 是否支持 allow for session？
- Agent 生成公式时如何展示 sample rows？
- Agent 是否允许创建新 View？

### 公式

- DSL 最终语法。
- `today()` / `now()` 的 dirty 策略。
- relation rollup 最大深度。
- 公式错误值 UI。

### 性能

- Grid 是否需要引入新依赖？需先做 AGPL 兼容 license 检查。
- 是否需要列虚拟化。
- 10 万 records 是否作为硬目标。
- 后台 compute job 是否复用现有 scheduler 还是独立队列。

### Markdown

- Embedded View 是否默认只读？
- 导出 Markdown / HTML 时是否展开 embedded view？
- 外部编辑删除 view block 是否只删除引用？

## 下一步建议

先把 Phase 0 变成正式 spec，再进入实现计划。Phase 0 的重点不是写代码，而是把 schema、权限、公式、UI 和测试策略压实，避免后续做成一团。
