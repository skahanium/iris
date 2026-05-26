# Iris 设计系统

**方向定稿**：主攻 **B · 纸墨编辑**；备选 **C · 命令优先**（键盘与可收起面板，不抢编辑区）。

**排期**：阶段 0～3 与路线图版本的绑定见下文「落地阶段与路线图版本对照」；**版本 checklist 以 [ROADMAP.md](../ROADMAP.md) 为准**。

本文档是 **界面** 的单一参考；交互线框见 [ARCHITECTURE.md](../ARCHITECTURE.md)；全库文档索引见 [docs/README.md](./README.md)。

**v0.3.1-ui 施工计划**：[plans/2026-05-26-ui-overlay-refresh.md](./plans/2026-05-26-ui-overlay-refresh.md)

---

## B · 纸墨编辑（主方向）

### 气质

长文写作优先：编辑区像**纸**，应用外壳像**墨**（低对比 chrome）。AI 是校对台与侧栏助手，不是聊天 App 主屏。

### 分区

| 区域 | 角色 | 默认观感 |
|------|------|----------|
| **Chrome** | 标签栏、状态栏、AI 侧栏外壳、命令浮层边框 | 中性炭灰（冷灰低饱和），圆角柔和（14–20px） |
| **Editor canvas** | 墨底桌面 + **固定视口纸页** | 衬线正文、居中约 42rem；纸内滚动，纸边常显 |
| **Accent** | 链接、主按钮、AI 标识 | 克制赭铜（**不用** violet 紫） |

### 色彩 token（CSS 变量）

实现见 `src/styles/globals.css`。

| Token | 亮色 `.light` | 暗色 chrome（`:root`） | 用途 |
|-------|---------------|----------------------|------|
| `--background` 等 | 中性浅灰 | 冷灰炭墨 `hsl(240 6% 7%)` 系 | 外壳、侧栏、墨底 |
| `--editor-paper` | 近白冷纸 `hsl(40 4% 99%)` | **暗暖灰纸** `hsl(35 8% 16%)`（方案 A，护眼） | 居中纸页背景 |
| `--editor-ink` | 深墨 `hsl(240 9% 11%)` | 浅墨 `hsl(40 12% 88%)` | 正文 |
| `--editor-border` | 浅灰边 | 略亮于纸 1 阶的边 | 纸页描边 |
| `--primary` | 赭铜深 | 赭铜亮 | 主操作、AI 强调 |

**主题原则（v0.3.1-ui 修订）**

- **亮色**：壳浅、纸更亮，保持「纸浮于桌面」。
- **暗色**：壳深、**纸为暗暖灰（非亮白纸）**，字为浅灰墨；整体护眼，纸仍比壳略亮 1 阶以保留层次。
- 禁止暗色模式下仍使用高亮白纸（旧 `hsl(40 6% 90%)` 已废弃）。

### 字体

| 场景 | 栈 | 说明 |
|------|-----|------|
| **编辑器正文** | `font-editor` | `"Noto Serif SC"`, `"Source Han Serif SC"`, `Georgia`, serif |
| **UI / 侧栏** | `font-sans` | 系统无衬线（苹方 / 微软雅黑） |
| **代码块** | `font-mono` | JetBrains Mono 等等宽 |

### 间距与栏宽

- 编辑区：`max-width: 42rem`，边距 `clamp` 响应式，行高 `1.65`，字间距 `0.012em`
- AI 侧栏：固定 `280px`，可 `Ctrl+Shift+A` 收起（**唯一**常驻右侧 dock）

---

## 纸页视口（方案甲）

写作区采用 **「稿纸」模型**，非「内容条随字数变高」。

| 规则 | 说明 |
|------|------|
| **固定视口高度** | 纸页容器高度 = 视口减去标签栏、状态栏与上下留白（`100dvh` 基准） |
| **仅纸内滚动** | `overflow-y: auto` 在纸页容器上；墨底桌面不随段落滚动 |
| **纸边常显** | 四边圆角与阴影始终可见，短文档也不塌成窄条 |
| **空文档** | 纸仍满高；placeholder 在纸内偏上，不缩小纸容器 |

结构（实现目标）：

```
.iris-editor（墨底，不滚动或仅极浅背景）
  └── .iris-paper（定高 + overflow-y: auto + 圆角 + shadow）
        └── .ProseMirror（内容流）
```

---

## 命令浮层（Command Overlay）

快捷键唤起的次级 UI **统一为居中浮层**，禁止右侧贴边长条 `SidePanel` 形态。

### 行为契约

| 规则 | 说明 |
|------|------|
| **蒙层** | 全窗 scrim（约 `foreground/45–55`），可选轻 `backdrop-blur`；**盖住含 AI 在内的整窗** |
| **AI** | 打开浮层时 **不自动收起** AI；编辑区 **不被裁切**，仅 dim |
| **互斥** | **同时仅一个** 命令浮层；新开替换旧开 |
| **关闭** | `Esc`、点击 scrim、显式关闭按钮 |
| **焦点** | 浮层打开时焦点陷阱；关闭后焦点回到触发源或编辑区 |

