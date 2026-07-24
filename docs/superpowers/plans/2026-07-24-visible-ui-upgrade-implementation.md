# 可感知 UI 升级 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 Home → Agent → 正文 → 壳层四段交付肉眼可辨的 UI 升级，并在同一次改造中消除 Home 双语义、欢迎页死代码与过程文案回归等技术债。

**Architecture:** 先落地单一 `variant="brand"` 按钮与清晰的「无文档主面」状态机（废除 `WelcomeEmpty` 工作台与品牌轨 Home），再修 Agent 过程投影真相与气泡/Composer 视觉，然后抬升 prose token（不碰 measure/justify/浮层 transform），最后统一 chrome 边框与字号消费。每段以失败测试先行，删净旧路径，禁止遗留兼容壳。

**Tech Stack:** React 19、TipTap、Tailwind + CVA `Button`、现有 `assistant-presentation` / `assistant-process`、Vitest、设计 token 于 `globals.css` / `markdown-prose.css`。

**Spec:** [docs/superpowers/specs/2026-07-24-visible-ui-upgrade-design.md](../specs/2026-07-24-visible-ui-upgrade-design.md)

## Global Constraints

- 肯定性主操作色：`--brand`（新建、发送）；`--primary`/`--ring` 仅外链与通用 focus。
- 过程完成文案常量：`答复完毕`（`ANSWER_COMPLETE_PROCESS_LABEL`），禁止另造「回答完毕」。
- 硬锁：`--prose-measure: 52rem`；编辑态 `justify`；居中浮层 enter/exit 仅 opacity；Outline ghost `translateY(-50%)` 几何不动。
- 不做 Notion 仪表盘、建议问题 chips、气泡尾巴、纸墨/紫渐变。
- **减债硬规则：** 禁止保留 `WelcomeEmpty` 工作台「仅隐藏」；禁止 `homeActive` 与「空主面」双语义并存超过本计划 Task 3；禁止用 UI 字符串替换掩饰过程投影 bug；禁止新增一次性 `className="bg-[hsl(var(--brand))]"` 散落，统一走 `Button variant="brand"`。
- 分支：`branch-1.2.15`；中文 Conventional Commits；每 Task 结束跑相关 vitest，Segment 结束加 `npm run lint` + `typecheck` + `format:check`（触及面）。
- 无用户可见 HTML 导出产品面：Segment 3 **不**新造导出管线；「导出对齐」落实为 editor + conversation 共用 `markdown-prose.css` token，并更新 ROADMAP 措辞避免空头承诺。

## File map（锁定拆分）

| 单元          | 路径                                                                  | 职责                                           |
| ------------- | --------------------------------------------------------------------- | ---------------------------------------------- |
| Brand CTA     | `src/components/ui/button.tsx`                                        | 唯一 `brand` variant                           |
| 空主面 UI     | `src/components/layout/WorkspaceEmpty.tsx`（新建；取代 WelcomeEmpty） | VaultEmpty / WorkspaceEmpty                    |
| 状态机        | `useHomeWorkspaceTransitions.ts` 等                                   | `showHome`→`enterWorkspaceEmpty`；布尔语义澄清 |
| 冷启动打开    | `usePreparedWorkspaceTransitions.ts` + 小纯函数                       | snapshot/recent 自动打开                       |
| 过程真相      | `useAssistantRunTranscript.ts` + presentation/process                 | terminal 必含「答复完毕」                      |
| 气泡/Composer | `globals.css` + `ai-composer.tsx` + `AiMessageBubble.tsx`             | 轻分层与发送绿                                 |
| 正文          | `markdown-prose.css` + light `--editor-code-*`                        | 节奏与对比                                     |
| 壳层          | StatusBar / TitleBar / Overlay 边框字号                               | token 收敛                                     |
| 文档          | design-system、ROADMAP、rail checklist、E2E 契约                      | 与实现同步                                     |

---

## Segment 1 — Home

### Task 1: `Button` 增加 `brand` variant（全计划 CTA 地基）

**Files:**

- Modify: `src/components/ui/button.tsx`
- Test: `tests/button-brand-variant.test.ts`（新建）

**Interfaces:**

- Produces: `buttonVariants` 增加 `brand`，绑定 `--brand` / `--brand-foreground`
- Consumes: 无

