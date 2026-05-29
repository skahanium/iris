# Iris 设计系统

**方向定稿**：主攻 **N · Notion 编辑**；备选 **C · 命令优先**（键盘与可收起面板，不抢编辑区）。

**排期**：阶段与路线图版本的绑定见下文「落地阶段与路线图版本对照」；**版本 checklist 以 [ROADMAP.md](../ROADMAP.md) 为准**。

本文档是 **界面** 的单一参考；交互线框见 [ARCHITECTURE.md](../ARCHITECTURE.md)；全库文档索引见 [docs/README.md](./README.md)。

**v0.4.0-ui 施工计划**：[plans/2026-05-27-notion-ui-rebuild.md](./plans/2026-05-27-notion-ui-rebuild.md)

---

## N · Notion 编辑（主方向）

### 气质

内容优先：编辑区与外壳**同色扁平**，无浮动纸页、无行线网格。AI 是校对台与侧栏助手（280px），不是聊天 App 主屏。

### 分区

| 区域              | 角色                              | 默认观感                                 |
| ----------------- | --------------------------------- | ---------------------------------------- |
| **Chrome**        | 标签栏、状态栏、AI 侧栏、命令浮层 | 中性灰阶，细 `1px` 分隔，小圆角（4–8px） |
| **Editor canvas** | 居中内容栏约 `45rem`，与背景同色  | 无衬线正文、左对齐文档标题               |
| **Accent**        | 链接、主按钮、AI 标识             | 中性蓝灰（**不用** violet 紫、赭铜）     |

### 色彩 token（CSS 变量）

实现见 `src/styles/globals.css`。详细参考 [design-system/notion-master.md](./design-system/notion-master.md)。

**品牌 monogram**（几何「I」v3；桌面图标、顶栏、托盘、欢迎页）：见 [design-system/brand.md](./design-system/brand.md)。

| Token                | 亮色 `.light`                           | 暗色 `:root`        | 用途                       |
| -------------------- | --------------------------------------- | ------------------- | -------------------------- |
| `--background`       | 纯白附近                                | `#191919` 附近      | 壳层、编辑区、侧栏         |
| `--foreground`       | 深灰字                                  | 浅灰字              | 正文、标题                 |
| `--primary`          | `hsl(210 12% 45%)`                      | `hsl(210 18% 62%)`  | 主操作、链接、caret        |
| `--panel` / `--card` | 略区别于 background                     | 略区别于 background | 标签选中、浮层、输入       |
| `--editor-*`         | 与 `--background` / `--foreground` 对齐 | 同上                | 兼容旧 `editor-paper` 类名 |

### 字体

| 场景                  | 栈          | 说明                  |
| --------------------- | ----------- | --------------------- |
| **全文（UI + 编辑）** | `font-sans` | `Inter` + 系统无衬线  |
| **代码块**            | `font-mono` | JetBrains Mono 等等宽 |

### 间距与栏宽

- 编辑区：`max-width: 45rem`，水平 `clamp(1.5rem, 5vw, 6rem)`，正文 `16px` / `line-height: 1.5`
- AI 侧栏：固定 `280px`，可 `Ctrl+Shift+A` 收起

### 编辑区结构

```
.iris-editor
  └── .iris-editor-zoom-scroll（滚动）
        └── .iris-editor-canvas（居中栏 + zoom）
              └── .iris-editor-body（左侧为折叠钮留白）
                    └── .ProseMirror
```

**无** `.iris-paper` 卡片、**无** 行线 `repeating-linear-gradient`。

### 文档与块样式

| 元素         | 规则                                                           |
| ------------ | -------------------------------------------------------------- |
| **文档标题** | `noteTitle`，左对齐、`~2.25rem` bold，与正文同背景             |
| **章节标题** | H1 `1.875rem` / H2 `1.5rem` / H3 `1.25rem`；块间距用 `em` 分级 |
| **段落**     | 无段首缩进                                                     |
| **章节折叠** | H1–H3 左侧 `▸/▾`，`noteTitle` 不可折叠                         |
| **Zen**      | `Ctrl+.` 隐藏 Tab/状态栏/AI，栏宽 `56rem`                      |
| **缩放**     | canvas `zoom` 75%–150%                                         |
| **悬浮目录** | `EditorOutline`，`Ctrl+Shift+O`                                |

---

## 命令浮层（Command Overlay）

快捷键唤起的次级 UI **统一为居中浮层**，禁止右侧贴边长条 `SidePanel` 形态。

### 行为契约

| 规则     | 说明                             |
| -------- | -------------------------------- |
| **蒙层** | 全窗 scrim；盖住含 AI 在内的整窗 |
| **AI**   | 打开浮层时 **不自动收起** AI     |
| **互斥** | **同时仅一个** 命令浮层          |
| **关闭** | `Esc`、点击 scrim、显式关闭按钮  |

### 尺寸变体（`IrisOverlay`）

| size      | 用途               | 约略尺寸               |
| --------- | ------------------ | ---------------------- |
| `compact` | Quick Open         | `max-w-xl`             |
| `command` | 搜索、文件、设置等 | `max-w-3xl`，高 `78vh` |
| `wide`    | 版本时间线         | `max-w-7xl`，高 `88vh` |
| `graph`   | 知识图谱           | 宽 `96vw`，高 `92vh`   |

浮层：`rounded-xl`（12px），`--shadow-overlay`，`border-border/60`。

