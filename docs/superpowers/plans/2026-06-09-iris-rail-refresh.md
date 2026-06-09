# Iris Rail Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the complete Iris Rail interface system: persistent brand rail, Rail Segments tabs, Home workspace, Outline Rail, quiet editor edge controls, AI collaboration sidecar, task-capsule overlays, and AI System Center.

**Architecture:** The refresh is frontend-only and token-driven. Add shared Iris Rail tokens and small reusable shells first, then migrate each surface to those contracts while preserving existing IPC, TipTap schema, Markdown storage, and Tauri backend behavior.

**Tech Stack:** React 19, TypeScript, TailwindCSS + shadcn/ui primitives, Tauri 2 window chrome, TipTap/ProseMirror, Vitest contract tests.

---

## File Structure

| Path                                                            | Responsibility                                                                                           |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `docs/superpowers/specs/2026-06-09-iris-rail-refresh-design.md` | Source design spec; read before each task.                                                               |
| `docs/design-system.md`                                         | Public UI system reference; add final Iris Rail token and surface rules.                                 |
| `ROADMAP.md`                                                    | High-level experience target; already points to Iris Rail complete system.                               |
| `src/styles/globals.css`                                        | CSS variables and component-level classes for Rail, tabs, outline, editor edge controls, AI, overlays.   |
| `tailwind.config.js`                                            | Tailwind token mappings for new CSS variables.                                                           |
| `src/components/layout/DesktopTitleBar.tsx`                     | Persistent brand rail, Home entry, Rail Segments tabs, right-side window controls.                       |
| `src/components/layout/WelcomeEmpty.tsx`                        | Home workspace surface and actions.                                                                      |
| `src/App.tsx`                                                   | Home view state, AI System Center wiring, overlay rendering.                                             |
| `src/hooks/useOverlayManager.ts`                                | Add `aiSystemCenter` overlay id and helpers.                                                             |
| `src/lib/command-palette.ts`                                    | Add AI System Center command entry without global shortcut.                                              |
| `src/components/settings/AiSystemCenterPanel.tsx`               | New AI System Center overlay shell and sections.                                                         |
| `src/components/settings/SettingsPanel.tsx`                     | Reduce to general settings: appearance, about, basic preferences.                                        |
| `src/components/editor/EditorOutline.tsx`                       | Convert current floating outline card to Outline Rail.                                                   |
| `src/components/editor/TipTapEditor.tsx`                        | Edge control classes and editor workspace test hooks.                                                    |
| `src/components/editor/EditorFindReplaceBar.tsx`                | Align with editor edge-control language.                                                                 |
| `src/components/ai/UnifiedAssistantPanel.tsx`                   | AI collaboration sidecar chrome and task surfaces.                                                       |
| `src/components/ai/AiMessageBubble.tsx`                         | Message surface classes for user/assistant/streaming/selected states.                                    |
| `src/components/ui/ai-composer.tsx`                             | Composer as fixed AI workbench surface.                                                                  |
| `src/components/ui/overlay-chrome.tsx`                          | Shared task-capsule header/footer shell.                                                                 |
| `src/components/ui/iris-overlay.tsx`                            | Shared overlay panel chrome and task-capsule class hooks.                                                |
| `src/lib/overlay-sizes.ts`                                      | Preserve existing size taxonomy while adding shared shell classes.                                       |
| `tests/iris-rail-refresh-contract.test.ts`                      | New source-level contract tests for the complete UI system.                                              |
| Existing focused tests                                          | Update existing tests that currently expect AI settings inside `SettingsPanel` or old titlebar behavior. |

Keep `src-tauri/src/embedding/engine.rs` out of all stages and commits for this work.

---

## Task 1: Iris Rail Tokens And Design-System Contracts

**Files:**

- Modify: `src/styles/globals.css`
- Modify: `tailwind.config.js`
- Modify: `docs/design-system.md`
- Test: `tests/design-tokens.test.ts`
- Test: `tests/iris-rail-refresh-contract.test.ts`

- [ ] **Step 1: Write failing token contract tests**

Add this test file:

```ts
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Iris Rail complete interface contracts", () => {
  it("defines semantic tokens for the full Iris Rail interface system", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--knowledge-accent");
    expect(css).toContain("--iris-rail-bg");
    expect(css).toContain("--iris-rail-active");
    expect(css).toContain("--outline-rail-bg");
    expect(css).toContain("--outline-rail-active");
    expect(css).toContain("--ai-workspace-bg");
    expect(css).toContain("--ai-workspace-border");
    expect(css).toContain("--overlay-task-header");
  });

  it("documents the complete Iris Rail target surfaces", () => {
    const design = read("docs/design-system.md");
    expect(design).toContain("Iris Rail 完整刷新设计");
    expect(design).toContain("Rail Segments Tab");
    expect(design).toContain("Outline Rail");
    expect(design).toContain("AI Conversation Workspace");
    expect(design).toContain("Overlay Family");
  });
});
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/design-tokens.test.ts
```