- [ ] **Step 1: 写失败测试**

```ts
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const buttonSrc = readFileSync(
  resolve(process.cwd(), "src/components/ui/button.tsx"),
  "utf8",
);

describe("Button brand variant", () => {
  it("exposes brand variant bound to --brand tokens", () => {
    expect(buttonSrc).toMatch(/brand:\s*"/);
    expect(buttonSrc).toContain("--brand");
    expect(buttonSrc).toContain("--brand-foreground");
  });
});
```

- [ ] **Step 2: 跑测确认失败**

Run: `npx vitest run tests/button-brand-variant.test.ts`

Expected: FAIL（无 `brand:`）

- [ ] **Step 3: 实现**

在 `button.tsx` 的 `variant` 中增加：

```ts
brand:
  "bg-[hsl(var(--brand))] text-[hsl(var(--brand-foreground))] hover:brightness-110",
```

保留 `default` 为 primary（外链/通用 chrome），**不要**把 default 改成 brand。

- [ ] **Step 4: 跑测通过并提交**

```bash
npx vitest run tests/button-brand-variant.test.ts
git add src/components/ui/button.tsx tests/button-brand-variant.test.ts
git commit -m "$(cat <<'EOF'
feat(ui): 新增 Button brand 变体作为肯定性主操作色

EOF
)"
```

---

### Task 2: `WorkspaceEmpty` 取代欢迎工作台 UI

**Files:**

- Create: `src/components/layout/WorkspaceEmpty.tsx`
- Create: `tests/workspace-empty.test.tsx`
- Delete later (Task 5): `src/components/layout/WelcomeEmpty.tsx`

**Interfaces:**

- Produces:

```ts
export type WorkspaceEmptyMode = "vault" | "workspace";

export interface WorkspaceEmptyProps {
  mode: WorkspaceEmptyMode;
  onNew: () => void | Promise<void>;
  onOpenRecent?: () => void | Promise<void>;
  errorMessage?: string | null;
}
```

- Vault：短句「还没有笔记」；主按钮「新建第一篇」`variant="brand"`
- Workspace：短句「未打开笔记」；主按钮「新建笔记」`variant="brand"`；若有 `onOpenRecent` 则弱链「打开最近」（`ghost`/`outline`，非 brand）
- `data-testid="workspace-empty"`；`data-mode={mode}`；主按钮 `data-testid="workspace-empty-new"`；最近链 `data-testid="workspace-empty-open-recent"`
- **禁止**四按钮矩阵、完整最近列表、`home-workbench` / `home-workbench-grid` / `home-recent-note`

- [ ] **Step 1: 写失败组件测**

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { WorkspaceEmpty } from "@/components/layout/WorkspaceEmpty";

it("vault mode shows create-first brand CTA only", async () => {
  const onNew = vi.fn();
  render(<WorkspaceEmpty mode="vault" onNew={onNew} />);
  expect(screen.getByTestId("workspace-empty")).toHaveAttribute(
    "data-mode",
    "vault",
  );
  expect(screen.queryByTestId("workspace-empty-open-recent")).toBeNull();
  await userEvent.click(screen.getByTestId("workspace-empty-new"));
  expect(onNew).toHaveBeenCalled();
});

it("workspace mode can open recent via weak link", async () => {
  const onOpenRecent = vi.fn();
  render(
    <WorkspaceEmpty
      mode="workspace"
      onNew={vi.fn()}
      onOpenRecent={onOpenRecent}
    />,
  );
  await userEvent.click(screen.getByTestId("workspace-empty-open-recent"));
  expect(onOpenRecent).toHaveBeenCalled();
});
```

- [ ] **Step 2: RED → 实现最小 JSX → GREEN**

布局：`flex flex-1 items-center justify-center`；内层 `max-w-sm`；说明 `text-muted-foreground text-sm`；错误时 `role="status"` 显示 `errorMessage`。

- [ ] **Step 3: 提交**

```bash
git add src/components/layout/WorkspaceEmpty.tsx tests/workspace-empty.test.tsx
git commit -m "$(cat <<'EOF'
feat(ui): 新增 Vault/Workspace 极简空主面组件

