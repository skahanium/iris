# Markdown 体系升级路线总索引

最后更新：2026-05-31

本文档汇总 Iris “对标 Codex 的 Markdown 体验升级”整体路线。

## 总目标

把 Iris 的 Markdown 体验提升到更接近 Codex 的水平，覆盖：

- 编辑器内的语义理解与可编辑能力
- AI 对话区的 Markdown 正确渲染
- 用户消息、助手消息、研究卡片、补丁预览、引用检查等表面的统一体验
- 编辑器与 AI 区之间的 Markdown 流转一致性
- 高级语法在“不破坏原文”前提下的展示、编辑与保存策略

## 拆分原则

考虑到当前项目存在多套 Markdown 解析与展示逻辑，本路线拆成三个连续子项目，避免一次性大改三个系统导致实现不可控。

### 子项目 1：Markdown 契约内核

文档：

- [MARKDOWN_CONTRACT_PLAN.md](./MARKDOWN_CONTRACT_PLAN.md)

目标：

- 统一 Markdown 解释权
- 建立能力分级、保留策略、流式修复策略
- 为编辑器、AI 区、预览区提供共享 contract

### 子项目 2：编辑器重构

文档：

- [MARKDOWN_EDITOR_REFACTOR_PLAN.md](./MARKDOWN_EDITOR_REFACTOR_PLAN.md)

目标：

- 在 contract 基础上扩展 TipTap ingest/export 和 schema
- 提升高级语法的可编辑能力
- 在保持 `.md` 权威与原文不破坏前提下增强 round-trip

### 子项目 3：AI 展示重构

文档：

- [MARKDOWN_AI_PRESENTATION_PLAN.md](./MARKDOWN_AI_PRESENTATION_PLAN.md)

目标：

- 统一 AI 所有 Markdown 消费表面
- 对齐用户消息与助手消息语义渲染
- 提升消息流、研究卡、补丁预览、引用检查等区域的展示层级与 Codex 风格体验

## 推荐执行顺序

1. 先完成 Markdown 契约内核
2. 再推进编辑器重构
3. 最后完成 AI 展示重构

原因：

- 编辑器和 AI 展示都依赖统一 contract
- 先做 contract 才能避免两个子项目继续各自解释 Markdown
- 编辑器的 ingest/export 与 AI 展示的 profile 都需要共享能力分级和原文保留规则

## 子项目依赖关系

### 编辑器重构依赖 contract

- 依赖共享能力分级
- 依赖 preserve-only 原文回吐能力
- 依赖 `editor_ingest` / `editor_export` profile

### AI 展示重构依赖 contract

- 依赖 `chat_assistant` / `chat_user` profile
- 依赖统一流式修复策略
- 依赖统一的 Markdown 渲染入口

### 编辑器重构与 AI 展示重构的关系

- 两者都消费 contract
- 不要求互相阻塞实现
- 但 AI 展示重构中的补丁预览、引用检查、研究卡片等展示策略，应以编辑器最终可接受的 Markdown 语义边界为基线

## 每个子项目的完成信号

### 子项目 1 完成

- 所有 Markdown 消费方开始依赖共享 contract
- 能力分级、原文保留和流式修复规则稳定

### 子项目 2 完成

- 编辑器对核心 GFM 和首批高级语法的 ingest/export 更稳定
- 高级语法即使暂不可完整编辑，也不会在保存时被破坏
- round-trip 与选择区、补丁回灌链路稳定

### 子项目 3 完成

- 用户消息与助手消息都正确渲染 Markdown
- AI 区所有主要表面的层级、节奏和信息结构显著提升
- 展示结果与编辑器和预览区的 Markdown 语义不再明显分裂

## 默认假设

- 技术栈保持不变：Tauri 2.x、Rust、React 19、TipTap、TailwindCSS + shadcn/ui
- 用户 `.md` 仍是唯一权威数据源
- 所有子项目都必须遵循“绝不破坏原文”的保存原则
- 允许通过阶段化交付逐步接近 Codex，不要求在一个子项目里同时解决全部问题