Expected: `tests/iris-rail-refresh-contract.test.ts` fails because the new token names are not defined.

- [ ] **Step 3: Add CSS variables**

In `src/styles/globals.css`, add dark values near the existing surface tokens:

```css
--knowledge-accent: 150 12% 54%;
--knowledge-accent-foreground: 150 16% 78%;
--iris-rail-bg: var(--surface-chrome);
--iris-rail-active: 150 12% 54%;
--iris-rail-hover: 0 0% 18%;
--outline-rail-bg: 0 0% 12% / 0.88;
--outline-rail-active: 150 12% 54%;
--ai-workspace-bg: var(--panel);
--ai-workspace-border: 0 0% 22%;
--overlay-task-header: var(--surface-elevated);
--overlay-task-selected: 150 12% 54% / 0.14;
```

Add light values in `.light`:

```css
--knowledge-accent: 150 10% 42%;
--knowledge-accent-foreground: 150 16% 24%;
--iris-rail-bg: var(--surface-chrome);
--iris-rail-active: 150 10% 42%;
--iris-rail-hover: 0 0% 94%;
--outline-rail-bg: 0 0% 100% / 0.9;
--outline-rail-active: 150 10% 42%;
--ai-workspace-bg: var(--panel);
--ai-workspace-border: 0 0% 88%;
--overlay-task-header: var(--surface-elevated);
--overlay-task-selected: 150 10% 42% / 0.1;
```

- [ ] **Step 4: Map Tailwind tokens**

In `tailwind.config.js`, extend colors:

```js
knowledge: {
  accent: "hsl(var(--knowledge-accent))",
  foreground: "hsl(var(--knowledge-accent-foreground))",
},
rail: {
  bg: "hsl(var(--iris-rail-bg))",
  active: "hsl(var(--iris-rail-active))",
  hover: "hsl(var(--iris-rail-hover))",
},
outline: {
  bg: "hsl(var(--outline-rail-bg))",
  active: "hsl(var(--outline-rail-active))",
},
task: {
  header: "hsl(var(--overlay-task-header))",
  selected: "hsl(var(--overlay-task-selected))",
},
```

- [ ] **Step 5: Update design-system token prose**

In `docs/design-system.md`, add a compact “Iris Rail surface tokens” paragraph under the color token section:

```md
Iris Rail Refresh adds semantic surface tokens for the complete interface system: `--iris-rail-*` for brand rail and Rail Segments tabs, `--outline-rail-*` for the editor outline rail, `--ai-workspace-*` for the collaboration sidecar, and `--overlay-task-*` for task-capsule overlays. These tokens are semantic and should not be reused as generic decoration colors.
```

- [ ] **Step 6: Verify tests pass**

Run:

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/design-tokens.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/styles/globals.css tailwind.config.js docs/design-system.md tests/design-tokens.test.ts tests/iris-rail-refresh-contract.test.ts
git commit -m "feat(ui): 添加 Iris Rail 语义 token"
```

---

## Task 2: Persistent Brand Rail, Home Workspace, And Rail Segments Tabs

**Files:**

- Modify: `src/components/layout/DesktopTitleBar.tsx`
- Modify: `src/components/layout/AppBrandZone.tsx`
- Modify: `src/components/layout/WelcomeEmpty.tsx`
- Modify: `src/App.tsx`
- Modify: `src/lib/platform-chrome.ts`
- Modify: `src-tauri/tauri.macos.conf.json`
- Modify: `src/styles/globals.css`
- Test: `tests/desktop-title-bar.test.ts`
- Test: `tests/iris-rail-refresh-contract.test.ts`

- [ ] **Step 1: Write failing chrome and Home contract tests**

Append to `tests/iris-rail-refresh-contract.test.ts`:

```ts
it("defines persistent brand rail, Home view, and Rail Segments tabs", () => {
  const titleBar = read("src/components/layout/DesktopTitleBar.tsx");
  const app = read("src/App.tsx");
  const welcome = read("src/components/layout/WelcomeEmpty.tsx");
  const platform = read("src/lib/platform-chrome.ts");
  const macos = read("src-tauri/tauri.macos.conf.json");

  expect(titleBar).toContain('data-testid="iris-brand-rail"');
  expect(titleBar).toContain('data-testid="rail-segment-tab"');
  expect(titleBar).toContain('data-testid="home-segment"');
  expect(titleBar).toContain("onHome");
  expect(titleBar).toContain("isHomeActive");
  expect(app).toContain("homeActive");
  expect(welcome).toContain('data-testid="home-workbench"');
  expect(platform).toContain("showCustomWindowControls");
  expect(platform).toContain("return isTauriRuntime()");
  expect(macos).toContain('"decorations": false');
});
```

- [ ] **Step 2: Run failing test**

Run:

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/desktop-title-bar.test.ts
```

