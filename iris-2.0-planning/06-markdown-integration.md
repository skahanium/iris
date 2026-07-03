# 06 - Markdown 与文档体系融合

## 原则

Markdown 继续是正文权威源。Object 系统不应把现有 `.md` 改造成专有数据库文件。

Object 属性、关系、视图、公式结果保存在 SQLite。Markdown 只保存正文和可选引用。

## 最小破坏方案

现有 Markdown 文件不需要迁移。

Iris 扫描 vault 时，可以为每篇笔记建立 Note Object：

```text
source_path: 产品/Iris 数据库设计.md
object_kind: note
title: Iris 数据库设计
```

这一步不需要写入 `.md`。

## View Embed

Markdown 中可以嵌入 View 引用。真实数据仍在 SQLite。

候选语法：

````md
```iris-view
collection = "research_sources"
view = "high_confidence"
```
````

优点：

- 外部编辑器可读。
- 不破坏 Markdown。
- 当前 Markdown parser 容易 preserve。
- Iris 内部可渲染成 embedded view。

## Object ID

是否写入 frontmatter 待讨论。

候选：

```yaml
---
iris_object_id: obj_123
---
```

不建议首版强制写入。只有在用户明确同意或需要跨重命名稳定绑定时才写入。

## Markdown Table 边界

现有 GFM table 继续作为排版表格。

多维表不是 Markdown table。多维表是 Collection 的 Grid View。

禁止把大量结构化记录塞进 Markdown table 后伪装成数据库。

## 编辑器影响

新增 `iris-view` fenced block 可以作为 TipTap atom / node view：

- 编辑器内展示 embedded view preview。
- 点击进入完整 Collection workspace。
- 保存时保持原始 fenced block。
- 无法解析或找不到 View 时显示安全 fallback。

## 外部编辑冲突

待讨论：

- 用户删除 embed block 时是否删除 View？倾向：只删除引用，不删除 View。
- 用户改 collection/view id 导致失效时如何修复？
- Object ID 未写入 Markdown 时，重命名如何保持绑定？
- 外部编辑修改 frontmatter 与 SQLite 属性冲突时谁优先？

## 与数据原则的关系

AGENTS.md 要求 `.md` 是笔记知识权威来源。Object 系统不能违反这一点：

- 正文知识仍以 `.md` 为权威。
- 结构化应用数据以 SQLite 为权威。
- 若结构化属性需要写回 Markdown，必须用户明确同意。

## 待讨论

- frontmatter import/export 策略。
- 属性是否可选择性同步到 frontmatter。
- embedded view 是否允许轻编辑。
- Markdown 导出时是否展开 embedded view 为静态表格。
