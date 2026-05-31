# Markdown 编辑器重构设计方案

最后更新：2026-05-31

## 概述

本文档定义“Markdown 契约内核”之后的第二子项目：**编辑器重构**。

该子项目的目标，是在共享 contract 基础上重构 TipTap 侧的 Markdown ingest/export、语义保留和可编辑能力，让编辑器不再只是一个“对部分 GFM 有效、遇到高级语法就 best-effort”的区域，而是成为一个对高级 Markdown 更稳、更可控、且不会破坏原文的主编辑环境。

当前现状的关键限制包括：

- `gfm-schema.ts` 只把一部分 GFM 定义为稳定支持
- `footnotes`、`math`、`raw HTML` 等仍属于 best-effort 或 plain text
- 编辑器导入与导出依赖独立 `markdown.ts` 往返逻辑
- 高级语法还没有统一的 preserve-only / render-only 策略
- AI 补丁回灌和编辑器保存最终都依赖当前 round-trip 边界

## 设计目标

- 让编辑器正式消费 Markdown contract，而不是独立解释 Markdown
- 扩展编辑器对首批高级语法的 ingest/export 能力
- 在不破坏原文前提下提高“可编辑”和“可见”的上限
- 让 AI 回灌、手动编辑、自动保存、版本保存共用同一套导出规则
- 把当前 `best-effort` 语法边界收敛成可测试、可解释的行为

## 非目标

本子项目不负责：

- AI 对话区视觉和消息层级重做
- 所有附属卡片的最终 UI 重构
- 一次性支持所有非 GFM 语法的完全可编辑体验

## 重构重点

### 1. 编辑器 ingest/export 改为 contract 驱动

编辑器必须从“直接把 Markdown 转成 TipTap 可吃的 HTML”升级为“先由 contract 判定能力，再决定如何进入编辑器”。

第一阶段要求：

- `editor_ingest` 负责导入时的能力分类
- `editor_export` 负责保存时的原文恢复和规范输出
- 编辑器不得继续自己决定哪些语法直接吞掉或平面化

### 2. 建立高级语法的块级策略

对首批高级语法建立明确的块级策略：

- 指令类语法
- Callout / Admonition
- 脚注

每类语法至少要明确：

- 是否可在编辑器内完整编辑
- 是否仅可只读展示
- 是否需要占位块
- 保存时如何恢复原文
- 选区复制、AI 补丁、版本保存时怎样处理

### 3. 设计 preserve-only 容器节点

对暂不可完整编辑的语法，不建议继续简单当纯文本吞进去。编辑器应引入 preserve-only 级别的容器策略：

- 在文档树中有明确边界
- 对用户可见，不伪装成普通段落
- 允许只读预览或轻量元信息展示
- 导出时可无损恢复原始 Markdown

这类容器不要求第一版支持复杂内部编辑，但必须保证：

- 不被普通格式化破坏
- 不在自动保存时丢失
- 不在 AI 回灌后结构错位

### 4. 统一 AI 回灌与编辑器落盘链路

当前 AI 补丁回灌最终会重新把 Markdown 回灌编辑器。重构后，这条链路必须建立在 contract + editor profile 之上：

- AI 补丁生成的新 Markdown 先过 contract
- 编辑器 ingest 时识别 preserve-only / render-only 片段
- 自动保存和手动保存统一走 `editor_export`
- 版本系统、回收站恢复、文档检查回写都复用同一条导出链路

### 5. 扩展可编辑能力，但不牺牲原文安全

编辑器重构不只是保底，也要扩上限。建议优先提升：

- 脚注的可识别与最小可编辑支持
- Callout 的块级表示与可编辑标题/内容边界
- 指令类语法的结构化识别
- 更复杂的引用、列表、表格混排稳定性

但默认原则仍是：**原文安全优先于激进可编辑化。**

## 关键设计决策

### 1. 不全面换掉 TipTap