Expected: FAIL because Home state and Rail Segments test hooks do not exist and macOS still uses native decorations.

- [ ] **Step 3: Update platform chrome policy**

In `src/lib/platform-chrome.ts`, replace `showCustomWindowControls` with:

```ts
/** Iris Rail: all desktop platforms use right-side custom controls. */
export function showCustomWindowControls(): boolean {
  return isTauriRuntime();
}
```

Keep `isMacOSDesktopChrome()` for macOS-specific sizing and fullscreen sync.

- [ ] **Step 4: Update macOS window config**

In `src-tauri/tauri.macos.conf.json`, set:

```json
"decorations": false,
"titleBarStyle": "Overlay",
"hiddenTitle": true
```

Keep `transparent: true`. Leave `trafficLightPosition` only if Tauri accepts it with `decorations: false`; if config validation fails during implementation, remove the `trafficLightPosition` object and update `tests/desktop-title-bar.test.ts` to assert that custom controls replace traffic lights.

- [ ] **Step 5: Add Home state in App**

In `src/App.tsx`, add:

```ts
const [homeActive, setHomeActive] = useState(false);

const showHome = useCallback(() => {
  setHomeActive(true);
}, []);

const leaveHome = useCallback(() => {
  setHomeActive(false);
}, []);
```

Call `leaveHome()` inside the handlers that open, activate, or create notes:

```ts
const handleOpenNoteFromHome = useCallback(
  (path: string) => {
    leaveHome();
    void openNote(path);
  },
  [leaveHome, openNote],
);
```

Use `homeActive || !activePath` to render `WelcomeEmpty`, but keep tabs intact.

- [ ] **Step 6: Extend DesktopTitleBar props**

In `DesktopTitleBarProps`, add:

```ts
isHomeActive?: boolean;
onHome?: () => void;
```

Render brand rail as a button:

```tsx
<button
  type="button"
  data-testid="iris-brand-rail"
  data-tauri-drag-region-exclude
  className={cn(
    "iris-brand-rail flex h-full shrink-0 items-center gap-2 border-r border-border/70 px-3 text-foreground",
    isHomeActive && "iris-brand-rail--active",
  )}
  aria-label="回到 Home"
  onMouseDown={(event) => event.stopPropagation()}
  onClick={onHome}
>
  <IrisMark size={18} />
  <span className="text-sm font-semibold">Iris</span>
</button>
```

Render Home segment when active:

```tsx
{
  isHomeActive ? (
    <div data-testid="home-segment" className="iris-home-segment">
      Home
    </div>
  ) : null;
}
```

Add `data-testid="rail-segment-tab"` to each tab button.

- [ ] **Step 7: Add Rail Segment CSS**

In `src/styles/globals.css`, add component classes:

```css
.iris-brand-rail {
  background: hsl(var(--iris-rail-bg));
}

.iris-brand-rail--active {
  box-shadow: inset 0 -2px 0 0 hsl(var(--iris-rail-active));
}

.iris-rail-tab {
  border-radius: var(--radius-md);
  min-width: 7rem;
  max-width: 14rem;
}

.iris-rail-tab--active {
  background: hsl(var(--surface-inset) / 0.72);
  box-shadow: inset 0 -1.5px 0 0 hsl(var(--iris-rail-active));
}

.iris-home-segment {
  border-radius: var(--radius-md);
  color: hsl(var(--knowledge-accent-foreground));
}
```

- [ ] **Step 8: Upgrade Home workspace**

In `WelcomeEmpty.tsx`, set the root test id:

```tsx
<div data-testid="home-workbench" className="...">
```

Add a compact action strip with text labels exactly:

```tsx
新建笔记
快速打开
全库搜索
AI 系统中心
```

Wire the non-existing action callbacks by extending props:

```ts
onQuickOpen?: () => void;
onSearch?: () => void;
onAiSystemCenter?: () => void;
```

- [ ] **Step 9: Verify**

Run:

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/desktop-title-bar.test.ts
npm run typecheck
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src/App.tsx src/components/layout/DesktopTitleBar.tsx src/components/layout/AppBrandZone.tsx src/components/layout/WelcomeEmpty.tsx src/lib/platform-chrome.ts src-tauri/tauri.macos.conf.json src/styles/globals.css tests/desktop-title-bar.test.ts tests/iris-rail-refresh-contract.test.ts
git commit -m "feat(ui): 实现 Iris Rail 顶栏与 Home 工作台"
```

---

## Task 3: Outline Rail And Quiet Editor Edge Controls

**Files:**

- Modify: `src/components/editor/EditorOutline.tsx`
- Modify: `src/components/editor/TipTapEditor.tsx`
- Modify: `src/components/editor/EditorFindReplaceBar.tsx`
- Modify: `src/styles/globals.css`
- Test: `tests/iris-rail-refresh-contract.test.ts`
- Test: `tests/editor-performance-regression.test.tsx`

- [ ] **Step 1: Write failing Outline Rail test**

Append:

```ts
it("uses Outline Rail instead of a floating outline card", () => {
  const outline = read("src/components/editor/EditorOutline.tsx");
  const editor = read("src/components/editor/TipTapEditor.tsx");
  const css = read("src/styles/globals.css");

  expect(outline).toContain('data-testid="outline-rail"');
  expect(outline).toContain('data-testid="outline-rail-handle"');
  expect(outline).toContain("outline-rail-item--active");
  expect(outline).not.toContain("shadow-floating");
  expect(editor).toContain("editor-edge-control");
  expect(css).toContain(".outline-rail");
  expect(css).toContain(".outline-rail-handle");
});
```

- [ ] **Step 2: Run failing test**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/editor-performance-regression.test.tsx
```

