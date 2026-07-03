# 07 - UI 与 View 体系

## 结论

View 类型收敛为三种核心类型：

```text
Grid
Board
List
```

Task、Calendar、Timeline、Gallery 都作为 preset 或 layout mode，不作为独立一等 View 类型。

## Grid

Grid 是多维表主视图。

能力：

- 大表虚拟滚动。
- 字段列展示。
- 冻结列。
- 排序、筛选、分组。
- 公式字段。
- 统计行。
- 批量编辑。
- 导入预览。
- 高成本计算状态。

Grid 是性能要求最高的视图。

## Board

Board 是按字段分组的卡片视图。

能力：

- 按 status/select/person/project 等字段分组。
- 拖动卡片修改分组字段。
- 支持 Task / Project / Source review flow。
- 拖动写入必须经过正常权限和 audit。

## List

List 是轻量对象流和上下文视图。

适合：

- Topic Collection。
- 今日任务。
- My Focus。
- 资料列表。
- 搜索结果沉淀。
- 混合 Note / Task / Source / Web / Attachment。

## Preset / Layout Mode

### Task List

```text
view_type = list
preset = task
group_by = due_bucket
quick_actions = complete, reschedule, status
visible_fields = title, project, priority, due_date
```

### Calendar / Timeline

```text
view_type = list 或 grid
layout = time
date_field = due_date
```

### Gallery

```text
view_type = list
layout = media
cover_field = preview
```

## Workspace 入口

待讨论：

- Collection workspace 是否作为新顶层 tab。
- 是否放入管理中心的知识库分区。
- 是否允许从 Markdown embedded view 进入完整 workspace。
- 是否需要全局 Collection switcher。

## 与现有界面原则的关系

必须符合 Iris 设计系统：

- 不做营销式 landing page。
- 不把数据库做成浮夸卡片堆。
- SaaS/CRM 类操作界面应安静、密集、可扫描。
- Grid / Board / List 控件应稳定尺寸，避免文本挤压和布局跳动。

## 待讨论

- Grid 具体组件选型：自研还是引入 AGPL 兼容依赖。
- 移动端不做，桌面窗口尺寸下的响应规则。
- Embedded View 默认只读还是轻编辑。
- 任务视图是否进入全局 Home / StatusBar。
