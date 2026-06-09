# Iris 设计系统

**方向定稿**：**Iris 知识工作台**——Iris Rail 壳层 + 沉静编辑画布 + AI 协作侧车 + 任务舱 Overlay；备选 **C · 命令优先**（键盘与可收起面板，不抢编辑区）。历史 Notion 参考见 [design-system/notion-master.md](./design-system/notion-master.md)（**不作**当前验收依据）。

**Iris Rail 完整刷新设计**：见 [docs/superpowers/specs/2026-06-09-iris-rail-refresh-design.md](./superpowers/specs/2026-06-09-iris-rail-refresh-design.md)。该 spec 是当前界面系统刷新目标态，覆盖 Home、顶栏、Rail Segments Tab、Editor Workspace、Outline Rail、AI Conversation Workspace、Overlay Family 与 AI 系统中心。

手工验收清单：见 [Iris Rail Refresh Manual Checklist](./testing/iris-rail-refresh-manual-checklist.md)。

**排期**：阶段与路线图版本的绑定见下文「落地阶段与路线图版本对照」；**版本 checklist 以 [ROADMAP.md](../ROADMAP.md) 为准**。

本文档是 **界面** 的单一参考；交互线框见 [ARCHITECTURE.md](../ARCHITECTURE.md)；全库文档索引见 [docs/README.md](./README.md)。

**v0.4.0-ui 施工计划**：[history/2026-05-27-notion-ui-rebuild.md](./history/2026-05-27-notion-ui-rebuild.md)

---

## Iris 编辑与协作（主方向）

### 气质

内容优先：编辑区与外壳**同色扁平**，无浮动纸页、无行线网格。**AI 侧栏**为与笔记并列的**协作对话区**（默认约 360px、可拖拽调宽），承载多轮对话、检索与长文回复，视觉精致、可读，而非弱化附庸或粗糙 IM。

Iris Rail 刷新后，识别度来自结构签名而非装饰：常驻品牌轨道、现代化 Rail Segments Tab、边缘目录轨、沉静编辑画布、AI 协作侧车和统一任务舱 Overlay。落地可以按风险分批，但设计目标一次性覆盖完整界面系统。

### 分区

| 区域              | 角色                              | 默认观感                                    |
| ----------------- | --------------------------------- | ------------------------------------------- |
| **Chrome**        | 标签栏、状态栏、AI 侧栏、命令浮层 | 中性灰阶，细 `1px` 分隔，小圆角（4–8px）    |
| **Editor canvas** | 居中内容栏约 `45rem`，与背景同色  | 扉页衬线标题 + 无衬线正文、两端对齐         |
| **AI 协作区**     | 对话消息流、证据与工具结果        | 15px 正文、角色分明消息壳、共用 prose token |
| **Accent**        | 链接、主按钮、AI 标识             | 中性蓝灰（**不用** violet 紫、赭铜）        |

Iris Rail Refresh 的 accent 演进为低饱和鼠尾草绿系，仅用于知识连接、AI/检索状态、Home/品牌轨道选中线、Outline Rail 当前章节和少量焦点态；灰阶仍承担编辑与壳层底色。

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

Iris Rail Refresh adds semantic surface tokens for the complete interface system: `--iris-rail-*` for brand rail and Rail Segments tabs, `--outline-rail-*` for the editor outline rail, `--ai-workspace-*` for the collaboration sidecar, and `--overlay-task-*` for task-capsule overlays. These tokens are semantic and should not be reused as generic decoration colors.

### 字体

| 场景                 | 栈 / Token   | 说明                                       |
| -------------------- | ------------ | ------------------------------------------ |
| **Chrome / UI**      | `font-sans`  | `Inter` + 系统无衬线（`--font-ui`）        |
| **编辑正文 / AI MD** | `font-prose` | `Noto Sans SC` + `Inter` + 系统中文字体    |
| **文档标题（扉页）** | `font-title` | `Noto Serif SC`（仅 `DocumentTitleField`） |
| **代码块**           | `font-mono`  | JetBrains Mono                             |

实现见 `src/styles/markdown-prose.css` 与 `index.html` Google Fonts 链接。

### 间距与栏宽