---

## 圆角、阴影与动效

| Token             | 值   | 用于                                    |
| ----------------- | ---- | --------------------------------------- |
| `--radius-sm`     | 6px  | chip、小控件                            |
| `--radius-md`     | 8px  | 输入、按钮                              |
| `--radius-lg`     | 12px | 卡片、工具条                            |
| `--radius-xl`     | 16px | 命令浮层                                |
| `--window-radius` | 12px | 无边框窗口外轮廓（配合 `shadow: true`） |

桌面窗口：`decorations: false`、`shadow: true`；单行 `TabBar`（Tab + 应用操作 + 窗口按钮，与 `bg-panel` 同色），避免系统黑条。

- **Windows 11**：`transparent: false`（见 `tauri.windows.conf.json`），圆角由 DWM + `shadow` 提供；**勿**与 `transparent: true` 同开。
- **macOS**：`transparent: true` + `set_effects`（`radius` = `--window-radius`）+ 前端 `data-iris-desktop-transparent` 裁切，见 `window_chrome.rs`。

阴影：仅浮层 / 悬浮工具条使用 `--shadow-overlay` / `--shadow-floating`；**编辑区无纸页阴影**。

动效：150–200ms，`prefers-reduced-motion` 降级。

### Chrome 表面与命令/AI token

| Token                                                         | 用途                                                |
| ------------------------------------------------------------- | --------------------------------------------------- |
| `--surface-chrome`                                            | TabBar、StatusBar、侧栏壳                           |
| `--surface-elevated`                                          | 浮层、popover                                       |
| `--surface-inset`                                             | 输入底、列表 hover 底                               |
| `--command-highlight-bg` / `--command-highlight-ring`         | 命令列表选中（浅底 + inset ring，非大面积 primary） |
| `--ai-user-bg` / `--ai-assistant-border` / `--ai-composer-bg` | AI 对话与输入区                                     |
| `--ai-stream-pulse`                                           | 流式等待指示                                        |

---

## Chrome 控件选型

| 场景       | 形态                                                                |
| ---------- | ------------------------------------------------------------------- |
| AI 场景    | `SceneSelector` 弹出（图标 + 描述）                                 |
| AI 发送    | `AiComposer` 多行；Enter 发送、Shift+Enter 换行                     |
| 证据包     | 可折叠 Section 标题 + badge                                         |
| 状态栏缩放 | Popover 滑块/步进（非三个并排按钮）                                 |
| 连通性     | 两枚 8px 圆点成组（LLM · 联网）；灰 / emerald / sky（`--status-*`） |
| 命令列表   | `CommandListOption` + `Kbd`；Lucide 图标                            |
| `/` 菜单   | 与命令列表同组件，禁止 emoji 图标                                   |
| 选区 AI    | 水平 pill 组 +「更多」                                              |

主路径保留可见控件或快捷键；StatusBar 避免超过 3 个并排 icon-only 按钮。

---

## AI 组件

- **引用卡**：`border-border`，`rounded-lg`，细 primary 边
- **对话泡**：用户 `bg-muted/60`，助手 `border` + `bg-card/60`，`rounded-lg`
- **流式节点**：与 primary 同系，无紫色渐变

---

## C · 命令优先（备选原则）

| 原则        | 现状                          |
| ----------- | ----------------------------- |
| 命令面板    | `Ctrl+Shift+P` 总览并执行功能 |
| 导航        | `Ctrl+P` Quick Open           |
| 次级功能    | 居中命令浮层                  |
| **AI 侧栏** | `Ctrl+Shift+A`                |
| Zen         | `Ctrl+.`                      |

---

## 非目标（视觉）

- 纸墨浮纸、信纸行线、衬线正文、段首缩进
- 紫色渐变、聊天主屏化
- 第三方主题 / 插件换肤

---

## 已废弃：B · 纸墨编辑 / 信纸（Letterhead）

v0.4.0-ui 起不再作为验收标准。历史实现含：`.iris-paper`、赭铜 accent、Noto Serif、`repeating-linear-gradient` 行线、`text-indent: 2em`。勿在新代码中引用。

---

## 落地阶段与路线图版本对照

| 设计阶段 | 路线图版本         | 内容                                        | 状态                      |
| -------- | ------------------ | ------------------------------------------- | ------------------------- |
| **0**    | **v0.1.1**         | 初版 token、AI 侧栏收起                     | 已完成（已被 N 取代视觉） |
| **1**    | **v0.2.0**         | 引用卡、`/` 菜单、图谱/标签                 | 已完成                    |
| **1.5**  | **v0.3.1-ui**      | 命令浮层基础设施                            | 部分 / 样式并入 v0.4.0-ui |
| **N**    | **v0.4.0-ui**      | Notion 扁平编辑、去行线、Inter、蓝灰 accent | **进行中**                |
| **N+**   | **v0.4.1-ui**      | Chrome 现代化：命令面板、AI、浮层原语       | **进行中**                |
| **2**    | **v1.0.0**（按需） | 标签栏自动隐藏、高对比主题                  | 待做                      |

---

## 参考

- **路线图**：[ROADMAP.md](../ROADMAP.md)
- **Notion 参考摘要**：[design-system/notion-master.md](./design-system/notion-master.md)
- **交互线框**：[ARCHITECTURE.md](../ARCHITECTURE.md)