### 尺寸变体（`IrisOverlay`）

| size | 用途 | 约略尺寸 |
|------|------|----------|
| `compact` | Quick Open | `max-w-xl`，高度随内容 |
| `command` | 搜索、文件、设置、反链、标签 | 宽 `80vw` `max-w-3xl`，高 `78vh` |
| `wide` | 版本时间线（双栏对比） | 宽 `92vw` `max-w-7xl`，高 `88vh` |
| `graph` | 知识图谱 | 宽 `96vw`，高 `92vh` |

浮层卡片：圆角 **`rounded-2xl`（16px）～ `rounded-3xl`（20px）`**，阴影 `--shadow-overlay`，边框 `border-border/60`。

### 快捷键对照（命令层）

| 快捷键 | 浮层 size |
|--------|-----------|
| `Ctrl+P` | `compact` |
| `Ctrl+Shift+E` / `F` / `B` / `T` | `command`（文件、搜索、反链、标签） |
| `Ctrl+,` | `command`（设置） |
| `Ctrl+Shift+V` | `wide` |
| `Ctrl+Shift+G` | `graph` |

---

## 圆角、阴影与动效

### 圆角尺度（柔和 SaaS）

| Token / 类 | 值 | 用于 |
|------------|-----|------|
| `--radius-sm` | 8px | 小标签、chip |
| `--radius-md` | 12px | 输入框、小按钮 |
| `--radius-lg` | 16px | 纸页、卡片、对话泡 |
| `--radius-xl` | 20px | 命令浮层外框、浮动工具条 |

避免大面积 `rounded-sm`（2–4px）作为默认；直角矩形仅用于 1px 分隔线。

### 动效

| 场景 | 时长 | 曲线 | 属性 |
|------|------|------|------|
| 浮层 scrim | 150ms | ease-out | opacity |
| 浮层内容 | 200ms enter / 140ms exit | ease-out | opacity + `scale(0.98→1)` |
| AI 侧栏收起 | 200ms | ease-out | width |
| 主题切换 | 200ms | ease | 纸面/壳 background-color |

**`prefers-reduced-motion: reduce`**：跳过 scale，opacity 瞬时或 ≤50ms。

### z-index 层叠

编辑 `0` → AI dock `10` → overlay scrim `40` → overlay content `50`。

---

## AI 组件（B）

- **引用卡**：与当前主题纸色一致 + 细赭石边；`rounded-lg`；来源 meta 一行
- **对话泡**：用户浅底/助手细边框；`rounded-2xl`；避免大块高饱和色
- **流式内联节点**（`ai-stream`）：与 accent 同系，勿用紫色渐变

---

## C · 命令优先（备选原则）

同一套 B token 下的布局策略：

| 原则 | 现状 / 规划 |
|------|-------------|
| 导航 | `Ctrl+P` Quick Open |
| 次级功能 | **居中命令浮层**（v0.3.1-ui），非右侧 Sheet |
| **AI 侧栏** | `Ctrl+Shift+A` 收起/展开 |
| 弱化常驻 chrome | Zen、标签栏自动隐藏 → v1.0 阶段 2 |

---

## 非目标（视觉）

- 紫色渐变、Inter/Space Grotesk 默认 AI 审美
- 聊天产品式全屏对话
- 第三方主题 / 插件换肤
- 快捷键面板以第二条右侧边栏形式叠在 AI 旁

---

## 落地阶段与路线图版本对照

| 设计阶段 | 路线图版本 | 内容 | 状态 |
|----------|------------|------|------|
| **0** | **v0.1.1** | 本文档初版、CSS token、纸面编辑区、赭石 accent、AI 侧栏收起 | 已完成 |
| **1** | **v0.2.0** | 引用卡、关联笔记芯片、`/` 菜单；图谱/标签/反链纸墨化 | 已完成 |
| **1.5** | **v0.3.1-ui** | 命令浮层、纸页视口、暗纸 A、圆角/动效、Chrome+AI 抛光 | 待做 |
| **2** | **v1.0.0**（按需） | Zen 模式、标签栏自动隐藏 | 待做 |
| **3** | **v1.0.0**（可选） | 克制流式动效（编辑器内） | 待做 |

v0.3.0 版本时间线等功能 UI 已在该版本交付；**交互形态升级**在 v0.3.1-ui 统一完成。

---

## 参考

- **路线图**：[ROADMAP.md](../ROADMAP.md)
- **UI 实现计划**：[plans/2026-05-26-ui-overlay-refresh.md](./plans/2026-05-26-ui-overlay-refresh.md)
- **交互线框**：[ARCHITECTURE.md](../ARCHITECTURE.md)