- 编辑区：`max-width: 45rem`，水平 `clamp(1.5rem, 5vw, 6rem)`，正文 `16px` / `line-height: 1.62`，段落**两端对齐**
- AI 侧栏：默认 `360px`，左缘拖拽调整（`280px`–`560px`，偏好写入 localStorage），可 `Ctrl+Shift+A` 收起
- AI 对话排版：`15px` / `line-height: 1.52`，`data-prose-surface="conversation"`，与编辑区共用 `markdown-prose.css` 与代码高亮 token
- AI 侧栏 chrome：**对话区仅消息流 + 证据包折叠条**；**Token 累计**、**工具/检索进行中**在全局底栏（`StatusBar` + `StatusBarTokenUsage`）；证据包标题行展示 **N 证据**、**M 搜索**（Token Plan 摘要）、**K 正文**（`fetch_web_page` 深读，`ContextPacketDrawer`）

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

| 元素         | 规则                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------- |
| **文档标题** | 独立字段 `DocumentTitleField`；**居中**、`Noto Serif SC`、`~2.25rem` bold；与正文间距 `--prose-title-gap`；Enter 进入正文 |
| **章节标题** | H1 `1.875rem` / H2 `1.5rem` / H3 `1.25rem`；左对齐；块间距 token 分级                                                     |
| **段落**     | 无段首缩进；**两端对齐**（`text-justify: inter-ideograph`）；块间距 `--prose-block-gap`；空行 spacer ≈ 55% 行高           |
| **共用排版** | `data-prose-surface="editor"` \| `conversation`；AI `--prose-size-chat`（15px），编辑 `--prose-size-editor`（16px）       |
| **AI 消息**  | 用户右对齐气泡；助手 `surface-elevated` 全宽壳；流式空态单行「正在思考…」，无 inset 左边条                                |
| **章节折叠** | H1–H3 左侧 `▸/▾`（仅正文区章节标题）                                                                                      |
| **Zen**      | `Ctrl+.` 隐藏 Tab/状态栏/AI，栏宽 `56rem`                                                                                 |
| **缩放**     | canvas `zoom` 75%–150%                                                                                                    |
| **悬浮目录** | `EditorOutline`，`Ctrl+Shift+O`                                                                                           |

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

桌面窗口：单行 **`DesktopTitleBar`**（`bg-surface-chrome`），禁止出现「Tauri App」或双层系统标题栏。顶栏高度统一为 44px，让品牌轨、Rail Segments 与右侧窗口控制在同一中线。

| 平台            | `--titlebar-height` | 装饰 / 标题                                                                              | 窗口按钮                                                         | 顶栏左侧                            |
| --------------- | ------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------- | ----------------------------------- |
| macOS           | **44px（2.75rem）** | `titleBarStyle: Overlay`、`hiddenTitle: true`、`decorations: false`；内部 title **Iris** | 右侧自绘红黄绿 `WindowControls`；`--titlebar-traffic-inset: 0px` | 常驻 `iris-brand-rail`，点击回 Home |
| Windows / Linux | **44px（2.75rem）** | `decorations: false`（Win 另 `shadow: true`）                                            | 右侧自绘红黄绿 `WindowControls`                                  | 常驻 `iris-brand-rail`，点击回 Home |

指标单一来源：Rust [`chrome_metrics.rs`](../src-tauri/src/chrome_metrics.rs)（统一 44）；前端镜像见 [`chrome-metrics.ts`](../src/lib/chrome-metrics.ts)。

- **Windows 11**：`transparent: false`（见 `tauri.windows.conf.json`），圆角由 DWM + `shadow` 提供；**勿**与 `transparent: true` 同开。
- **macOS**：`transparent: true` + `set_effects`（`radius` = `--window-radius`）+ `data-iris-platform-macos`；不恢复系统交通灯，左侧空间交给品牌轨。
- **macOS 全屏**：`useMacOSWindowChromeSync` 只同步标题栏高度与 fullscreen dataset；右侧自定义窗口控件随应用顶栏控制显隐。调试可用 `html[data-iris-window-fullscreen]`。

**人工验收**：macOS 窗口模式 — 左侧 Iris 品牌轨常驻且承担 Home 入口，不出现独立 Home tab，右侧红黄绿三点可用；全屏→退出后标题栏高度与品牌轨不漂移。Windows — 顶栏 44px、三点窗口控制与 Tab 无回归。

阴影：仅浮层 / 悬浮工具条使用 `--shadow-overlay` / `--shadow-floating`；**编辑区无纸页阴影**。

动效：150–200ms，`prefers-reduced-motion` 降级。

### Chrome 表面与命令/AI token

