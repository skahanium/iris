# 可感知 UI 升级设计（v1.2.16 续）

## Purpose

在既有「冷灰壳 + 知识绿品牌点」方向上，交付**肉眼可辨**的视觉与行为升级，纠正上一轮「只铺 token、观感几乎不变」的问题。范围覆盖 Home 行为重构、Agent 对话面、正文节奏与壳层收敛；一次做满，实施按表面分段验收。

**状态（2026-07-25）：** Wave 1 Segment 1–4 **代码已合入** `branch-1.2.15`；人工清单亮/暗抽检仍待勾选。本文件已从「规划态」改为与实现对齐的事实说明。

## Locked decisions

| 决策 | 选择 |
|------|------|
| 感知重心 | 写作画布为主，壳层配套 sharpen |
| 肯定性主操作色 | 知识绿 `--brand`（空主面新建、Composer 发送等经 `Button variant="brand"`）；顶栏「+」为轻量 icon，非填充 brand；灰蓝 `--primary`/`--ring` 仅外链与通用 focus |
| Agent 空对话 | 轻修顶栏密度、发送钮、边框；不加建议问题 chips |
| 对话气泡 | 轻分层：助手几乎无底；用户极浅 brand tint；过程区脚注感 |
| Home | **已取消**独立欢迎工作台；冷启动自动打开最近/上次笔记 |
| 品牌轨 | 纯身份标识，取消点击与 Home/`--active` 态；保留拖窗命中 |
| 关到零 Tab | 主区轻空态 +「新建笔记」，**不**自动打开；可有「打开最近」弱链 |
| 交付切段 | 按表面 A：Home → Agent → 正文 → 壳层 |

过程折叠摘要产品文案以现有常量为准：`答复完毕`（`ANSWER_COMPLETE_PROCESS_LABEL`），不另造「回答完毕」分叉。

## Non-goals

- 不恢复纸墨/紫渐变；不换编辑器栈；不做 Notion 仪表盘欢迎页
- 不改 `--prose-measure: 52rem`；不取消编辑态 `justify`
- 居中浮层 enter/exit 禁止 `transform`/`scale`（仅 opacity）
- 不改 Outline ghost 的 `translateY(-50%)` 几何
- 过程区仍不展示工具参数、原始思维链、敏感载荷
- 不把 Agent 改成独立聊天 App 布局
- 不新造用户可见「导出 HTML」产品面；编辑与会话共用 `markdown-prose.css`

## Segment 1 — Home 行为与品牌轨（已实现）

### Behavior

| 场景 | 行为 |
|------|------|
| 冷启动 / 选库完成 | 有最近或上次会话笔记 → 自动打开；库空 → **VaultEmpty** |
| 关掉最后一个 Tab | **不**自动打开 → **WorkspaceEmpty** |
| 点击 Iris 品牌 | 无操作；静态标识；无 `--active`；可拖窗 |
| 打开失败 / 超时 | 落到 WorkspaceEmpty + 可读错误；`enterWorkspaceEmpty` / `setWorkspaceEmpty(true)` |

### Empty surfaces

实现：[`WorkspaceEmpty.tsx`](../../../src/components/layout/WorkspaceEmpty.tsx)（`WelcomeEmpty.tsx` **已删除**）。

1. **VaultEmpty**（`mode="vault"`）：「还没有笔记」+ 知识绿「新建第一篇」。
2. **WorkspaceEmpty**（`mode="workspace"`）：「未打开笔记」+ 知识绿「新建笔记」+「打开最近」。

状态机：`workspaceEmpty` / `enterWorkspaceEmpty`（原 `homeActive` / `showHome` 已重命名）。品牌轨见 [`DesktopTitleBar.tsx`](../../../src/components/layout/DesktopTitleBar.tsx)。冷启动解析：[`resolve-startup-note.ts`](../../../src/lib/resolve-startup-note.ts)。

## Segment 2 — Agent（已实现）

- 气泡：助手 `background: transparent` + 弱边；用户 `--ai-user-bg` brand tint。
- 发送：`AiComposer` `variant="brand"`。
- 过程：`ensureTerminalAnswerComplete` 接线 live transcript 与历史；折叠摘要末项为「答复完毕」。
- 空列表：一句引导，无 chips。

## Segment 3 — 正文节奏（已实现）

硬锁保持：`--prose-measure: 52rem`；`justify`；浮层仅 opacity。

已调：`--prose-h1`/`h2`、`--prose-heading-gap-before`、`--prose-block-gap`；亮色 `--editor-code-*`；callout 基类去掉 `bg-muted/25`。`example` callout 仍偏 muted → 记入 ROADMAP Wave 2。

## Segment 4 — 壳层（代码已合入；人工抽检待结案）

壳层分隔使用 Tailwind **`border-border-subtle`**（映射 CSS `--border-subtle`）。字号：`text-caption` / `text-micro`。Rail/Outline 对齐 `--brand`，不改 ghost 几何。

## Documentation

- [`ROADMAP.md`](../../../ROADMAP.md) v1.2.16：Wave 1 代码交付 / Wave 2 待办（含 example callout、动效、AbortController、人工抽检结案）。
- [`docs/design-system.md`](../../design-system.md)：品牌轨身份化、`variant="brand"`、空主面与气泡轻分层。
- 人工清单：[`iris-rail-refresh-manual-checklist.md`](../../testing/iris-rail-refresh-manual-checklist.md)。

## Known residual debt（不假装已清）

- `NoteOpenSource` 仍保留 legacy `"welcome"` 兼容分支；新路径用 `"workspace_empty"` / `"startup"`。
- Hook 名 `useHomeRecentNotes` / 模块前缀 `home-open-transition` 仍带 Home 字样（行为已是空主面 catalog）。
- Segment 4 亮/暗人工抽检清单未勾。
- Composer 外层可能仍有 `bg-ai-composer` 与 workbench `surface-elevated` 双层背景。

## Verification

| 段 | 必须证明 |
|----|----------|
| 1 | 无欢迎工作台；冷启动打开最近；零 Tab 不自动打开；品牌轨不可点；库空仅 VaultEmpty |
| 2 | 气泡轻分层可辨；发送为 brand；完成后折叠摘要为「答复完毕」；历史轮次同理 |
| 3 | measure/justify/overlay opacity 合同测试仍绿；light code/callout 对比提升有断言或清单 |
| 4 | chrome 边框/字号抽样一致；Rail/Outline brand 激活不破坏 ghost 几何；**人工亮暗抽检勾选后结案** |
