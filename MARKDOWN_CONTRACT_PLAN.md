# Markdown 契约内核设计方案

最后更新：2026-05-31

## 概述

本文档定义 Iris “全面对标 Codex 的 Markdown 体验”路线中的第一子项目：**Markdown 契约内核**。

这个子项目的目标，不是立刻重做全部 UI，也不是一次性完成 TipTap 全量扩展，而是先统一全应用对 Markdown 的解释权。当前项目里，编辑器、AI 对话区、研究卡片、补丁预览、引用检查、Vault HTML 预览等表面存在多套 Markdown 解析与展示逻辑，导致：

- 同一种 Markdown 在不同区域表现不一致
- 用户消息与助手消息行为不一致
- 编辑器与 AI 区能力边界分裂
- 流式半成品容易退化成源码展示
- 后续能力扩展会继续各修各的

本子项目完成后，后续“编辑器重构”和“AI 展示重构”都必须建立在这份共享 contract 之上，而不是继续点状拼接新的 Markdown 规则。

## 设计目标

- 建立全应用共享的 Markdown contract
- 统一编辑器、AI 区、预览区、附属卡片对 Markdown 的能力判定
- 明确定义“支持渲染但不可安全 round-trip”和“必须原样保留”的语法处理方式
- 对流式 Markdown 建立统一容错策略
- 把“绝不破坏原文”作为保存链路默认原则

## 非目标

本子项目**不直接包含**以下工作：

- 完整视觉重做
- 完整 TipTap schema 扩展实现
- 所有附属卡片的最终样式定稿
- 一次性实现所有高级 Markdown 语法的完全可编辑支持

这些工作属于后续子项目，但它们必须消费本 contract 定义的规则和接口。

## 契约内核结构

建议将 Markdown contract 设计为四段流水线，而不是单一渲染函数。

### 1. Source Ingest

接收原始 Markdown 文本，并保留：

- 原始 Markdown 内容
- 来源表面
- 是否处于流式输入状态
- 可能影响策略的上下文信息

该层的职责是固定“原始事实”，而不是立即渲染。

### 2. Normalize / Classify

把输入语法片段划分为不同能力等级：

- `native`
  当前体系原生支持，允许稳定渲染与稳定 round-trip
- `render_only`
  可以高保真渲染，但暂不保证安全编辑和完整回写
- `preserve_only`
  必须原样保留，允许只读展示或占位展示，但不允许破坏原文
- `unsupported`
  当前无法安全支持，必须明确降级策略，不得默默吞掉

### 3. Preservation / Fallback

对 `render_only`、`preserve_only`、`unsupported` 三类内容建立明确规则：

- AI 区和预览区允许高保真渲染或只读展示
- 编辑器可以使用占位、只读块、能力提示或降级显示
- 保存时必须能吐回 preserve-only 原文
- 流式输入必须有显示容错，但不能污染持久化原文

### 4. Render Profiles

同一份 contract 输出不同表面的 render profile。首批至少包含：

- `chat_assistant`
- `chat_user`
- `editor_ingest`
- `editor_export`
- `vault_preview`

后续可扩展：

- `research_card`
- `patch_preview`
- `citation_panel`

设计重点不是“所有表面长得一样”，而是“所有表面对同一段 Markdown 的语义解释一致，差异仅来自 profile 能力边界”。

## 第一阶段覆盖范围

第一子项目的 contract 需要覆盖下列展示或流转表面：

- 编辑器页面
- AI 主消息时间线
- 用户消息展示
- 研究结果卡片
- 补丁预览
- 引用检查视图
- Vault HTML 预览

其中，第一子项目不要求所有表面都立即完成最终 UI 改造，但要求它们开始依赖同一套能力分级、降级策略和渲染入口。

## 高级语法优先级

在“尽量全量支持 Markdown”的总目标下，第一子项目先正式建模并优先考虑以下高级语法族：

- 指令类语法
- Callout / Admonition 类块
- 脚注

选择这三类的原因：

- 更贴近笔记和知识管理场景
- 更容易在“可渲染”和“不可破坏原文”之间建立清晰规则
- 对编辑器 ingest/export 和 AI 区展示的 contract 价值高

Mermaid、数学块、原生 HTML、自定义嵌入等能力不排除，但不作为第一优先建模对象。

## 保存原则

本路线默认采用以下保存原则：

**绝不破坏原文。**

具体含义：

- 如果某种语法当前不能被完整编辑，不等于可以被改写或吞掉
- AI 区和预览区可以先做到高保真显示
- 编辑器内允许临时使用占位、只读块、降级显示
- 但在 `editor_ingest -> editor_export` 过程中，原始 Markdown 结构必须可恢复

不采用“为了可编辑而自动正规化或改写原文”的默认策略。

## 公共接口建议

第一子项目应新增或重组为以下共享接口。

### `MarkdownProfile`

定义消费表面，首批包括：

- `chat_assistant`
- `chat_user`
- `editor_ingest`
- `editor_export`
- `vault_preview`

### `MarkdownCapabilityLevel`

能力等级枚举：

- `native`
- `render_only`
- `preserve_only`
- `unsupported`

### `MarkdownContractResult`

用于承载 contract 输出结果，至少包含：