Expected: FAIL because `outline-rail` hooks do not exist.

- [ ] **Step 3: Convert closed state to Rail handle**

In `EditorOutline.tsx`, replace the closed-state `Button` with:

```tsx
<button
  type="button"
  data-testid="outline-rail-handle"
  className="outline-rail-handle pointer-events-auto absolute z-editor-chrome"
  style={{ left: "var(--editor-outline-inset)" }}
  aria-label="显示目录"
  onClick={() => onOpenChange(true)}
>
  <ListTree className="h-3.5 w-3.5" />
  <span className="sr-only">目录</span>
</button>
```

- [ ] **Step 4: Convert open state to Outline Rail**

Set the nav attributes and classes:

```tsx
<nav
  data-testid="outline-rail"
  className="outline-rail pointer-events-none absolute z-editor-chrome flex max-h-[min(70dvh,28rem)] w-[var(--editor-outline-width)] flex-col"
  style={{ left: "var(--editor-outline-inset)" }}
  aria-label="文档目录"
>
```

For active items:

```tsx
className={cn(
  "outline-rail-item",
  `outline-rail-item--level-${entry.level}`,
  index === activeIndex && "outline-rail-item--active",
)}
```

- [ ] **Step 5: Add edge-control class hooks**

In `TipTapEditor.tsx`, add `editor-edge-control` to the lock button:

```tsx
className = "editor-edge-control editor-lock-btn absolute right-3 top-3 ...";
```

In `EditorFindReplaceBar.tsx`, add `editor-edge-control` to the root class:

```tsx
className = "iris-find-replace-bar editor-edge-control";
```

- [ ] **Step 6: Add Outline Rail CSS**

In `globals.css`:

```css
.outline-rail {
  top: 1rem;
}

.outline-rail > div {
  background: hsl(var(--outline-rail-bg));
  border: 1px solid hsl(var(--border) / 0.55);
  border-radius: var(--radius-lg);
  box-shadow: none;
  backdrop-filter: blur(12px);
}

.outline-rail-handle {
  top: 1rem;
  display: inline-flex;
  height: 2rem;
  width: 2rem;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-md);
  border: 1px solid hsl(var(--border) / 0.55);
  background: hsl(var(--outline-rail-bg));
  color: hsl(var(--muted-foreground));
}

.outline-rail-item {
  position: relative;
  width: 100%;
  border-radius: var(--radius-sm);
  padding: 0.25rem 0.375rem;
  text-align: left;
  color: hsl(var(--muted-foreground));
}

.outline-rail-item--level-2 {
  padding-left: 0.875rem;
}

.outline-rail-item--level-3 {
  padding-left: 1.25rem;
}

.outline-rail-item--active {
  color: hsl(var(--foreground));
  background: hsl(var(--overlay-task-selected));
}

.outline-rail-item--active::before {
  position: absolute;
  left: 0.125rem;
  top: 0.375rem;
  bottom: 0.375rem;
  width: 2px;
  border-radius: 999px;
  background: hsl(var(--outline-rail-active));
  content: "";
}
```

