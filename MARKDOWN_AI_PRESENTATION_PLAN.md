# Markdown AI 展示重构设计方案

最后更新：2026-05-31

## 概述

本文档定义“Markdown 契约内核”之后的第三子项目：**AI 展示重构**。

该子项目的目标，是在共享 contract 基础上统一 AI 相关全部 Markdown 消费表面，把当前“助手消息能渲染一部分 Markdown、用户消息仍是纯文本、研究卡和附属卡片各自有自己的解释方式”的状态，提升为更接近 Codex 的一致展示体验。

当前主要问题包括：

- 用户消息发送后仍以纯文本展示，`**粗体**` 之类直接显示源码
- 助手消息、研究卡片、补丁预览、引用检查等区域没有共享统一的 Markdown 呈现模型
- `UnifiedAssistantPanel` 过于庞大，消息流、工件流、确认流、研究流耦合在一个组件中
- 部分工件表面仍以“手写结构”展示，而不是基于统一语义层组织信息
- AI 表面虽然有 Markdown 支持，但整体层级、节奏、密度和 Codex 风格仍有距离

## 设计目标

- 让 AI 所有主要表面都消费统一 Markdown contract
- 让用户消息与助手消息在核心 Markdown 语义上行为一致
- 把消息流和工件流区分清楚，但保持统一的语义呈现
- 提升信息层级、留白、代码块、引用、表格、任务列表等展示效果
- 在不破坏现有工作流的前提下，让 AI 区的整体阅读体验更接近 Codex

## 非目标

本子项目不负责：

- 重做 Markdown contract 本身
- 重写 TipTap 编辑器内核
- 扩展全部高级 Markdown 语法的编辑器可编辑能力

## 重构重点

### 1. 统一消息渲染模型

用户消息与助手消息都必须建立在统一 contract 之上。第一阶段至少做到：

- 用户消息发送后按 Markdown 渲染
- 助手消息继续按 Markdown 渲染，但与用户消息共享 profile 规则
- 系统消息仍可保持轻量文本样式，但需明确是否允许最小 Markdown

目标不是让所有气泡视觉完全一样，而是消除“相同 Markdown 在不同角色气泡里被不同解释”的问题。

### 2. 把 AI 表面拆成“消息流”和“工件流”

当前 AI 区实际上混合了两种内容：

- 会话消息流
- 工件 / 结果流

建议明确分层：

- `Conversation Surface`
  负责用户消息、助手消息、系统消息、流式内容
- `Artifact Surface`
  负责研究结果、补丁预览、引用检查、文档检查结果、执行计划等

这两层仍在同一个面板中协同，但不再让所有内容都硬塞进同一个“消息气泡”心智模型里。

### 3. 统一工件表面的 Markdown 语义

以下表面应逐步并入统一 contract 体系：

- `ResearchResultMessage`
- `PatchPreview`
- `CitationCheckView`
- 文档检查相关工件
- 执行计划与上下文说明类卡片

其中：

- 研究结果卡优先做成“Markdown 摘要 + 结构元信息”的混合卡
- 补丁预览保持 diff 样式，但补丁说明、警告、证据摘要等改由统一 Markdown 语义层驱动
- 引用检查保留结构化事实/证据关系，但解释文本、建议文本、claim 摘要等统一走 contract

### 4. 提升流式内容体验

AI 区要更接近 Codex，不能只“能渲染 Markdown”，还要让流式过程本身稳定、自然。

需要重点改善：

- 半截粗体、半截代码块、半截引用不直接暴露源码噪音
- streaming 中的消息更新不频繁抖动布局
- 流式结束后最终展示与完整解析结果平滑收敛
- thinking / working / tool confirmation / result 阶段的层次更清楚

### 5. 对齐更高质量的排版与视觉层级

在 contract 已统一的前提下，本子项目要把 AI 区展示提升到更像 Codex 的层次，重点不是仿造外观，而是提升以下质量：

- 标题、段落、列表、引用、代码块的排版层级
- 气泡与非气泡内容的边界感
- 研究卡片、补丁卡片、引用检查卡片的阅读节奏
- 表格与代码块的滚动、留白、视觉密度
- 选中、引用点击、工具确认、错误状态的反馈清晰度