EOF
)"
```

---

### Task 3: 状态机语义去债（废除品牌轨 Home）

**目标：** 空主面布尔只表示「主区无已提交文档/媒体主面」。品牌轨不可点。`showHome` 重命名为 `enterWorkspaceEmpty`。本 Task **一次性**将 `homeActive`/`setHomeActive` 重命名为 `workspaceEmpty`/`setWorkspaceEmpty`（约 11–14 文件），禁止半改留下双名。

**Files:**

- Modify: `src/hooks/useHomeWorkspaceTransitions.ts`
- Modify: `src/hooks/usePreparedWorkspaceTransitions.ts`
- Modify: `src/hooks/useWorkspaceTabRouting.ts`
- Modify: `src/App.impl.tsx`
- Modify: `src/components/layout/DesktopTitleBar.tsx`
- Modify: `src/App.tsx`（注释）
- Modify tests: `tests/use-home-workspace-transitions.test.tsx`, `tests/use-workspace-tab-routing.test.tsx`, `tests/use-prepared-workspace-transitions.test.tsx`, `tests/desktop-title-bar.test.ts`, `tests/iris-rail-refresh-contract.test.ts`, `tests/app-editor-workspace-pending-open.test.tsx`, `tests/document-open-first-frame.test.tsx`
- Docs: `docs/design-system.md`, `docs/testing/iris-rail-refresh-manual-checklist.md`

**Interfaces:**

- Produces: `enterWorkspaceEmpty(closedPath?: string | null): void`（清 watchdog、取消 pending open、`setWorkspaceEmpty(true)`）
- DesktopTitleBar **删除** `onHome` / `isHomeActive`；品牌区静态：

```tsx
<div
  data-testid="iris-brand-rail"
  className="iris-brand-rail pointer-events-none flex h-8 min-w-[6.75rem] shrink-0 select-none items-center justify-center gap-2 px-3 text-foreground"
>
  {/* logo + Iris；无 button role；无 onClick；无 --active */}
</div>
```

- 关最后 note Tab 且无 media：`enterWorkspaceEmpty`（不自动打开）
- 打开超时/失败：`enterWorkspaceEmpty` + pending 错误，由空主面展示（Task 5）

- [ ] **Step 1: 更新契约测**

品牌轨源码不得含 `onClick={onHome}` / `isHomeActive` / `iris-brand-rail--active`；不得再要求 `home-workbench-grid`。

- [ ] **Step 2: RED → 改 TitleBar + 重命名 `showHome`/`homeActive` → 更新 hook 测试**

- [ ] **Step 3: App.impl 去掉 `onHome` / `isHomeActive` 传入**

- [ ] **Step 4: 跑相关测**

```bash
npx vitest run tests/use-home-workspace-transitions.test.tsx \
  tests/use-workspace-tab-routing.test.tsx \
  tests/desktop-title-bar.test.ts \
  tests/iris-rail-refresh-contract.test.ts
```

- [ ] **Step 5: 提交**

```bash
git commit -m "$(cat <<'EOF'
refactor(ui): 品牌轨去 Home，空主面状态机语义澄清

EOF
)"
```

---

### Task 4: 冷启动自动打开（snapshot → recent → vault empty）

**Files:**

- Create: `src/lib/resolve-startup-note.ts`
- Create: `tests/resolve-startup-note.test.ts`
- Modify: `src/hooks/usePreparedWorkspaceTransitions.ts`
- Modify: `tests/use-prepared-workspace-transitions.test.tsx`

**Interfaces:**

- Produces:

```ts
export interface StartupNoteCandidate {
  path: string;
  titleHint?: string;
}

/** Prefer snapshot.activePath if in openNotePaths or recentPaths; else first recentPaths entry. */
export function resolveStartupNote(input: {
  activePath: string | null;
  openNotePaths: readonly string[];
  recentPaths: readonly string[];
}): StartupNoteCandidate | null;
```

规则：

1. `activePath` 非空且（在 `openNotePaths` 或 `recentPaths`）→ 选它
2. 否则 `recentPaths[0]`
3. 否则 `null`（库空 → VaultEmpty）

冷启动 effect（vault 就绪后）：

1. 保留现有 `warmNotePath` warmup
2. 若已有 tabs / 非 empty → 不抢开
3. `resolveStartupNote` 有结果 → 走既有 `openNote` 离开空态路径（`source: "startup"`）
4. **不要**在关零 Tab 路径调用此逻辑

Recent paths：启动时一次 `fileList()`（按 `updated_at`）；不要为欢迎页常驻订阅。Task 5 删除 `useHomeRecentNotes` 对欢迎页的耦合后，启动查询可内聚在 prepared transitions。

- [ ] **Step 1: 纯函数单测（无 recent、仅 snapshot、snapshot 失效回落）**
- [ ] **Step 2: hook 测：mock snapshot + fileList → openNote 一次；关 Tab 空态不二次 auto-open**
- [ ] **Step 3: 实现 → GREEN → 提交**

```bash
git commit -m "$(cat <<'EOF'
feat(ui): 冷启动按会话快照或最近笔记自动打开