- [ ] **Step 7: Verify**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/editor-performance-regression.test.tsx
npm run typecheck
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/components/editor/EditorOutline.tsx src/components/editor/TipTapEditor.tsx src/components/editor/EditorFindReplaceBar.tsx src/styles/globals.css tests/iris-rail-refresh-contract.test.ts tests/editor-performance-regression.test.tsx
git commit -m "feat(editor): 实现边缘目录轨与编辑器边缘控件"
```

---

## Task 4: AI System Center And Lightweight General Settings

**Files:**

- Create: `src/components/settings/AiSystemCenterPanel.tsx`
- Modify: `src/components/settings/SettingsPanel.tsx`
- Modify: `src/hooks/useOverlayManager.ts`
- Modify: `src/lib/command-palette.ts`
- Modify: `src/lib/command-palette-icons.ts`
- Modify: `src/App.tsx`
- Test: `tests/iris-rail-refresh-contract.test.ts`
- Test: `tests/unified-assistant-shell.test.ts`
- Test: `tests/command-palette.test.ts`

- [ ] **Step 1: Write failing AI System Center test**

Append:

```ts
it("moves AI configuration into AI System Center", () => {
  const settings = read("src/components/settings/SettingsPanel.tsx");
  const aiCenter = read("src/components/settings/AiSystemCenterPanel.tsx");
  const overlays = read("src/hooks/useOverlayManager.ts");
  const palette = read("src/lib/command-palette.ts");
  const app = read("src/App.tsx");

  expect(aiCenter).toContain('data-testid="ai-system-center"');
  expect(aiCenter).toContain("LlmRoutingSection");
  expect(aiCenter).toContain("MinimaxSearchSection");
  expect(aiCenter).toContain("PersonaSettingsPanel");
  expect(aiCenter).toContain("SkillsPanel");
  expect(aiCenter).toContain("AiRulesPanel");
  expect(settings).not.toContain("LlmRoutingSection");
  expect(settings).not.toContain("MinimaxSearchSection");
  expect(settings).not.toContain("AiRulesPanel");
  expect(overlays).toContain('"aiSystemCenter"');
  expect(palette).toContain("ai-system-center");
  expect(app).toContain("AiSystemCenterPanel");
});
```

- [ ] **Step 2: Run failing tests**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/unified-assistant-shell.test.ts tests/command-palette.test.ts
```

Expected: FAIL because the new panel and overlay id do not exist.

- [ ] **Step 3: Create AI System Center panel**

Create `src/components/settings/AiSystemCenterPanel.tsx`:

```tsx
import { useState } from "react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { PersonaSettingsPanel } from "@/components/settings/PersonaSettingsPanel";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";

interface AiSystemCenterPanelProps {
  open: boolean;
  onClose: () => void;
}

export function AiSystemCenterPanel({
  open,
  onClose,
}: AiSystemCenterPanelProps) {
  const [personaOpen, setPersonaOpen] = useState(false);
  const [skillsOpen, setSkillsOpen] = useState(false);

  return (
    <>
      <IrisOverlay
        open={open}
        onClose={onClose}
        title="AI 系统中心"
        size="wide"
      >
        <div data-testid="ai-system-center" className="flex min-h-0 flex-1">
          <aside className="w-48 shrink-0 border-r border-border/60 bg-surface-inset/20 p-3 text-xs text-muted-foreground">
            模型 · 联网 · 人格 · Skills · 记忆
          </aside>
          <ScrollArea className="min-h-0 flex-1">
            <div className="space-y-6 px-5 py-4">
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  模型路由
                </h3>
                <LlmRoutingSection open={open} />
              </section>
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  联网搜索
                </h3>
                <MinimaxSearchSection open={open} />
              </section>
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  人格与 Skills
                </h3>
                <div className="flex gap-2">
                  <button type="button" onClick={() => setPersonaOpen(true)}>
                    打开人格配置
                  </button>
                  <button type="button" onClick={() => setSkillsOpen(true)}>
                    管理 Skills
                  </button>
                </div>
              </section>
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  AI 记忆与规则
                </h3>
                <AiRulesPanel compact />
              </section>
            </div>
          </ScrollArea>
        </div>
      </IrisOverlay>
      <PersonaSettingsPanel
        open={personaOpen}
        onClose={() => setPersonaOpen(false)}
      />
      <SkillsPanel open={skillsOpen} onClose={() => setSkillsOpen(false)} />
    </>
  );
}
```

After the minimal pass, replace raw `<button>` elements with existing `Button` components.

- [ ] **Step 4: Slim SettingsPanel**

Remove imports and sections for `LlmRoutingSection`, `MinimaxSearchSection`, `AiRulesPanel`, `SkillsPanel`, and `PersonaSettingsPanel`. Keep appearance and about sections.

- [ ] **Step 5: Add overlay id**

In `useOverlayManager.ts`, add `"aiSystemCenter"` to `OverlayId`, `SIDE_PANELS`, booleans, and setter:

```ts
const aiSystemCenterOpen = activeOverlay === "aiSystemCenter";
setAiSystemCenterOpen: (open: boolean) =>
  setOverlayOpen("aiSystemCenter", open),
```

- [ ] **Step 6: Add command palette entry**

In `buildCommandPaletteItems`, add:

```ts
{
  id: "ai-system-center",
  label: "AI 系统中心",
  group: "AI",
  keywords: "ai system center model search skills memory rules 系统 中心 模型 联网 人格 规则",
  icon: "SlidersHorizontal",
  action: { type: "openOverlay", overlay: "aiSystemCenter" },
},
```

Add `SlidersHorizontal` in `command-palette-icons.ts` if absent.

- [ ] **Step 7: Wire App**

Lazy import `AiSystemCenterPanel` and render it in overlays:

```tsx
<AiSystemCenterPanel
  open={overlays.aiSystemCenterOpen}
  onClose={() => overlays.closeOverlay("aiSystemCenter")}
/>
```