## 关键设计决策

### 1. 不把所有内容都塞回纯消息气泡

工件类内容不应被强行伪装成普通聊天消息。对 Codex 风格的真正借鉴，应是“会话流与结果流各司其职，但仍在同一阅读上下文内协作”。

### 2. 允许同一面板内存在多种展示容器

建议至少包含：

- 标准消息气泡
- 研究结果卡
- diff/patch 卡
- 结构化检查卡
- 执行计划卡

统一点在于都消费同一 contract，而不是强制形态一致。

### 3. 优先拆大组件边界，而不是继续往 `UnifiedAssistantPanel` 堆逻辑

`UnifiedAssistantPanel` 当前承担：

- 路由
- 请求触发
- streaming
- 消息状态
- 工件状态
- 研究状态
- tool confirm / rule confirm
- session history

重构后应把“Markdown 展示”和“面板编排”解耦，避免后续每加一种展示形态都继续扩大单组件复杂度。

## 推荐组件边界

建议逐步拆出以下职责层：

### `ConversationSurface`

负责：

- 用户消息
- 助手消息
- 系统消息
- streaming 状态

### `ArtifactSurface`

负责：

- 研究结果卡
- patch diff 卡
- citation check 卡
- execution plan 卡
- 其他非纯聊天工件

### `MarkdownRenderable`

统一的 Markdown 展示壳，用于：

- 消息正文
- 卡片摘要
- 建议说明
- 工具/研究/引用相关文本说明

### `AiPanelStateModel`

抽象消息流、工件流、确认流和研究流的展示状态，减少 UI 直接依赖零散状态位。

## 与现有代码的关系

本子项目重点收编以下现有区域：

- `src/components/ai/AiMessageBubble.tsx`
- `src/components/ai/AiMessageList.tsx`
- `src/components/ai/ResearchResultMessage.tsx`
- `src/components/ai/PatchPreview.tsx`
- `src/components/ai/CitationCheckView.tsx`
- `src/components/ai/UnifiedAssistantPanel.tsx`
- 相关 Markdown render / sanitize / stream hooks

其中：

- `AiMessageBubble` 将不再只为 assistant 提供 Markdown 渲染
- `UnifiedAssistantPanel` 需要从“全能面板”逐步拆成编排层
- 附属卡片要开始共享统一的 Markdown 可渲染区域，而不是各自拼字符串和手写结构

## 测试计划

### 1. 消息渲染一致性测试

覆盖以下场景：

- 用户消息中的 `**粗体**`、列表、代码块、引用能正确渲染
- 助手消息与用户消息对相同 Markdown 语料的展示语义一致
- 系统消息在允许范围内的展示行为稳定

### 2. 工件表面一致性测试

覆盖以下场景：

- 研究结果摘要与主消息区对同一 Markdown 片段解释一致
- 补丁预览中的说明文本和警告文本走统一 Markdown 语义
- 引用检查中的说明、建议、claim 摘要在统一语义层下展示

### 3. 流式体验测试

覆盖以下场景：

- 半截 Markdown 在 streaming 中不退化成大段源码
- token 更新频率不会造成严重闪烁和布局跳变
- streaming 结束后最终内容正确替换并稳定

### 4. 面板编排测试

覆盖以下场景：

- 会话流与工件流共存时不相互覆盖
- tool confirm、rule confirm、execution plan、research progress 与消息展示层次清楚
- 切换会话、恢复 harness、重新发送时消息和工件状态不串线

## 完成标准

- AI 区主要 Markdown 表面全部消费共享 contract
- 用户消息发送后按 Markdown 渲染，不再以源码形式裸露
- 助手消息、研究卡、补丁卡、引用检查卡之间的 Markdown 解释不再明显分裂
- `UnifiedAssistantPanel` 的展示职责得到有效拆分，不再继续堆积所有 UI 细节
- AI 区在阅读层级、代码块、引用、表格、流式稳定性上明显接近 Codex 级体验

## 默认假设

- 会话流与工件流仍共存于统一助手面板中
- 不引入新的前端框架或编辑器框架
- contract 已经先行落地
- 视觉重构要服务于语义统一和阅读质量，而不是单纯追求外观相似