EOF
)"
```

---

### Task 5: 接线空主面、删除 WelcomeEmpty、修 E2E 契约

**Files:**

- Modify: `src/components/layout/AppEditorWorkspace.tsx`
- Delete: `src/components/layout/WelcomeEmpty.tsx`
- Rewrite: `tests/welcome-empty-recent.test.tsx` → 删除或改为空主面接线测
- Modify: `tests/app-editor-workspace-pending-open.test.tsx`, `tests/document-open-first-frame.test.tsx`
- Modify: `tests/windows-desktop-persistence-contract.test.ts`, `scripts/run-windows-persistence-e2e.mjs`
- Modify: `tests/app-shell-refactor-contract.test.ts`, `tests/iris-rail-refresh-contract.test.ts`
- Modify: `ROADMAP.md`（Segment 1 验收；去欢迎工作台表述）

**Wiring:**

- 无 media 且无 editor surface 且无 pending loading → `<WorkspaceEmpty mode={vaultHasNotes ? "workspace" : "vault"} ... />`
- `vaultHasNotes`：`fileList().length > 0`（或等价 catalog），禁止用「是否有 recent 缓存」猜测
- `onOpenRecent`：仅 workspace；打开与 `resolveStartupNote` 同序的第一篇
- `errorMessage`：`pendingOpen?.error`
- 全仓 `rg`：`WelcomeEmpty|home-workbench|home-workbench-grid|home-recent-note|onHome|isHomeActive|iris-brand-rail--active` 必须清空

Windows E2E：冷启动改为等待编辑器或 `workspace-empty`；不再点击 `home-recent-note`。若脚本依赖「从欢迎点最近」，改为断言自动打开后的编辑器标题/路径，或在零 Tab 空态点「打开最近」。

- [ ] **Step 1: 契约测切到 `workspace-empty` → RED**
- [ ] **Step 2: 接线 + 删除 WelcomeEmpty → GREEN**
- [ ] **Step 3: Segment 1 质量门**

```bash
npx vitest run tests/workspace-empty.test.tsx tests/resolve-startup-note.test.ts \
  tests/use-home-workspace-transitions.test.tsx tests/use-workspace-tab-routing.test.tsx \
  tests/use-prepared-workspace-transitions.test.tsx tests/iris-rail-refresh-contract.test.ts \
  tests/app-editor-workspace-pending-open.test.tsx tests/windows-desktop-persistence-contract.test.ts
npm run lint && npm run typecheck && npm run format:check
```

- [ ] **Step 4: 提交**

```bash
git commit -m "$(cat <<'EOF'
feat(ui): 退役欢迎工作台并接线极简空主面

EOF
)"
```

**Segment 1 人工门禁：** 冷启动有笔记自动打开；关光 Tab 见 WorkspaceEmpty 且不自动开；库空见 VaultEmpty；Iris 标识不可点。

---

## Segment 2 — Agent

### Task 6: 过程折叠摘要「答复完毕」真相（数据层修债）

**根因：** 折叠 UI 读 `events.at(-1).label`；`useAssistantRunTranscript` 在 `presentationOwnsMessage` 时冻结 `current.processItems`。若缺 `answer_complete` / durable `completed`，末项停在「正在生成答复」。禁止 UI 字符串 hack。

**Files:**

- Create: `src/lib/ensure-answer-complete-process.ts`
- Create: `tests/ensure-answer-complete-process.test.ts`
- Modify: `src/components/ai/hooks/useAssistantRunTranscript.ts`
- Modify: `src/components/ai/SessionHistoryDropdown.tsx`（`toChatLines`）
- Modify: `tests/use-assistant-run-transcript.test.tsx`
- Create or modify: `tests/assistant-process-summary.test.tsx`（折叠摘要可见「答复完毕」）

**Interfaces:**

- Produces: `ensureTerminalAnswerComplete(items, runState)` — 仅当 `runState === "completed"` 且列表尚无 `ANSWER_COMPLETE_PROCESS_ID` / `ANSWER_COMPLETE_PROCESS_LABEL` 时追加完成项；`failed`/`cancelled` 不追加「答复完毕」

```ts
import {
  ANSWER_COMPLETE_PROCESS_ID,
  ANSWER_COMPLETE_PROCESS_LABEL,
} from "@/lib/assistant-presentation";