Pass `onAiSystemCenter={() => overlays.openOverlay("aiSystemCenter")}` to `WelcomeEmpty`.

- [ ] **Step 8: Update tests that expected AI config in SettingsPanel**

In `tests/unified-assistant-shell.test.ts`, replace the old settings assertion with:

```ts
it("AI System Center hosts persona, rules, and model config", () => {
  const source = read("src/components/settings/AiSystemCenterPanel.tsx");
  expect(source).toContain("PersonaSettingsPanel");
  expect(source).toContain("AiRulesPanel");
  expect(source).toContain("MinimaxSearchSection");
  expect(read("src/components/settings/SettingsPanel.tsx")).not.toContain(
    "MinimaxSearchSection",
  );
});
```

- [ ] **Step 9: Verify**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/unified-assistant-shell.test.ts tests/command-palette.test.ts
npm run typecheck
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src/App.tsx src/components/settings/AiSystemCenterPanel.tsx src/components/settings/SettingsPanel.tsx src/hooks/useOverlayManager.ts src/lib/command-palette.ts src/lib/command-palette-icons.ts tests/iris-rail-refresh-contract.test.ts tests/unified-assistant-shell.test.ts tests/command-palette.test.ts
git commit -m "feat(ai): 添加 AI 系统中心并精简设置页"
```

---

## Task 5: AI Collaboration Sidecar Surface Language

**Files:**

- Modify: `src/components/ai/UnifiedAssistantPanel.tsx`
- Modify: `src/components/ai/AiMessageBubble.tsx`
- Modify: `src/components/ai/ConversationSurface.tsx`
- Modify: `src/components/ui/ai-composer.tsx`
- Modify: `src/components/ai/ContextPacketDrawer.tsx`
- Modify: `src/components/ai/PatchPreview.tsx`
- Modify: `src/components/ai/ToolConfirmDialog.tsx`
- Modify: `src/components/ai/RuleConfirmDialog.tsx`
- Modify: `src/styles/globals.css`
- Test: `tests/iris-rail-refresh-contract.test.ts`
- Test: `tests/unified-assistant-shell.test.ts`

- [ ] **Step 1: Write failing AI sidecar contract**

Append:

```ts
it("defines AI collaboration sidecar surfaces", () => {
  const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
  const bubble = read("src/components/ai/AiMessageBubble.tsx");
  const composer = read("src/components/ui/ai-composer.tsx");
  const css = read("src/styles/globals.css");

  expect(panel).toContain("ai-sidecar");
  expect(panel).toContain("ai-sidecar-header");
  expect(panel).toContain("ai-task-surface");
  expect(bubble).toContain("ai-message-surface-assistant");
  expect(bubble).toContain("ai-message-surface-user");
  expect(composer).toContain("ai-composer-workbench");
  expect(css).toContain(".ai-sidecar");
  expect(css).toContain(".ai-composer-workbench");
});
```

- [ ] **Step 2: Run failing tests**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/unified-assistant-shell.test.ts
```

Expected: FAIL because sidecar class hooks do not exist.

- [ ] **Step 3: Add panel class hooks**

In `UnifiedAssistantPanel.tsx`, change root:

```tsx
className = "ai-sidecar flex h-full flex-col bg-ai-workspace";
```

Change header:

```tsx
<header className="ai-sidecar-header shrink-0 border-b border-border/60 px-3 py-1.5">
```

Add `ai-task-surface` to research progress, document artifacts, citation, organize, and patch preview wrappers.

- [ ] **Step 4: Add message class hooks**

In `AiMessageBubble.tsx`, add:

```tsx
"ai-message-surface-user";
```

to user bubble and:

```tsx
"ai-message-surface-assistant";
```

to assistant bubble.

- [ ] **Step 5: Add composer class hook**

In `src/components/ui/ai-composer.tsx`, change the composer wrapper class to include:

```tsx
"ai-composer-workbench";
```

- [ ] **Step 6: Add AI sidecar CSS**

In `globals.css`:

```css
.ai-sidecar {
  background: hsl(var(--ai-workspace-bg));
}

.ai-sidecar-header {
  background: hsl(var(--surface-chrome) / 0.72);
}

.ai-task-surface {
  border: 1px solid hsl(var(--ai-workspace-border) / 0.72);
  border-radius: var(--radius-lg);
  background: hsl(var(--surface-elevated) / 0.62);
}

.ai-message-surface-assistant {
  border-color: hsl(var(--ai-workspace-border) / 0.78);
}

.ai-message-surface-user {
  background: hsl(var(--surface-inset) / 0.82);
}

.ai-composer-workbench {
  border-color: hsl(var(--ai-workspace-border) / 0.9);
  background: hsl(var(--surface-elevated));
}
```