| Token                                                         | 用途                                                 |
| ------------------------------------------------------------- | ---------------------------------------------------- |
| `--surface-chrome`                                            | TabBar、StatusBar、侧栏壳                            |
| `--surface-elevated`                                          | 浮层、popover                                        |
| `--surface-inset`                                             | 输入底、列表 hover 底                                |
| `--command-highlight-bg` / `--command-highlight-ring`         | 命令列表选中（浅底 + inset ring，非大面积 primary）  |
| `--ai-user-bg` / `--ai-assistant-border` / `--ai-composer-bg` | AI 对话与输入区                                      |
| `--ai-stream-pulse`                                           | 流式等待指示                                         |
| `--knowledge-accent` / `--knowledge-accent-foreground`        | 鼠尾草绿知识连接色；品牌轨道选中线、Outline 当前章节 |
| `--iris-rail-*`                                               | 品牌轨道与 Rail Segments Tab 背景、激活、hover       |
| `--outline-rail-*`                                            | 边缘目录轨背景与当前章节 marker                      |
| `--ai-workspace-*`                                            | AI 协作侧车背景与边框                                |
| `--overlay-task-*`                                            | 任务舱 Overlay 标题区与列表选中态                    |

---

## Chrome 控件选型

| 场景        | 形态                                                                |
| ----------- | ------------------------------------------------------------------- |
| AI 场景     | `SceneSelector` 弹出（图标 + 描述）                                 |
| AI 发送     | `AiComposer` 多行；Enter 发送、Shift+Enter 换行                     |
| 证据包      | 可折叠 Section 标题 + badge                                         |
| 状态栏缩放  | Popover 滑块/步进（非三个并排按钮）                                 |
| 连通性      | 两枚 8px 圆点成组（LLM · 联网）；灰 / emerald / sky（`--status-*`） |
| 命令列表    | `CommandListOption` + `Kbd`；Lucide 图标                            |
| `/` 菜单    | `IrisSurfaceMenu`；仅文档级命令；有选区时提示用右键（非命令面板）   |
| 选区 AI     | **右键为主**；`editor-actions` 注册表；无自动浮动条                 |
| 右键菜单    | `iris_only`：`IrisContextMenu` + `IrisSurfaceMenu` 分组             |
| AI 消息选区 | 仅右键：复制、引用到输入；选区高亮限制在 `.ai-message-body` 内      |
| AI 输入框   | 右键仅剪贴板（复制/粘贴/全选），不含润色类动作                      |

主路径保留可见控件或快捷键；StatusBar 避免超过 3 个并排 icon-only 按钮。写作型 AI **不**进入 ⌘⇧P 命令面板。

---

## AI 组件

- **引用卡**：`border-border`，`rounded-lg`，细 primary 边
- **对话泡**：`AiMessageBubble` — 用户轻底、助手细边框；壳层 `overflow-hidden`；流式左边线 `--ai-stream-pulse`
- **研究结果卡**：时间线内 `ResearchResultMessage`；详情展开 `ResearchFocusView`
- **流式节点**：与 primary 同系，无紫色渐变

---

## C · 命令优先（备选原则）

| 原则        | 现状                                |
| ----------- | ----------------------------------- |
| 命令面板    | `Ctrl+Shift+P` 总览并执行功能       |
| 导航        | `Ctrl+P` Quick Open                 |
| 保存笔记    | `Ctrl+S`（层 1，写当前 `.md`）      |
| 版本快照    | `Ctrl+Shift+S`（层 1 + 手动检查点） |
| 次级功能    | 居中命令浮层                        |
| **AI 侧栏** | `Ctrl+Shift+A`                      |
| Zen         | `Ctrl+.`                            |

---

## 编辑器 · Callout 展示

Obsidian 式 `> [!note]` 在编辑区渲染为带左边框的 blockquote（accent 色与 `globals.css` 中 `.ProseMirror blockquote` 一致）。保存时写回 `> [!type]` 前缀，语义见 [markdown-export.md](./markdown-export.md)。

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
| **N**    | **v0.4.0-ui**      | Notion 扁平编辑、去行线、Inter、蓝灰 accent | **已发布**                |
| **N+**   | **v0.4.1-ui**      | Chrome 现代化：命令面板、AI、浮层原语       | **已发布**                |
| **2**    | **v1.0.0**（按需） | 标签栏自动隐藏、高对比主题                  | 待做                      |

---

## 参考

- **路线图**：[ROADMAP.md](../ROADMAP.md)
- **Notion 参考摘要**：[design-system/notion-master.md](./design-system/notion-master.md)
- **交互线框**：[ARCHITECTURE.md](../ARCHITECTURE.md)