- 规范化结果
- 保留原文片段
- 能力告警
- 流式修复信息
- 渲染产物元数据

### `renderMarkdownWithProfile(source, profile, options)`

统一渲染入口。所有 Markdown 展示表面应逐步迁移到这个入口，而不是继续直接调用各自的 `marked`、`markdown.ts` 或手写 HTML 逻辑。

### `serializePreservedMarkdown(...)`

在编辑器导出阶段用于恢复 preserve-only 原文，保证“不可完整编辑的语法”依然可以无损保存。

### `classifyMarkdownCapabilities(source, options)`

为编辑器和 AI 区提供能力判定，用于：

- 决定当前语法是否可编辑
- 决定是否需要只读块或占位展示
- 决定是否给出能力警告

## 实施变化

### 1. 建立共享 contract 层

新增统一的 Markdown 内核层，负责：

- ingest
- normalize/classify
- preservation/fallback
- render profiles

### 2. 统一 AI 区消息入口

AI 区必须停止“按消息角色分别决定是否走 Markdown”。

第一阶段要求：

- 用户消息发送后也走 Markdown 语义渲染
- 助手消息继续走 Markdown 渲染，但改为依赖统一 contract
- 主消息区与研究卡片不再各自决定 Markdown 解释方式

### 3. 统一编辑器 ingest/export 规则

编辑器仍可继续使用 TipTap，但其输入与输出规则必须改为消费 contract：

- `editor_ingest` 负责把 Markdown 安全导入到编辑器语义空间
- `editor_export` 负责吐回 Markdown，并优先保护原文
- 不再让编辑器独立定义“哪些语法算支持、哪些语法直接丢”

### 4. 统一预览与附属表面能力判定

`PatchPreview`、`CitationCheckView`、Vault 预览等表面，不要求第一阶段全部改成 Markdown 驱动，但要求它们开始使用同一能力分级与降级策略，避免继续形成新的“局部方言”。

### 5. 统一流式修复策略

当前零散的流式 Markdown 修复逻辑应提升为 contract 级规则。至少覆盖：

- 未闭合粗体
- 未闭合删除线
- 未闭合代码围栏
- 中途中断的列表
- 中途中断的引用
- 中途中断的高级语法标记

流式修复只用于展示容错，不得污染最终保存内容。

## 测试计划

### 1. 语义金样本测试

建立共享 Markdown 语料库，覆盖：

- 标题
- 粗体、斜体、删除线、行内代码
- 无序列表、有序列表、任务列表
- 引用块
- 表格
- 代码块
- 链接、图片
- 指令类语法
- Callout
- 脚注
- 基础语法与 preserve-only 语法混合语料

断言同一语料在不同 profile 下的能力判定一致。

### 2. 展示一致性测试

至少覆盖以下场景：

- 用户消息中的 `**粗体**` 发送后按 Markdown 展示
- 助手消息与用户消息在核心 Markdown 语义上等价
- 研究卡片与主消息区对相同 Markdown 片段的解释一致
- Vault 预览与 AI 区对相同基础语法的语义解释一致

### 3. 不破坏原文测试

至少覆盖以下场景：

- 含 Callout 的文档经过 `editor_ingest -> editor_export` 后原文不被破坏
- 含脚注的文档经过导入导出后结构保持可恢复
- 含 preserve-only 片段的文档不会在保存时被吞掉或错误改写
- 高级语法与普通 GFM 混排时，普通语法不会被副作用污染

### 4. 流式稳健性测试

至少覆盖以下场景：

- 半截粗体输入不会直接展示为长段源码噪音
- 半截代码块不会把 UI 渲染崩掉
- 半截脚注或 Callout 在流式过程里有稳定降级策略
- 流式修复完成后结果尽量收敛到完整输入的最终渲染结果

## 完成标准

第一子项目完成时，应达到以下标准：

- 用户消息和助手消息都能按同一 contract 正确渲染核心 Markdown
- 编辑器、AI 区、Vault 预览对同一语料的语义解释一致
- 高级语法即便暂不可编辑，也不会在保存后被破坏
- 所有 Markdown 消费表面开始依赖同一 contract，而不是继续各自解释 Markdown
- 后续子项目只是在这个 contract 上扩能力和做 UI，不再重新定义 Markdown 规则

## 后续子项目建议

在本 contract 子项目完成后，后续建议按以下顺序推进：

1. 编辑器重构
   目标：扩展 TipTap schema、增强 unsupported syntax 的保留与可编辑策略、补齐 round-trip
2. AI 展示重构
   目标：对齐 Codex 级视觉层级、消息体验、补丁预览与引用展示表现

这两个子项目都不得绕开本 contract 单独定义 Markdown 规则。

## 默认假设

- 当前技术栈保持不变：Tauri 2.x、Rust、React 19、TipTap、TailwindCSS + shadcn/ui
- 第一子项目只解决“统一解释权与保底机制”，不承诺一次性实现所有高级语法的完整可编辑支持
- 高级语法第一优先级为：指令、Callout、脚注
- 保存链路默认遵循“绝不破坏原文”
- 视觉完全对标 Codex 作为后续子项目推进，而不是在本子项目里与 contract 混做