- [ ] **Step 7: Verify**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/unified-assistant-shell.test.ts
npm run typecheck
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/components/ai/UnifiedAssistantPanel.tsx src/components/ai/AiMessageBubble.tsx src/components/ai/ConversationSurface.tsx src/components/ui/ai-composer.tsx src/components/ai/ContextPacketDrawer.tsx src/components/ai/PatchPreview.tsx src/components/ai/ToolConfirmDialog.tsx src/components/ai/RuleConfirmDialog.tsx src/styles/globals.css tests/iris-rail-refresh-contract.test.ts tests/unified-assistant-shell.test.ts
git commit -m "feat(ai): 统一 AI 协作侧车视觉语言"
```

---

## Task 6: Task-Capsule Overlay Family

**Files:**

- Modify: `src/components/ui/iris-overlay.tsx`
- Modify: `src/components/ui/overlay-chrome.tsx`
- Modify: `src/lib/overlay-sizes.ts`
- Modify: `src/components/layout/CommandPalette.tsx`
- Modify: `src/components/file/QuickOpen.tsx`
- Modify: `src/components/file/SearchPanel.tsx`
- Modify: `src/components/file/VaultNavigator.tsx`
- Modify: `src/components/version/VersionTimeline.tsx`
- Modify: `src/components/graph/GraphView.tsx`
- Modify: `src/components/ai/SkillsPanel.tsx`
- Modify: `src/components/settings/PersonaSettingsPanel.tsx`
- Modify: `src/styles/globals.css`
- Test: `tests/overlay-sizes.test.ts`
- Test: `tests/iris-overlay.test.tsx`
- Test: `tests/iris-rail-refresh-contract.test.ts`

- [ ] **Step 1: Write failing overlay family contract**

Append:

```ts
it("uses task-capsule overlay family hooks across command surfaces", () => {
  const overlay = read("src/components/ui/iris-overlay.tsx");
  const chrome = read("src/components/ui/overlay-chrome.tsx");
  const search = read("src/components/file/SearchPanel.tsx");
  const quickOpen = read("src/components/file/QuickOpen.tsx");
  const command = read("src/components/layout/CommandPalette.tsx");

  expect(overlay).toContain("task-overlay");
  expect(chrome).toContain("task-overlay-header");
  expect(chrome).toContain("task-overlay-footer");
  expect(search).toContain("task-overlay-filter");
  expect(quickOpen).toContain("task-overlay-results");
  expect(command).toContain("task-overlay-results");
});
```

- [ ] **Step 2: Run failing tests**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/overlay-sizes.test.ts tests/iris-overlay.test.tsx
```

Expected: FAIL because task overlay hooks do not exist.

- [ ] **Step 3: Add shared overlay shell classes**

In `iris-overlay.tsx`, add `task-overlay` to content:

```tsx
className={irisOverlayPanelClass(size, cn("task-overlay", className))}
```

In `overlay-chrome.tsx`, add:

```tsx
task - overlay - header;
task - overlay - body;
task - overlay - footer;
```

to the existing header wrapper, the main content wrapper, and the footer wrapper in `OverlayChrome`. If a wrapper is rendered through props, apply the class on the concrete `<div>` that owns the border/background for that region.

- [ ] **Step 4: Add result/filter hooks to key overlays**

In `SearchPanel.tsx`, add:

```tsx
className = "task-overlay-filter ...";
className = "task-overlay-results min-h-0 flex-1 px-2";
```

In `QuickOpen.tsx` and `CommandPalette.tsx`, add `task-overlay-results` to the scroll/list area wrapper.

In `VaultNavigator.tsx`, `VersionTimeline.tsx`, `SkillsPanel.tsx`, and `PersonaSettingsPanel.tsx`, add `task-overlay-filter` for top filter/header regions and `task-overlay-results` for main list/detail regions.

- [ ] **Step 5: Add task overlay CSS**

In `globals.css`:

```css
.task-overlay {
  background: hsl(var(--panel));
}

.task-overlay-header {
  background: hsl(var(--overlay-task-header));
}

.task-overlay-filter {
  border-bottom: 1px solid hsl(var(--border) / 0.6);
  background: hsl(var(--surface-inset) / 0.28);
}

.task-overlay-results [aria-selected="true"],
.task-overlay-results [data-active="true"] {
  background: hsl(var(--overlay-task-selected));
}

.task-overlay-footer {
  border-top: 1px solid hsl(var(--border) / 0.6);
  background: hsl(var(--surface-inset) / 0.34);
}
```

