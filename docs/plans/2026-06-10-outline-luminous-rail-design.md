# Outline Luminous Rail 文档目录设计 v2

**日期**: 2026-06-10  
**状态**: 已废弃  
**被取代**: [Outline Ghost Spine 文档目录设计](./2026-06-10-outline-ghost-spine-design.md)

## 摘要

该方案曾尝试将目录设计为编辑画布左缘的 **光轨（Luminous Rail）**：窄轨 + 层级刻度 + 单条浮动标题。实际验证发现长文标题密集时刻度和标题位置不可读，已由 Ghost Spine 透明文字索引列取代。

## 交互

- 收起：竖线 + ListTree 把手（~1.5rem）
- 展开：刻度轨（~1.75rem）+ 悬停/擦洗时单条 caption
- 滚轮在轨上：擦洗章节索引，不滚动列表
- `Ctrl/Cmd+Shift+O` / Esc / 把手切换

## 层级

H1/H2/H3 用刻度宽度与粗细区分，非文字缩进列表。

## 非目标

全文列表、虚拟滚动、折叠树、目录内搜索。
