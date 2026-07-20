# Iris 设计系统

> 本文定义当前 UI 的 token、组件边界和验收规则，不承担版本排期；排期见 [ROADMAP.md](../ROADMAP.md)。

## 方向与非目标

Iris 采用扁平、安静、面向长文写作的桌面界面：编辑区优先，命令与 AI 是辅助层。避免纸墨/信纸视觉、紫色渐变、聊天主屏化、第三方主题和插件换肤。

## Token 与实现位置

主题变量在 `src/styles/globals.css`；新增或调整 token 时，先更新本文档、ROADMAP 对应事项和样式源，再修改组件。

| Token 组                                   | 用途                                       |
| ------------------------------------------ | ------------------------------------------ |
| `--background`、`--foreground`、`--border` | 基础画布、文字与分隔                       |
| `--surface-*`                              | Chrome、浮层、输入区与内嵌表面             |
| `--command-highlight-*`                    | 命令列表焦点与选中态                       |
| `--ai-*`                                   | AI 消息、输入、流式状态与协作侧栏          |
| `--knowledge-accent`、`--outline-rail-*`   | 链接、目录、知识图谱与当前章节             |
| `--iris-rail-*`                            | 品牌轨、Tab rail 与激活/hover 状态         |
| `--shadow-overlay`、`--shadow-floating`    | 仅用于浮层和悬浮工具；编辑区不使用纸页阴影 |

动效通常为 150–200ms；`prefers-reduced-motion` 下必须降级。

## 组件边界

- `components/ui/`：shadcn/ui 基础原语与共享无业务组件。
- `components/editor/`：TipTap、编辑器命令、查找、媒体和 Markdown 往返体验。
- `components/ai/`：助手、证据包、工具确认、消息与写作提案。
- `components/layout/`：窗口 Chrome、Rail、标题栏、Overlay 和全局布局。

可复用控件应优先使用现有 `OverlayChrome`、`IrisSurfaceMenu`、`CommandListOption`、`Kbd`、`AiComposer`、`AiMessageBubble`、`SurfaceCard` 等原语，不能在业务组件重复实现。

## 交互规则

- 主路径必须有可见入口或快捷键；纯 icon 控件必须有可访问名称和 tooltip。
- `/` 菜单仅承载文档级命令；选区 AI 以右键和助手面板为主。
- AI 写入必须显示目标笔记、范围与风险并要求确认；不得展示或复制原始模型思维链。
- AI 证据卡显示来源、摘录与引用，不能伪造“链接即证据”。
- 普通域 `@` 文件/文件夹与 `#` 标签在输入框和用户消息中只以内联名称呈现，使用 `--ai-mention` 浅绿色前景色；不得显示 `@`、方括号、胶囊、“引用”行，也不得添加底色、边框、圆角或图标。真实相对路径与类型仅用于安全 tooltip。
- 标题栏、Rail 和 Tab 溢出应维持当前平台窗口行为；人工验收见 `docs/testing/`。

## Iris Rail 完整刷新设计

Iris Rail 由持久品牌轨、Rail Segments Tab、Outline Rail、AI Conversation Workspace 与 Overlay Family 组成。品牌轨是唯一 Home 入口；Rail Segments Tab 只承载已打开工作区对象；Outline Rail 负责当前文档结构；AI Conversation Workspace 保持写作上下文、证据和工具确认；Overlay Family 负责搜索、图谱、设置、版本和管理中心等临时任务。

TaskPlan 体验遵循 Markdown-first：助手对话先形成可读 Markdown 草稿；临时 tab 是高价值产物，用于承载结构化结果。过程 tab 只用于长任务进度，不替代最终笔记；引用胶囊显示短摘要、来源和可追溯证据，不展示原始敏感载荷。

## 验收

### 文档持久化与嵌入状态

状态栏和 Tab 只能消费文档持久化协调器的投影，不能根据 `activePath`、编辑器是否挂载或本地 Tab 缓存推断“已保存”。对有路径的文档，状态栏必须始终显示以下其中一个状态：`正在保存`、`已保存`、`保存失败`、`已保存但索引待修复`。其中 `正在保存` 覆盖尚未收到当前修订落盘回执的 dirty/saving 状态；只有对应修订的 Markdown 磁盘写入成功才可显示 `已保存`。

`保存失败` 是阻断性、可操作的错误：关闭、关闭标签、切换库和安装更新必须停留在当前界面，提供清楚的“重试 / 返回编辑”路径。编辑器重挂载期间，若协调器没有可信的完整 Markdown 快照，同样必须阻断这些操作；不得将空内容、`null` 或编辑器未就绪呈现为成功。索引待修复不是保存失败：Markdown 已安全落盘时，允许关闭和更新，状态用中性色或弱警示色说明派生索引正在修复。

管理中心只读取嵌入调度器的完整状态，而不本地拼接进度。`旧版检索可用，等待空闲升级`、`后台重建`、`已暂停`、`失败但不影响编辑` 和 `就绪` 必须与调度器 phase 一一对应。运行时可暂停；失败时仅显示安全失败原因和手动重试，不展示模型原始错误、笔记标题、正文或路径。后台重建不得遮挡编辑器、阻止 Markdown 保存或伪装成全局加载态。

这些状态文字应使用 `role="status"` 与 `aria-live="polite"`，并在亮/暗主题和窄窗口下保持可辨识；红色仅用于实际保存失败，不能用于已保存但索引待修复或可恢复的嵌入失败。

应用更新的缓存进度属于可恢复状态：发现部分缓存时以中性文案说明将继续下载，不展示发布时间；签名验证或预检失败才使用错误状态，且不得丢弃可安全续传的部分工件。

### AI 能力降级状态

`capability_degraded` 是对话内的轻量、非终态状态：使用中性色或弱警示色，显示能力名称、用户安全说明和可重试提示，不遮挡已生成内容，也不触发全局红色错误条。只有模型完全不可用、权限拒绝、持久化失败或非法请求等整轮无法回答的故障使用红色终态错误。降级状态必须可由键盘和读屏器感知，并与最终 `completed` 状态同时成立。

UI 改动至少验证亮/暗主题、键盘导航、窄窗口/Tab 溢出、`prefers-reduced-motion`、错误与加载态，并运行 lint、format、typecheck 与相关测试。涉及文档持久化或嵌入状态时，还必须验证上述保存/降级状态、关闭与更新阻断、暂停/继续和手动重试；涉及编辑器 schema 时，还必须运行 Markdown parse → node tree → serialize 往返测试。