export function ensureTerminalAnswerComplete(
  items:
    | readonly { id: string; label: string; kind: string; status: string }[]
    | undefined,
  runState: string | null | undefined,
): typeof items extends undefined ? [] : NonNullable<typeof items> {
  const list = items ? [...items] : [];
  if (runState !== "completed") return list as never;
  if (
    list.some(
      (i) =>
        i.id === ANSWER_COMPLETE_PROCESS_ID ||
        i.label === ANSWER_COMPLETE_PROCESS_LABEL,
    )
  ) {
    return list as never;
  }
  list.push({
    id: ANSWER_COMPLETE_PROCESS_ID,
    kind: "stage",
    label: ANSWER_COMPLETE_PROCESS_LABEL,
    status: "completed",
  });
  return list as never;
}
```

（实现时改用仓库内真实 `AssistantProcessItem` 类型并补齐必填字段。）

- Transcript：

```ts
const rawItems = presentationOwnsMessage
  ? current?.processItems
  : projectAssistantProcessEvents(run.events, run.reasoningSummaries);
const processItems = ensureTerminalAnswerComplete(rawItems, run.state);
```

- 历史：`projectAssistantProcessEvents(...)` 之后 `ensureTerminalAnswerComplete(..., "completed")`

- [ ] **Step 1: 纯函数测 + transcript completed 末项测 → RED**
- [ ] **Step 2: 实现 helper 与接线 → GREEN**
- [ ] **Step 3: UI 测折叠摘要「答复完毕」**
- [ ] **Step 4: 提交**

```bash
git commit -m "$(cat <<'EOF'
fix(ai): 答复完成后过程摘要固定为答复完毕

EOF
)"
```

---

### Task 7: 气泡轻分层 + Composer 发送绿 + Header 密度

**Files:**

- Modify: `src/styles/globals.css`
- Modify: `src/components/ai/AiMessageBubble.tsx`（过程区脚注感 class）
- Modify: `src/components/ui/ai-composer.tsx`（发送 `variant="brand"` `size="icon"`）
- Modify: `src/components/ai/AssistantPanelHeader.tsx`
- Modify: `tests/iris-rail-refresh-contract.test.ts`
- Modify: `tests/design-tokens.test.ts`（若改 `--ai-user-bg`）

**视觉锁定：**

- 助手：近透明底；弱边或仅分隔；可 `rounded-lg`
- 用户：`--ai-user-bg` 改为极浅 brand tint（例 light `150 12% 94%`，dark `150 10% 16%`）
- 过程区：去厚底；caption 字号；弱上边线
- Composer：`surface-elevated` + 规范边框；发送 `variant="brand"`
- Header：不改信息架构；压缩 gap / 统一 caption

- [ ] **Step 1: 契约测（composer brand、user bubble brand 色相）→ RED**
- [ ] **Step 2: 改 CSS/组件 → GREEN**
- [ ] **Step 3: Segment 2 质量门 + 提交**

```bash
npx vitest run tests/ensure-answer-complete-process.test.ts \
  tests/use-assistant-run-transcript.test.tsx \
  tests/assistant-process.test.ts tests/assistant-presentation.test.ts \
  tests/iris-rail-refresh-contract.test.ts tests/design-tokens.test.ts
npm run lint && npm run typecheck && npm run format:check
git commit -m "$(cat <<'EOF'
feat(ai): 对话气泡轻分层与发送钮品牌色

