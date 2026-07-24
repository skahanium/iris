# 可感知 UI 升级设计（v1.2.16 续）

## Purpose

在既有「冷灰壳 + 知识绿品牌点」方向上，交付**肉眼可辨**的视觉与行为升级，纠正上一轮「只铺 token、观感几乎不变」的问题。范围覆盖 Home 行为重构、Agent 对话面、正文节奏与壳层收敛；一次做满，实施按表面分段验收。

## Locked decisions

| 决策 | 选择 |
|------|------|
| 感知重心 | 写作画布为主，壳层配套 sharpen |
| 肯定性主操作色 | 知识绿 `--brand`（新建、发送等）；灰蓝 `--primary`/`--ring` 仅外链与通用 focus |
| Agent 空对话 | 轻修顶栏密度、发送钮、边框；不加建议问题 chips |
| 对话气泡 | 轻分层：助手几乎无底；用户极浅 brand tint；过程区脚注感 |
| Home | 取消独立欢迎工作台；冷启动自动打开最近/上次笔记 |
| 品牌轨 | 纯身份标识，取消点击与 Home/`--active` 态 |
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

## Segment 1 — Home 行为与品牌轨

### Behavior

| 场景 | 行为 |
|------|------|
| 冷启动 / 选库完成 | 有最近或上次会话笔记 → 自动打开；库空 → **VaultEmpty** |
| 关掉最后一个 Tab | **不**自动打开 → **WorkspaceEmpty** |
| 点击 Iris 品牌 | 无操作；不可作按钮；去掉 `isHomeActive` / `iris-brand-rail--active` |
| 打开失败 / 超时 | 落到 WorkspaceEmpty + 可读错误；不再 `setHomeActive(true)` 回欢迎工作台 |

### Empty surfaces（替代 WelcomeEmpty 工作台）

1. **VaultEmpty**：短句「还没有笔记」+ 知识绿「新建第一篇」。无四按钮矩阵。
2. **WorkspaceEmpty**：短句「未打开笔记」+ 知识绿「新建笔记」+ 可选弱链「打开最近」。无完整最近列表大页。

新建 / 快速打开 / 全库搜索 / AI 管理继续走顶栏与 Overlay，不在空态重复成矩阵。

### Implementation touchpoints

- 退役 [`WelcomeEmpty.tsx`](../../../src/components/layout/WelcomeEmpty.tsx) 工作台构图；改为上述空态。
- [`DesktopTitleBar.tsx`](../../../src/components/layout/DesktopTitleBar.tsx)：去掉 `onHome` / `isHomeActive` 点击态。
- [`useHomeWorkspaceTransitions.ts`](../../../src/hooks/useHomeWorkspaceTransitions.ts) / `showHome`：语义改为进入零 Tab 空态（或拆 `clearToWorkspaceEmpty`）。
- 更新 [`docs/design-system.md`](../../design-system.md)：品牌轨不再是「唯一 Home 入口」。
- 同步 [`docs/testing/iris-rail-refresh-manual-checklist.md`](../../testing/iris-rail-refresh-manual-checklist.md)。

Agent 侧栏在空主区时保持打开策略不变。

## Segment 2 — Agent 气泡、Composer 与过程文案

### Bubbles（轻分层）

- **助手**：几乎无底色；弱分隔；略增留白；无厚卡片、无气泡尾巴。
- **用户**：极浅 `--brand` tint，右对齐保留。
- **过程区**：脚注感——更小字、更弱上边线、无独立厚底；折叠行仍为 `处理过程 · {latest.label}`。
- **左侧操作轨**：保留确认/复制/刷新，对齐气泡，不抢内容。

### Composer / 顶栏（空态轻修）

- 发送钮：实心知识绿；禁用降透明。
- 输入框：`surface-elevated` + 规范边框，与消息区拉开一层。
- 顶栏四控件：统一 chrome 字号阶梯与间距，不改信息架构。
- 空列表：保留一句引导，不加 chips/插画。

### Process label truth

折叠摘要消费 `latest.label`。

1. 运行中可为「正在生成答复」。
2. Run `completed` 后过程列表须落到 **「答复完毕」**；折叠摘要必须是它，禁止完成后仍显示「正在生成答复」。
3. 历史已完成轮次同样显示「答复完毕」。
4. 以 [`tests/assistant-process.test.ts`](../../../tests/assistant-process.test.ts) 与气泡/transcript 消费路径回归测试锁住行为；修投影或 UI 消费回归点。

## Segment 3 — 正文节奏

### Hard locks

- `--prose-measure: 52rem` 与 canvas `max-width`
- 编辑态 `text-align: justify` + `inter-character`
- 浮层仅 opacity；Outline ghost `translateY`；文档标题聚焦 max-height 策略

### Changes

1. **标题阶梯**：略增 `--prose-h1`/`h2` 与 `--prose-heading-gap-before`；`--prose-block-gap` 微增（约 0.85–0.9em）。不换展示字体。
2. **亮色 code/callout**：抬高 light `--editor-code-bg`/`fg` 对比；callout 语义浅底 + 左边框更清楚，去掉万能 `bg-muted/25` 感。
3. **链接分工保持**：外链 primary 实线；wiki brand 虚线。
4. **导出 HTML** 与编辑面 prose token 对齐一轮。

## Segment 4 — 壳层收敛

1. 顶栏/底栏/AI 分隔/Overlay 顶栏统一 `--border-subtle`/`--border`/`--border-strong`。
2. StatusBar 与次要标注消费 `--text-caption`/`--text-ui`/`--text-body`，少用裸 `text-[11px]`。
3. Rail Tab 激活与 Outline marker 与 `--brand` 对齐（仅 tint，不改 ghost 几何）。
4. 控件禁止默认 `shadow-sm`/`shadow-md`；浮层才用 overlay/floating 阴影。

不做：加高标题栏、大毛玻璃、恢复品牌轨 Home、重做 Overlay 信息架构。

## Documentation updates

- [`ROADMAP.md`](../../../ROADMAP.md) v1.2.16：将本规格四段写入可验收事项；纠正与实现冲突的「取消两端对齐」若仍写在 Wave 1 则改为与硬锁一致（保留 justify）。
- [`docs/design-system.md`](../../design-system.md)：品牌轨身份化、主操作 brand、空态与气泡轻分层规则。
- 人工清单：Home 冷启动/零 Tab、品牌轨不可点、过程「答复完毕」、亮色 code/callout。

## Verification

| 段 | 必须证明 |
|----|----------|
| 1 | 无欢迎工作台；冷启动打开最近；零 Tab 不自动打开；品牌轨不可点；库空仅 VaultEmpty |
| 2 | 气泡轻分层可辨；发送为 brand；完成后折叠摘要为「答复完毕」；历史轮次同理 |
| 3 | measure/justify/overlay opacity 合同测试仍绿；light code/callout 对比提升有断言或清单 |
| 4 | chrome 边框/字号抽样一致；Rail/Outline brand 激活不破坏 ghost 几何 |

前端质量门：相关 vitest + `npm run lint` + `typecheck` + `format:check`（涉及面合并前按 AGENTS.md）。

## Delivery order

1. Segment 1 Home  
2. Segment 2 Agent  
3. Segment 3 Prose  
4. Segment 4 Chrome  

每段独立可截图验收后再进入下一段。
