# 03 - 数据模型草案

本文是候选模型，不是最终 schema。正式落地必须通过增量 migration，并提供 down 脚本。

## 原则

- Markdown 正文不迁入 SQLite 专有格式。
- SQLite 是结构化对象、字段、关系、视图、公式和缓存的权威源。
- Object 系统与现有 files/chunks/links/tags 索引层分离，但可以互相引用。
- AI runtime 内部表不进入 Object 层。

## 核心实体

候选表族：

```text
objects
object_sources
object_core_fields
object_property_values
object_relations
collections
collection_members
views
view_configs
property_definitions
formula_definitions
formula_dependencies
formula_value_cache
aggregate_cache
object_change_proposals
object_change_audit
compute_jobs
```

## Objects

职责：对象身份和类型。

候选字段：

```text
id
kind note|task|project|person|source|web|attachment|record
title
status archived|active|deleted
created_at
updated_at
```

## Object Sources

职责：对象与外部/文档来源绑定。

示例：

```text
object_id
source_kind markdown|media|web|attachment|manual
source_path
source_url
content_hash
```

Note 对象通常绑定 `.md`。Record 对象可以没有正文来源。

## Property Definitions

职责：字段定义。

候选字段：

```text
id
owner_scope type|collection
owner_id
name
display_name
value_type text|number|boolean|date|select|multi_select|status|url|object_relation|file_relation|formula|rollup
is_core
is_required
is_indexed
config_json
created_at
updated_at
```

## Property Values

待讨论：存储策略。

候选方案：

1. typed value table：按类型拆列，例如 value_text/value_number/value_date/value_json。
2. per-kind optimized columns：核心字段落专门列，扩展字段走 typed value table。
3. JSON-only：实现简单，但性能和索引较弱，不推荐作为主路径。

倾向：核心字段专门优化 + 扩展字段 typed value table。

## Collections

两类 Collection：

```text
type_collection: Task / Source / Project 等类型库
topic_collection: 混合对象集合
```

候选字段：

```text
id
kind type|topic
object_kind nullable
name
description
membership_mode static|query|mixed
query_json nullable
created_at
updated_at
```

## Views

View 是 Collection 的保存查询和展示配置。

候选字段：

```text
id
collection_id
view_type grid|board|list
name
filter_json
sort_json
group_json
layout_json
visible_properties_json
aggregation_json
created_at
updated_at
```

## Formula

候选表：

```text
formula_definitions:
  property_id
  expression
  result_type
  ast_json
  cost_class low|medium|high
  created_at
  updated_at

formula_dependencies:
  formula_property_id
  depends_on_property_id
  relation_path_json

formula_value_cache:
  object_id
  property_id
  value_type
  value_*
  dirty
  computed_at
```

## Proposal / Audit

所有 Agent 写入和高风险用户批量操作先进入 proposal。

```text
object_change_proposals:
  id
  actor user|agent
  kind
  risk_level low|medium|high|critical
  summary
  changes_json
  compute_impact_json
  status pending|applied|rejected|expired
  created_at
  applied_at

object_change_audit:
  id
  proposal_id nullable
  actor
  operation
  before_json
  after_json
  created_at
  reversible_by
```

## 仍需讨论

- Object ID 是否写入 Markdown frontmatter。
- Topic Collection 是否支持动态查询。
- 删除策略：回收站、软删除、硬删除的边界。
- 关系字段是否允许跨任意内置类型。
- 大表索引策略是否自动化。
- 是否需要对象级版本快照，或依赖 audit + proposal。
