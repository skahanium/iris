# Outline Ghost Spine 文档目录设计

**日期**: 2026-06-10  
**状态**: 已批准实施  
**范围**: `EditorOutline`、outline CSS token、契约测试

## 摘要

废弃 minimap 刻度 + 悬停标签云，以及带边框毛玻璃的卡片式目录面板。新方案 **Ghost Spine（幽灵书脊）**：收起为编辑区左缘细竖线把手，展开为贴附画布的透明文字索引列，契合 Iris 沉静画布与鼠尾草绿边缘控件语言。

## 设计原则

1. **导航是 Chrome** — `font-sans` 12–13px，非 prose/衬线
2. **无容器感** — 无边框、无 blur、无阴影；`text-shadow` 保证可读
3. **鼠尾草绿仅标记当前章节** — `--outline-rail-active` 2px 左缘 marker
4. **不依赖 hover** — 展开/收起靠把手、`Ctrl/Cmd+Shift+O`、命令面板
5. **长文可扫** — 50+ 条目使用 `@tanstack/react-virtual` 窗口化

## 状态

### 收起（Spine Handle）

- 2px 竖线 + `ListTree` 图标，宽 ~1.5rem
- 不推挤正文（无 `iris-editor-outline-open`）

### 展开（Ghost Index）

- 宽 `12rem`，`max-height: min(78dvh, 36rem)`
- 平面列表 + H1/H2/H3 缩进，单行 `truncate`，`title` 显示全文
- 当前项 `aria-current="location"`，自动 `scrollIntoView`
- 推挤正文 `padding-left`（`iris-editor-outline-open`）

## 交互契约

| 项 | 行为 |
|----|------|
| `Ctrl/Cmd+Shift+O` | 切换目录（命令面板 chord） |
| `toggle-outline` 命令 | 同上 |
| `iris-outline-open` | localStorage 持久化 |
| Zen | 隐藏目录轨 |
| 列表键盘 | ↑/↓ 移动焦点，Enter 跳转，Esc 收起 |

## 数据层

不变：[`document-outline.ts`](../../src/lib/document-outline.ts) 提取 H1–H3。

## 非目标

- 折叠树、minimap、目录内搜索、H4+