继续使用 TipTap/ProseMirror，但增强节点策略与 ingest/export 管线，而不是整体更换编辑器框架。这样能保持现有基础能力和工程连续性。

### 2. 不以“正规化原文”换取编辑体验

对 preserve-only 语法，默认不把用户原文改写成“更适合 TipTap”的 Markdown 变体。只有在 contract 明确标记为 safe normalization 的场景下，才允许做可逆正规化。

### 3. 编辑器内部允许能力分层

同一份 Markdown 在编辑器里可以有三种存在方式：

- 原生可编辑块
- 结构化只读 / 半只读块
- 明确的 preserve-only 容器

目标不是强行让所有语法都假装可编辑，而是让它们在用户看来“被理解了”，并且保存安全。

## 公共接口与类型变化

编辑器子项目建议新增或扩展以下接口：

### `EditorMarkdownBlockKind`

用于标记编辑器内部块的来源和能力类别，例如：

- `native_block`
- `render_only_block`
- `preserve_only_block`
- `contract_warning_block`

### `EditorIngestResult`

用于承载导入结果，至少包含：

- TipTap 初始内容
- preserve-only 片段映射
- contract 能力告警
- 需要展示给用户的降级信息

### `EditorExportResult`

用于承载导出结果，至少包含：

- Markdown 正文
- 是否含 preserve-only 恢复片段
- 是否发生 contract 降级
- 可用于版本系统和 AI 回灌的元信息

### `ingestMarkdownForEditor(source, options)`

由 contract 驱动，生成适合 TipTap 的编辑器输入结果。

### `exportEditorToMarkdown(editorState, options)`

统一的编辑器导出入口，供自动保存、版本保存、AI 回灌、导出、预览等复用。

## 与现有代码的关系

本子项目需要重点收编以下现有区域：

- `src/lib/markdown.ts`
- `src/lib/serialize-open-note.ts`
- `src/components/editor/TipTapEditor.tsx`
- `src/components/editor/gfm-schema.ts`
- 各类 TipTap extension

其中，`gfm-schema.ts` 将从“静态说明当前支持列表”升级为“与 contract 对齐的编辑器能力声明面”。

## 测试计划

### 1. round-trip 扩展测试

在现有 `editor-real-roundtrip` 基础上，新增：

- 脚注 round-trip
- Callout round-trip
- 指令类语法 round-trip
- preserve-only 片段 round-trip
- 混合内容 round-trip

### 2. preserve-only 安全测试

覆盖以下场景：

- preserve-only 语法导入后不丢失边界
- 自动保存不破坏 preserve-only 原文
- 版本保存和恢复后原文仍可恢复
- AI 补丁回灌后 preserve-only 块不串位

### 3. 编辑行为测试

覆盖以下场景：

- 在 preserve-only 块前后输入普通文字不破坏结构
- 删除、复制、粘贴、选区跨越特殊块时不产生脏导出
- 章节折叠、标题字段、AI stream 节点与高级语法块共存稳定

### 4. 导出一致性测试

覆盖以下场景：

- 编辑器导出、Vault HTML 预览、AI 回灌后落盘使用同一导出语义
- `editor_export` 与 contract 语义等级一致
- 对同一语料重复导入导出结果稳定

## 完成标准

- 编辑器 ingest/export 正式改为 contract 驱动
- 首批高级语法拥有明确的编辑器策略，而不是继续落在 best-effort 灰区
- preserve-only 原文在自动保存、版本系统、AI 回灌链路中保持安全
- 现有核心 GFM round-trip 不回退
- 编辑器不再是整个 Markdown 体系里最容易破坏原文的一环

## 默认假设

- 仍然使用 TipTap/ProseMirror
- 用户 `.md` 为唯一权威数据源
- preserve-only 容器第一阶段允许只读或半只读，不强求复杂内部编辑
- 脚注、Callout、指令类语法为首批编辑器增强重点