EOF
)"
```

**Segment 2 人工门禁：** 完成后折叠为「答复完毕」；发送钮绿；助手非厚灰卡片。

---

## Segment 3 — 正文

### Task 8: 标题阶梯、块距、亮色 code/callout

**Files:**

- Modify: `src/styles/markdown-prose.css`
- Modify: `src/styles/globals.css`（light `--editor-code-*`）
- Modify: `tests/prose-tokens.test.ts`
- Modify: 既有 callout/prose 合同测（若断言旧 gap）
- Modify: `ROADMAP.md`：删除「编辑态取消两端对齐」；保留 justify；「导出 HTML」改为「编辑与会话共用 prose token」

**数值锁定：**

```css
--prose-block-gap: 0.875em;
--prose-heading-gap-before: 1.85em;
--prose-h1: 1.75rem;
--prose-h2: 1.4375rem;
```

Light code：

```css
--editor-code-bg: 210 10% 92%;
--editor-code-fg: 210 12% 18%;
```

Callout：去掉通用 `bg-muted/25`，保留语义浅底 + 左边框。

硬锁断言保留：`--prose-measure: 52rem`；`text-align: justify`。

- [ ] **Step 1: 更新 prose-tokens 期望 → RED**
- [ ] **Step 2: 改 CSS → GREEN**

```bash
npx vitest run tests/prose-tokens.test.ts tests/markdown-prose-contract.test.ts \
  tests/overlay-enter-animation-safety.test.ts
```

- [ ] **Step 3: 提交**

```bash
git commit -m "$(cat <<'EOF'
feat(editor): 抬升正文标题节奏与亮色代码块对比

EOF
)"
```

---

## Segment 4 — 壳层

### Task 9: 边框/字号收敛 + 文档收官

**Files:**

- Modify: `src/components/layout/DesktopTitleBar.tsx`
- Modify: StatusBar 组件（仓库内 StatusBar 实现文件）
- Modify: Overlay 顶栏（`IrisOverlay` / task overlay header）
- Modify: AI sidecar 外层分隔
- Modify: `tests/iris-rail-refresh-contract.test.ts`
- Modify: `docs/design-system.md`
- Modify: `ROADMAP.md` v1.2.16 四段验收对齐
- Modify: `docs/testing/iris-rail-refresh-manual-checklist.md`

**减债：** 只替换本 Task 触及文件内的 `border-border/60`、`text-[11px]` 为语义 token；禁止全仓无脑替换。Rail/Outline 仅确认 `--brand`，不改 ghost 几何。

- [ ] **Step 1: 契约/清单更新 → 改壳层 → 跑测**
- [ ] **Step 2: 质量门**

```bash
npm run lint && npm run typecheck && npm run format:check
npx vitest run tests/button-brand-variant.test.ts tests/workspace-empty.test.tsx \
  tests/resolve-startup-note.test.ts tests/ensure-answer-complete-process.test.ts \
  tests/prose-tokens.test.ts tests/iris-rail-refresh-contract.test.ts \
  tests/design-tokens.test.ts tests/overlay-enter-animation-safety.test.ts
```

- [ ] **Step 3: 提交**

```bash
git commit -m "$(cat <<'EOF'
refactor(ui): 壳层边框与字号收敛并同步设计文档

EOF
)"
```

**Segment 4 人工门禁：** 顶/底/AI/Overlay 边框一系；亮暗各扫一眼。

---

## Debt checklist（合并前自检）

- [ ] `rg WelcomeEmpty|home-workbench|home-recent-note|onHome|isHomeActive|iris-brand-rail--active` 为空
- [ ] `rg showHome` 为空（或仅历史 changelog）
- [ ] 新建/发送均 `variant="brand"`，无散落主操作 `bg-primary`
- [ ] 过程完成走 `ensureTerminalAnswerComplete`，无 UI 文案 hack
- [ ] ROADMAP 无「取消两端对齐」；无未实现的「导出 HTML 产品」承诺
- [ ] Windows persistence E2E 不依赖欢迎最近列表

## Spec coverage

| Spec 段                              | Task            |
| ------------------------------------ | --------------- |
| Segment 1 Home / 品牌轨 / 空态       | 2–5             |
| Segment 2 气泡 / Composer / 答复完毕 | 6–7             |
| Segment 3 正文                       | 8               |
| Segment 4 壳层 + 文档                | 3（部分）、5、9 |
| Brand CTA 地基                       | 1、2、5、7      |