- [ ] **Step 6: Verify**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts tests/overlay-sizes.test.ts tests/iris-overlay.test.tsx
npm run typecheck
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/components/ui/iris-overlay.tsx src/components/ui/overlay-chrome.tsx src/lib/overlay-sizes.ts src/components/layout/CommandPalette.tsx src/components/file/QuickOpen.tsx src/components/file/SearchPanel.tsx src/components/file/VaultNavigator.tsx src/components/version/VersionTimeline.tsx src/components/graph/GraphView.tsx src/components/ai/SkillsPanel.tsx src/components/settings/PersonaSettingsPanel.tsx src/styles/globals.css tests/overlay-sizes.test.ts tests/iris-overlay.test.tsx tests/iris-rail-refresh-contract.test.ts
git commit -m "feat(ui): 统一任务舱悬浮页语言"
```

---

## Task 7: Documentation And Manual Acceptance Checklist

**Files:**

- Modify: `docs/design-system.md`
- Modify: `ROADMAP.md`
- Create: `docs/testing/iris-rail-refresh-manual-checklist.md`
- Test: `tests/iris-rail-refresh-contract.test.ts`

- [ ] **Step 1: Write failing documentation contract**

Append:

```ts
it("ships a manual checklist for the complete Iris Rail refresh", () => {
  const checklist = read("docs/testing/iris-rail-refresh-manual-checklist.md");
  expect(checklist).toContain("macOS 顶栏与右侧窗口控制");
  expect(checklist).toContain("Rail Segments Tab");
  expect(checklist).toContain("Outline Rail 长文");
  expect(checklist).toContain("AI 协作侧车长对话");
  expect(checklist).toContain("任务舱 Overlay");
});
```

- [ ] **Step 2: Run failing documentation test**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts
```

Expected: FAIL because the checklist file does not exist.

- [ ] **Step 3: Create manual checklist**

Create `docs/testing/iris-rail-refresh-manual-checklist.md`:

```md
# Iris Rail Refresh Manual Checklist

## macOS 顶栏与右侧窗口控制

- [ ] 左侧 Iris 品牌轨道始终可见。
- [ ] 右侧最小化、最大化、关闭按钮可点击，拖拽区域不吞点击。
- [ ] 全屏进入和退出后顶栏不重叠。

## Rail Segments Tab

- [ ] 激活、未激活、hover、focus、dirty、close button 状态清楚。
- [ ] 长标题截断稳定，不挤压品牌轨道和窗口控制。
- [ ] Home 状态不伪装为普通文档 tab。

## Outline Rail 长文

- [ ] 展开态不压迫正文。
- [ ] 收起 handle 位置稳定。
- [ ] 当前章节 marker 随光标变化。
- [ ] 空目录状态轻量。

## AI 协作侧车长对话

- [ ] 用户消息、助手消息、证据包、patch preview、研究状态层级清楚。
- [ ] Composer 在底部稳定可用。
- [ ] 工具确认和规则确认与 AI sidecar 视觉一致。

## 任务舱 Overlay

- [ ] 命令面板、Quick Open、搜索、文件管理、版本、图谱均使用统一任务舱 chrome。
- [ ] 键盘导航和底部快捷键提示清楚。
- [ ] 空态、结果卡、选中态一致。
```

- [ ] **Step 4: Update docs**

In `docs/design-system.md`, add a link to the manual checklist near the Iris Rail spec link:

```md
手工验收清单：见 [Iris Rail Refresh Manual Checklist](./testing/iris-rail-refresh-manual-checklist.md)。
```

In `ROADMAP.md`, keep the existing Iris Rail target sentence and do not add version promises.

- [ ] **Step 5: Verify**

```bash
npm run test -- tests/iris-rail-refresh-contract.test.ts
npm run format:check
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add docs/design-system.md ROADMAP.md docs/testing/iris-rail-refresh-manual-checklist.md tests/iris-rail-refresh-contract.test.ts
git commit -m "docs(ui): 添加 Iris Rail 手工验收清单"
```

---

## Task 8: Full Verification And Final Review

**Files:**

- No required source changes.
- Review all files touched by Tasks 1-7.

- [ ] **Step 1: Run full frontend verification**

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
```

Expected: all commands pass.

- [ ] **Step 2: Run Rust checks if macOS Tauri config changed**

```bash
cargo check
```

Expected: pass. If this command is too broad for the workspace, run the existing project Rust verification command documented in `AGENTS.md`.

- [ ] **Step 3: Inspect git diff**

```bash
git status --short
git diff --stat
```

Expected: only Iris Rail UI refresh files remain modified. `src-tauri/src/embedding/engine.rs` must not be staged unless the user explicitly says it belongs to this work.

- [ ] **Step 4: Manual app smoke test**

Run:

```bash
npm run tauri dev
```

Expected manual checks:

- Home opens and brand rail returns to Home without closing document tabs.
- Rail Segments tabs handle active, dirty, hover, close, and long-title states.
- Outline Rail opens, closes, and follows current heading in a long note.
- AI sidecar keeps messages, evidence, patch previews, confirmations, and composer visually coherent.
- Command Palette, Quick Open, Search, Version Timeline, Graph, Skills, and AI System Center share task-capsule overlay chrome.

- [ ] **Step 5: Commit final fixes if needed**

If verification required small fixes:

```bash
git add <changed-files>
git commit -m "fix(ui): 修正 Iris Rail 验收问题"
```

If no fixes were needed, do not create an empty commit.
