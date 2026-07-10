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
- 标题栏、Rail 和 Tab 溢出应维持当前平台窗口行为；人工验收见 `docs/testing/`。

## 验收

UI 改动至少验证亮/暗主题、键盘导航、窄窗口/Tab 溢出、`prefers-reduced-motion`、错误与加载态，并运行 lint、format、typecheck 与相关测试。涉及编辑器 schema 时，还必须运行 Markdown parse → node tree → serialize 往返测试。
