# Iris Performance Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove avoidable cold-start network work, reduce AI streaming main-thread pressure, preserve existing editor surface caching, and prepare a contract-safe Worker path for assistant Markdown rendering.

**Architecture:** The plan keeps the current React/Tauri architecture and improves only the hot paths identified in the design spec. Low-risk changes land first: measurable performance baselines, local font loading, deferred streaming render snapshots, and finalized-message paint containment. Deep changes are isolated behind a small Markdown render Worker boundary that reuses the existing markdown contract instead of creating a second renderer.

**Tech Stack:** Tauri 2, Rust, React 19, TypeScript, Vite workers, TipTap/ProseMirror, TailwindCSS, Vitest, Prettier, ESLint.

---

## Source Spec

- Design spec: `docs/superpowers/specs/2026-06-28-iris-perf-optimization-design.md`
- Existing performance guide: `docs/ops/performance-guide.md`
- Existing AI streaming contracts:
  - `tests/assistant-streaming-lifecycle.test.tsx`
  - `tests/assistant-stream-rendering-performance-contract.test.ts`
  - `tests/assistant-panel-performance-contract.test.ts`
  - `tests/ai-code-copy.test.tsx`
- Existing editor cache contracts:
  - `tests/editor-html-cache.test.ts`
  - `tests/document-open-budget-contract.test.ts`

---

## File Map

### Documentation

- Modify `docs/ops/performance-guide.md`: add the reproducible measurement matrix for this performance work.
- Keep `docs/superpowers/specs/2026-06-28-iris-perf-optimization-design.md` unchanged unless execution discovers a contradiction.

### Startup Fonts

- Modify `index.html`: remove Google Fonts preconnect and stylesheet links; preload local Inter font files.
- Modify `src/styles/globals.css`: define local `@font-face` entries and keep CJK fonts on system fallback by default.
- Create `src/assets/fonts/OFL.txt`: record SIL Open Font License text or a license pointer used by the imported fonts.
- Create font assets under `src/assets/fonts/`:
  - `inter-latin-400-normal.woff2`
  - `inter-latin-500-normal.woff2`
  - `inter-latin-600-normal.woff2`
  - `inter-latin-700-normal.woff2`
  - `jetbrains-mono-latin-400-normal.woff2`
  - `jetbrains-mono-latin-500-normal.woff2`
- Create `tests/startup-fonts-contract.test.ts`: lock no external Google Fonts and require local font-face declarations.

### Assistant Streaming Main-Thread Relief

- Modify `src/components/ai/AiMessageBubble.tsx`: use `useDeferredValue` for streaming render snapshots and keep final content synchronous.
- Modify `tests/assistant-streaming-lifecycle.test.tsx`: lock the deferred streaming render contract.
- Modify `tests/ai-code-copy.test.tsx`: keep code rendering and copy-control behavior stable.

### Finalized Message Paint Containment

- Modify `src/styles/globals.css`: add `content-visibility: auto` only for non-streaming assistant bubbles.
- Modify `src/components/ai/AiMessageBubble.tsx`: add stable data attributes that distinguish assistant/user and streaming/finalized states if current classes are insufficient.
- Modify `tests/assistant-stream-rendering-performance-contract.test.ts`: lock streaming bubble exclusion and finalized bubble inclusion.

### Editor Surface Cache Audit

- Modify `tests/editor-html-cache.test.ts`: add a source-level contract that forbids adding `EditorViewCache` / WeakRef editor pools and preserves path-stable surfaces.
- Modify `docs/ops/performance-guide.md`: add manual checks for editor surface hit/miss behavior.
- Modify `src/components/layout/AppEditorWorkspace.tsx` only if profiler evidence shows unnecessary hidden-surface work; keep this task observational unless the evidence is recorded in the guide.

### Worker Markdown Rendering

- Create `src/lib/markdown-render-worker-core.ts`: pure, Node-testable worker rendering core that calls `renderMarkdownWithProfile`.
- Create `src/workers/markdown-render.worker.ts`: worker message loop and duplicate/abort result filtering.
- Create `src/hooks/useMarkdownRenderWorker.ts`: narrow hook for worker lifecycle and async result management.
- Modify `src/components/ai/AiMessageBubble.tsx`: use worker HTML only for streaming assistant content; keep sync fallback.
- Create `tests/markdown-render-worker-core.test.ts`: verify worker core output equals existing sync contract.
- Create `tests/markdown-render-worker-source-contract.test.ts`: lock Worker behavior that cannot be executed in Node without browser Worker support.

---

## Commit Boundaries

Use Chinese Conventional Commits. Suggested commits:

1. `docs(ui): 补充性能优化基线采样指南`
2. `perf(ui): 移除启动字体外部网络依赖`
3. `perf(ai): 降低流式 Markdown 渲染抢占`
4. `perf(ai): 为已完成消息启用离屏渲染跳过`
5. `test(editor): 锁定现有编辑器 surface 缓存边界`
6. `perf(ai): 增加流式 Markdown Worker 渲染通道`

Do not commit generated trace files unless they are intentionally small fixtures. Keep DevTools trace artifacts outside git or add only summarized measurements to `docs/ops/performance-guide.md`.

---

### Task 1: Performance Baseline Documentation

**Files:**

- Modify: `docs/ops/performance-guide.md`
- Create: `tests/performance-guide-contract.test.ts`

- [ ] **Step 1: Write the failing contract test**

Create `tests/performance-guide-contract.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("performance guide contract", () => {
  it("documents Iris performance optimization baseline scenarios", () => {
    const guide = read("docs/ops/performance-guide.md");

    expect(guide).toContain("Iris Performance Optimization Baselines");
    expect(guide).toContain("离线冷启动");
    expect(guide).toContain("AI 长流式输出");
    expect(guide).toContain("已打开 Tab 快速回切");
    expect(guide).toContain("10000+ 文件库");
  });

  it("requires before and after measurements before claiming performance completion", () => {
    const guide = read("docs/ops/performance-guide.md");

    expect(guide).toContain("改前");
    expect(guide).toContain("改后");
    expect(guide).toContain(">50ms long task");
    expect(guide).toContain("React Profiler");
    expect(guide).toContain("DevTools Performance");
  });

  it("states that traces must not include user note content or credentials", () => {
    const guide = read("docs/ops/performance-guide.md");

    expect(guide).toContain("不得包含用户笔记正文");
    expect(guide).toContain("不得包含 API Key");
    expect(guide).toContain("不得包含解密后的涉密内容");
  });
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
npm run test -- tests/performance-guide-contract.test.ts
```

Expected: FAIL because the new baseline section is not yet present in `docs/ops/performance-guide.md`.

- [ ] **Step 3: Update the performance guide**

Append this section to `docs/ops/performance-guide.md`:

```md
## Iris Performance Optimization Baselines

本节用于执行 `2026-06-28-iris-perf-optimization-design.md` 的改前 / 改后量测。没有改前和改后记录，不得声称性能优化完成。

### 记录模板

| 字段          | 内容                                                             |
| ------------- | ---------------------------------------------------------------- |
| 日期          | YYYY-MM-DD                                                       |
| 分支 / commit | 记录当前 commit hash                                             |
| 构建模式      | development / production                                         |
| 机器          | CPU、内存、系统版本                                              |
| 数据集        | vault 文件数、样本文档大小                                       |
| 场景          | 离线冷启动 / AI 长流式输出 / 已打开 Tab 快速回切 / 10000+ 文件库 |
| 改前          | 关键指标、trace 名称、截图路径                                   |
| 改后          | 关键指标、trace 名称、截图路径                                   |
| 结论          | 是否达到本节预算                                                 |

### 必测场景

| 场景                | 步骤                                                    | 关注指标                                                                 |
| ------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------ |
| 离线冷启动          | 断开网络，启动 Iris，打开 DevTools Network              | 外部字体请求为 0，首屏不等待字体 CSS                                     |
| AI 长流式输出       | 使用 3000-8000 字输出样本，包含段落、表格、引用、代码块 | DevTools Performance 中 >50ms long task 数量，React Profiler commit 分布 |
| 已打开 Tab 快速回切 | 打开 5 个 Markdown 文档后连续切换                       | 已打开 tab 不重新 ingest Markdown，不重新显示冷打开 loading surface      |
| 10000+ 文件库       | 选择大 vault，等待索引稳定后打开搜索、文件树、编辑器    | 首次可交互时间、索引结束后 CPU 回落时间                                  |

### 隐私要求

性能 trace、截图、日志和文档摘要不得包含用户笔记正文、frontmatter、prompt、API Key、Token、解密后的涉密内容或凭据材料。需要说明笔记规模时，只记录文件数、字节数、路径 hash 或合成 fixture 名称。
```

- [ ] **Step 4: Run the focused test and verify it passes**

Run:

```bash
npm run test -- tests/performance-guide-contract.test.ts
```

Expected: PASS.

- [ ] **Step 5: Run formatting**

Run:

```bash
npm run format:check
```

Expected: PASS. If it fails on the Markdown table, run:

```bash
npx prettier --write docs/ops/performance-guide.md tests/performance-guide-contract.test.ts
npm run format:check
```

- [ ] **Step 6: Commit**

```bash
git add docs/ops/performance-guide.md tests/performance-guide-contract.test.ts
git commit -m "docs(ui): 补充性能优化基线采样指南"
```

---

### Task 2: Startup Fonts Without External Network

**Files:**

- Modify: `index.html`
- Modify: `src/styles/globals.css`
- Create: `src/assets/fonts/OFL.txt`
- Create: `src/assets/fonts/inter-latin-400-normal.woff2`
- Create: `src/assets/fonts/inter-latin-500-normal.woff2`
- Create: `src/assets/fonts/inter-latin-600-normal.woff2`
- Create: `src/assets/fonts/inter-latin-700-normal.woff2`
- Create: `src/assets/fonts/jetbrains-mono-latin-400-normal.woff2`
- Create: `src/assets/fonts/jetbrains-mono-latin-500-normal.woff2`
- Create: `tests/startup-fonts-contract.test.ts`

- [ ] **Step 1: Write the failing font contract test**

Create `tests/startup-fonts-contract.test.ts`:

```ts
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("startup font loading contract", () => {
  it("does not load Google Fonts or gstatic during startup", () => {
    const html = read("index.html");

    expect(html).not.toContain("fonts.googleapis.com");
    expect(html).not.toContain("fonts.gstatic.com");
    expect(html).not.toContain("display=swap");
  });

  it("preloads only local first-viewport fonts", () => {
    const html = read("index.html");

    expect(html).toContain('rel="preload"');
    expect(html).toContain('as="font"');
    expect(html).toContain("/src/assets/fonts/inter-latin-400-normal.woff2");
    expect(html).toContain("/src/assets/fonts/inter-latin-600-normal.woff2");
    expect(html).not.toContain("Noto+Sans+SC");
    expect(html).not.toContain("Noto+Serif+SC");
  });

  it("declares local Inter and JetBrains Mono faces with system CJK fallback", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain("@font-face");
    expect(css).toContain('font-family: "Inter"');
    expect(css).toContain('font-family: "JetBrains Mono"');
    expect(css).toContain("font-display: swap");
    expect(css).toContain("--font-sans");
    expect(css).toContain("PingFang SC");
    expect(css).toContain("Microsoft YaHei");
  });

  it("keeps font licenses with the bundled font assets", () => {
    expect(existsSync("src/assets/fonts/OFL.txt")).toBe(true);
  });
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
npm run test -- tests/startup-fonts-contract.test.ts
```

Expected: FAIL because `index.html` still references Google Fonts and the local assets do not exist.

- [ ] **Step 3: Add the font assets**

Fetch Latin-only Inter and JetBrains Mono woff2 files from `@fontsource` package assets without adding npm dependencies:

```bash
mkdir -p src/assets/fonts
curl -L "https://unpkg.com/@fontsource/inter@5.2.8/files/inter-latin-400-normal.woff2" -o src/assets/fonts/inter-latin-400-normal.woff2
curl -L "https://unpkg.com/@fontsource/inter@5.2.8/files/inter-latin-500-normal.woff2" -o src/assets/fonts/inter-latin-500-normal.woff2
curl -L "https://unpkg.com/@fontsource/inter@5.2.8/files/inter-latin-600-normal.woff2" -o src/assets/fonts/inter-latin-600-normal.woff2
curl -L "https://unpkg.com/@fontsource/inter@5.2.8/files/inter-latin-700-normal.woff2" -o src/assets/fonts/inter-latin-700-normal.woff2
curl -L "https://unpkg.com/@fontsource/jetbrains-mono@5.2.8/files/jetbrains-mono-latin-400-normal.woff2" -o src/assets/fonts/jetbrains-mono-latin-400-normal.woff2
curl -L "https://unpkg.com/@fontsource/jetbrains-mono@5.2.8/files/jetbrains-mono-latin-500-normal.woff2" -o src/assets/fonts/jetbrains-mono-latin-500-normal.woff2
curl -L "https://unpkg.com/@fontsource/inter@5.2.8/LICENSE" -o /tmp/inter-OFL.txt
curl -L "https://unpkg.com/@fontsource/jetbrains-mono@5.2.8/LICENSE" -o /tmp/jetbrains-mono-OFL.txt
```

Create `src/assets/fonts/OFL.txt` with:

```txt
Bundled fonts:

- Inter latin woff2 files sourced from @fontsource/inter 5.2.8.
- JetBrains Mono latin woff2 files sourced from @fontsource/jetbrains-mono 5.2.8.

Both font families are distributed under the SIL Open Font License 1.1.
The downloaded license texts used during import were:

- /tmp/inter-OFL.txt
- /tmp/jetbrains-mono-OFL.txt

When refreshing these assets, verify the upstream license remains OFL-compatible before replacing files.
```

- [ ] **Step 4: Update `index.html`**

Remove the Google Fonts `preconnect` and stylesheet block. Add local preloads inside `<head>`:

```html
<link
  rel="preload"
  href="/src/assets/fonts/inter-latin-400-normal.woff2"
  as="font"
  type="font/woff2"
  crossorigin
/>
<link
  rel="preload"
  href="/src/assets/fonts/inter-latin-600-normal.woff2"
  as="font"
  type="font/woff2"
  crossorigin
/>
```

Do not preload CJK fonts in this task.

- [ ] **Step 5: Update `src/styles/globals.css`**

Add these `@font-face` declarations near the top of the file before Tailwind layers:

```css
@font-face {
  font-family: "Inter";
  src: url("/src/assets/fonts/inter-latin-400-normal.woff2") format("woff2");
  font-display: swap;
  font-style: normal;
  font-weight: 400;
}

@font-face {
  font-family: "Inter";
  src: url("/src/assets/fonts/inter-latin-500-normal.woff2") format("woff2");
  font-display: swap;
  font-style: normal;
  font-weight: 500;
}

@font-face {
  font-family: "Inter";
  src: url("/src/assets/fonts/inter-latin-600-normal.woff2") format("woff2");
  font-display: swap;
  font-style: normal;
  font-weight: 600;
}

@font-face {
  font-family: "Inter";
  src: url("/src/assets/fonts/inter-latin-700-normal.woff2") format("woff2");
  font-display: swap;
  font-style: normal;
  font-weight: 700;
}

@font-face {
  font-family: "JetBrains Mono";
  src: url("/src/assets/fonts/jetbrains-mono-latin-400-normal.woff2")
    format("woff2");
  font-display: swap;
  font-style: normal;
  font-weight: 400;
}

@font-face {
  font-family: "JetBrains Mono";
  src: url("/src/assets/fonts/jetbrains-mono-latin-500-normal.woff2")
    format("woff2");
  font-display: swap;
  font-style: normal;
  font-weight: 500;
}
```

Ensure the root font variables keep CJK system fallback:

```css
--font-sans:
  Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
  "Microsoft YaHei", "Noto Sans SC", sans-serif;
--font-title:
  "Noto Serif SC", "Songti SC", "STSong", "Noto Sans SC", Georgia, serif;
--font-mono: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace;
```

- [ ] **Step 6: Run the focused test and verify it passes**

Run:

```bash
npm run test -- tests/startup-fonts-contract.test.ts
```

Expected: PASS.

- [ ] **Step 7: Run formatting and inspect asset size**

Run:

```bash
npm run format:check
du -h src/assets/fonts/*.woff2
```

Expected: `format:check` PASS. The `du` output shows only six Latin font files, not full CJK font bundles.

- [ ] **Step 8: Commit**

```bash
git add index.html src/styles/globals.css src/assets/fonts tests/startup-fonts-contract.test.ts
git commit -m "perf(ui): 移除启动字体外部网络依赖"
```

---

### Task 3: Deferred Streaming Markdown Rendering

**Files:**

- Modify: `src/components/ai/AiMessageBubble.tsx`
- Modify: `tests/assistant-streaming-lifecycle.test.tsx`
- Modify: `tests/ai-code-copy.test.tsx`

- [ ] **Step 1: Write the failing source contract**

Extend `tests/assistant-streaming-lifecycle.test.tsx` inside `describe("token batches are throttled", ...)`:

```ts
it("AiMessageBubble defers streamed markdown snapshots but renders final content immediately", () => {
  const src = read("src/components/ai/AiMessageBubble.tsx");

  expect(src).toContain("useDeferredValue");
  expect(src).toContain("const deferredRenderContent = useDeferredValue");
  expect(src).toContain(
    "const markdownContent = streaming ? deferredRenderContent : content",
  );
  expect(src).toContain("renderMarkdownWithProfile(");
  expect(src).toContain('markdownContent || ""');
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
npm run test -- tests/assistant-streaming-lifecycle.test.tsx
```

Expected: FAIL because `AiMessageBubble.tsx` does not import or use `useDeferredValue`.

- [ ] **Step 3: Update imports in `AiMessageBubble.tsx`**

Change the React import to include `useDeferredValue`:

```ts
import {
  useCallback,
  useDeferredValue,
  useMemo,
  memo,
  type MouseEvent,
  type ReactNode,
} from "react";
```

- [ ] **Step 4: Add deferred content selection**

Inside `AssistantBody`, replace:

```ts
const renderContent = useStreamingContent(content, streaming);
```

with:

```ts
const renderContent = useStreamingContent(content, streaming);
const deferredRenderContent = useDeferredValue(renderContent);
const markdownContent = streaming ? deferredRenderContent : content;
```

Then update the `useMemo` body so every render input uses `markdownContent`:

```ts
const html = useMemo(() => {
  try {
    const result = renderMarkdownWithProfile(
      markdownContent || "",
      "chat_assistant",
      {
        streaming,
      },
    );

    return result.output;
  } catch (err) {
    console.warn("[ai-message] Markdown render failed", {
      contentSummary: summarizeLogContent(markdownContent || ""),

      error:
        err instanceof Error
          ? { name: err.name, messageLength: err.message.length }
          : { name: typeof err, messageLength: String(err).length },
    });

    const escaped = (markdownContent || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\n/g, "<br>");

    return `<p class="text-muted-foreground whitespace-pre-wrap">${escaped}</p>`;
  }
}, [markdownContent, streaming]);
```

- [ ] **Step 5: Keep code rendering behavior covered**

Add this test to `tests/ai-code-copy.test.tsx`:

```ts
it("renders final assistant markdown content when streaming is false", async () => {
  await act(async () => {
    root.render(
      createElement(AiMessageBubble, {
        role: "assistant",
        content: "**final answer**",
        streaming: false,
      }),
    );
  });

  expect(host.querySelector("strong")?.textContent).toBe("final answer");
});
```

- [ ] **Step 6: Run focused tests and verify they pass**

Run:

```bash
npm run test -- tests/assistant-streaming-lifecycle.test.tsx tests/ai-code-copy.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Run lint and typecheck**

Run:

```bash
npm run lint
npm run typecheck
```

Expected: both PASS.

- [ ] **Step 8: Commit**

```bash
git add src/components/ai/AiMessageBubble.tsx tests/assistant-streaming-lifecycle.test.tsx tests/ai-code-copy.test.tsx
git commit -m "perf(ai): 降低流式 Markdown 渲染抢占"
```

---

### Task 4: Finalized Assistant Message Content Visibility

**Files:**

- Modify: `src/components/ai/AiMessageBubble.tsx`
- Modify: `src/styles/globals.css`
- Modify: `tests/assistant-stream-rendering-performance-contract.test.ts`

- [ ] **Step 1: Write the failing CSS/source contract**

Extend `tests/assistant-stream-rendering-performance-contract.test.ts`:

```ts
it("allows content-visibility only for finalized assistant bubbles", () => {
  const css = read("src/styles/globals.css");
  const finalizedRule =
    css.split(".ai-message-bubble-assistant:not([data-streaming])")[1] ?? "";
  const streamingRule =
    css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";

  expect(finalizedRule).toContain("content-visibility: auto");
  expect(finalizedRule).toContain("contain-intrinsic-size");
  expect(streamingRule).not.toContain("content-visibility: auto");
});

it("assistant bubbles expose stable data attributes for finalized and streaming states", () => {
  const src = read("src/components/ai/AiMessageBubble.tsx");

  expect(src).toContain("data-role={role}");
  expect(src).toContain('data-streaming={streaming ? "" : undefined}');
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
npm run test -- tests/assistant-stream-rendering-performance-contract.test.ts
```

Expected: FAIL because finalized bubble CSS and data attribute contract are not present yet.

- [ ] **Step 3: Add stable data attributes to `AiMessageBubble`**

In the outer bubble element in `AiMessageBubble.tsx`, ensure it has:

```tsx
data-role={role}
data-streaming={streaming ? "" : undefined}
```

Keep the existing role-specific classes. Do not remove `ai-message-bubble-streaming[data-streaming]`.

- [ ] **Step 4: Add finalized assistant CSS rule**

In `src/styles/globals.css`, near the existing `.ai-message-bubble-streaming[data-streaming]` rule, add:

```css
.ai-message-bubble-assistant:not([data-streaming]) {
  content-visibility: auto;
  contain-intrinsic-size: auto 320px;
}
```

Keep the existing streaming rule:

```css
.ai-message-bubble-streaming[data-streaming] {
  contain: layout paint style;
}
```

Do not add `content-visibility: auto` to `.ai-message-bubble-streaming[data-streaming]`.

- [ ] **Step 5: Run the focused test and verify it passes**

Run:

```bash
npm run test -- tests/assistant-stream-rendering-performance-contract.test.ts
```

Expected: PASS.

- [ ] **Step 6: Run the AI scroll performance tests**

Run:

```bash
npm run test -- tests/ai-message-list-scroll-perf.test.ts tests/assistant-streaming-lifecycle.test.tsx
```

Expected: PASS. If virtualized scroll source contracts fail, remove the finalized `content-visibility` rule and keep only the data attributes.

- [ ] **Step 7: Commit**

```bash
git add src/components/ai/AiMessageBubble.tsx src/styles/globals.css tests/assistant-stream-rendering-performance-contract.test.ts
git commit -m "perf(ai): 为已完成消息启用离屏渲染跳过"
```

---

### Task 5: Editor Surface Cache Boundary Audit

**Files:**

- Modify: `tests/editor-html-cache.test.ts`
- Modify: `docs/ops/performance-guide.md`
- Inspect: `src/components/layout/AppEditorWorkspace.tsx`

- [ ] **Step 1: Write the cache boundary contract**

Add this test to `tests/editor-html-cache.test.ts`:

```ts
it("does not introduce a second editor view cache or WeakRef editor pool", () => {
  const workspace = readSource("src/components/layout/AppEditorWorkspace.tsx");

  expect(workspace).toContain("surfaceRecords");
  expect(workspace).toContain("READY_SURFACE_RETAIN_LIMIT");
  expect(workspace).toContain("data-editor-visibility");
  expect(workspace).not.toContain("new WeakRef");
  expect(workspace).not.toContain("EditorViewCache");
});
```

- [ ] **Step 2: Run the focused test and verify it passes**

Run:

```bash
npm run test -- tests/editor-html-cache.test.ts
```

Expected: PASS. This is a guardrail task; it should pass against current code.

- [ ] **Step 3: Add manual surface-cache measurement guidance**

Append this subsection under `Document Open Runtime` in `docs/ops/performance-guide.md`:

```md
### Editor Surface Cache Check

Use this check before changing `AppEditorWorkspace` caching behavior.

1. Open five Markdown notes with different sizes.
2. Switch between the five tabs twice.
3. Confirm already opened tabs do not show the cold-open loading surface again.
4. Record whether `data-editor-visibility="hidden"` surfaces remain bounded by `READY_SURFACE_RETAIN_LIMIT`.
5. Confirm hidden editor surfaces do not trigger repeated expensive outline, stats, or ingest work while inactive.

If a regression is observed, prefer a small adjustment inside `AppEditorWorkspace` before introducing a new cache owner. Do not add `EditorViewCache.tsx` or a WeakRef editor pool.
```

- [ ] **Step 4: Run documentation and cache tests**

Run:

```bash
npm run test -- tests/editor-html-cache.test.ts tests/performance-guide-contract.test.ts
npm run format:check
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/editor-html-cache.test.ts docs/ops/performance-guide.md
git commit -m "test(editor): 锁定现有编辑器 surface 缓存边界"
```

---

### Task 6: Markdown Render Worker Core and Protocol

**Files:**

- Create: `src/lib/markdown-render-worker-core.ts`
- Create: `src/workers/markdown-render.worker.ts`
- Create: `tests/markdown-render-worker-core.test.ts`
- Create: `tests/markdown-render-worker-source-contract.test.ts`

- [ ] **Step 1: Write failing core equivalence tests**

Create `tests/markdown-render-worker-core.test.ts`:

````ts
import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";
import { renderMarkdownForWorker } from "@/lib/markdown-render-worker-core";

const fixtures = [
  "Hello **world**",
  "See [citation:1].",
  "```ts\nconst x = 1;\n```",
  "| A | B |\n| --- | --- |\n| 1 | 2 |",
  "**partial",
];

describe("markdown render worker core", () => {
  it("matches chat_assistant sync output for core fixtures", () => {
    for (const content of fixtures) {
      const streaming = content === "**partial";
      const sync = renderMarkdownWithProfile(content, "chat_assistant", {
        streaming,
      });
      const worker = renderMarkdownForWorker({
        id: 1,
        profile: "chat_assistant",
        content,
        streaming,
        type: "render",
      });

      expect(worker.type).toBe("rendered");
      if (worker.type === "rendered") {
        expect(worker.html).toBe(sync.output);
        expect(worker.renderedLength).toBe(content.length);
      }
    }
  });

  it("returns a stable hash for identical content and different hash for changed content", () => {
    const first = renderMarkdownForWorker({
      id: 1,
      profile: "chat_assistant",
      content: "**same**",
      streaming: true,
      type: "render",
    });
    const second = renderMarkdownForWorker({
      id: 2,
      profile: "chat_assistant",
      content: "**same**",
      streaming: true,
      type: "render",
    });
    const changed = renderMarkdownForWorker({
      id: 3,
      profile: "chat_assistant",
      content: "**changed**",
      streaming: true,
      type: "render",
    });

    expect(first.type).toBe("rendered");
    expect(second.type).toBe("rendered");
    expect(changed.type).toBe("rendered");
    if (
      first.type === "rendered" &&
      second.type === "rendered" &&
      changed.type === "rendered"
    ) {
      expect(second.contentHash).toBe(first.contentHash);
      expect(changed.contentHash).not.toBe(first.contentHash);
    }
  });
});
````

- [ ] **Step 2: Write failing source contract for the browser Worker**

Create `tests/markdown-render-worker-source-contract.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("markdown render worker source contract", () => {
  it("worker delegates rendering to the shared markdown contract core", () => {
    const worker = read("src/workers/markdown-render.worker.ts");

    expect(worker).toContain("renderMarkdownForWorker");
    expect(worker).not.toContain("marked.parse");
    expect(worker).not.toContain("replace(/<script");
  });

  it("worker skips duplicate content and honors abort messages", () => {
    const worker = read("src/workers/markdown-render.worker.ts");

    expect(worker).toContain("lastRenderedHash");
    expect(worker).toContain('type === "abort"');
    expect(worker).toContain('reason: "duplicate"');
    expect(worker).toContain('reason: "aborted"');
  });
});
```

- [ ] **Step 3: Run focused tests and verify they fail**

Run:

```bash
npm run test -- tests/markdown-render-worker-core.test.ts tests/markdown-render-worker-source-contract.test.ts
```

Expected: FAIL because the new core and worker files do not exist.

- [ ] **Step 4: Implement `src/lib/markdown-render-worker-core.ts`**

Create:

```ts
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";

export interface MarkdownRenderRequest {
  type: "render";
  id: number;
  profile: "chat_assistant";
  content: string;
  streaming: boolean;
}

export interface MarkdownAbortRequest {
  type: "abort";
  id: number;
}

export type MarkdownRenderWorkerRequest =
  | MarkdownRenderRequest
  | MarkdownAbortRequest;

export type MarkdownRenderWorkerResponse =
  | {
      type: "rendered";
      id: number;
      html: string;
      contentHash: string;
      renderedLength: number;
    }
  | {
      type: "skipped";
      id: number;
      reason: "duplicate" | "aborted";
    }
  | {
      type: "error";
      id: number;
      message: string;
    };

export function markdownContentHash(content: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < content.length; i += 1) {
    hash ^= content.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function renderMarkdownForWorker(
  request: MarkdownRenderRequest,
): MarkdownRenderWorkerResponse {
  try {
    const result = renderMarkdownWithProfile(request.content, request.profile, {
      streaming: request.streaming,
    });
    return {
      type: "rendered",
      id: request.id,
      html: result.output,
      contentHash: markdownContentHash(request.content),
      renderedLength: request.content.length,
    };
  } catch (error: unknown) {
    return {
      type: "error",
      id: request.id,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}
```

- [ ] **Step 5: Implement `src/workers/markdown-render.worker.ts`**

Create:

```ts
import {
  markdownContentHash,
  renderMarkdownForWorker,
  type MarkdownRenderWorkerRequest,
  type MarkdownRenderWorkerResponse,
} from "@/lib/markdown-render-worker-core";

let lastRenderedHash: string | null = null;
const abortedIds = new Set<number>();

function post(response: MarkdownRenderWorkerResponse): void {
  self.postMessage(response);
}

self.onmessage = (event: MessageEvent<MarkdownRenderWorkerRequest>) => {
  const request = event.data;

  if (request.type === "abort") {
    abortedIds.add(request.id);
    post({ type: "skipped", id: request.id, reason: "aborted" });
    return;
  }

  if (abortedIds.has(request.id)) {
    post({ type: "skipped", id: request.id, reason: "aborted" });
    return;
  }

  const contentHash = markdownContentHash(request.content);
  if (contentHash === lastRenderedHash) {
    post({ type: "skipped", id: request.id, reason: "duplicate" });
    return;
  }

  const response = renderMarkdownForWorker(request);
  if (abortedIds.has(request.id)) {
    post({ type: "skipped", id: request.id, reason: "aborted" });
    return;
  }

  if (response.type === "rendered") {
    lastRenderedHash = response.contentHash;
  }
  post(response);
};
```

- [ ] **Step 6: Run focused tests and verify they pass**

Run:

```bash
npm run test -- tests/markdown-render-worker-core.test.ts tests/markdown-render-worker-source-contract.test.ts
```

Expected: PASS.

- [ ] **Step 7: Run typecheck**

Run:

```bash
npm run typecheck
```

Expected: PASS. If the worker global `self` type fails, add `/// <reference lib="webworker" />` to the first line of `src/workers/markdown-render.worker.ts`.

- [ ] **Step 8: Commit**

```bash
git add src/lib/markdown-render-worker-core.ts src/workers/markdown-render.worker.ts tests/markdown-render-worker-core.test.ts tests/markdown-render-worker-source-contract.test.ts
git commit -m "perf(ai): 增加流式 Markdown Worker 渲染核心"
```

---

### Task 7: Integrate Markdown Worker With Assistant Bubble

**Files:**

- Create: `src/hooks/useMarkdownRenderWorker.ts`
- Modify: `src/components/ai/AiMessageBubble.tsx`
- Modify: `tests/assistant-streaming-lifecycle.test.tsx`
- Create: `tests/use-markdown-render-worker-source-contract.test.ts`

- [ ] **Step 1: Write failing source contracts**

Add to `tests/assistant-streaming-lifecycle.test.tsx`:

```ts
it("AiMessageBubble uses the markdown render worker only for streaming assistant content", () => {
  const src = read("src/components/ai/AiMessageBubble.tsx");

  expect(src).toContain("useMarkdownRenderWorker");
  expect(src).toContain("workerRender = useMarkdownRenderWorker");
  expect(src).toContain("enabled: streaming");
  expect(src).toContain("workerRender.html");
  expect(src).toContain("workerRender.failed");
});
```

Create `tests/use-markdown-render-worker-source-contract.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("useMarkdownRenderWorker source contract", () => {
  it("owns only worker lifecycle and async render results", () => {
    const src = read("src/hooks/useMarkdownRenderWorker.ts");

    expect(src).toContain("new Worker");
    expect(src).toContain("markdown-render.worker.ts");
    expect(src).toContain("postMessage");
    expect(src).toContain("terminate");
    expect(src).not.toContain("useStreamingContent");
  });

  it("keeps previous html while worker render is pending", () => {
    const src = read("src/hooks/useMarkdownRenderWorker.ts");

    expect(src).toContain("lastHtmlRef");
    expect(src).toContain("setState");
    expect(src).toContain("pending: true");
  });
});
```

- [ ] **Step 2: Run focused tests and verify they fail**

Run:

```bash
npm run test -- tests/assistant-streaming-lifecycle.test.tsx tests/use-markdown-render-worker-source-contract.test.ts
```

Expected: FAIL because the hook does not exist and `AiMessageBubble` is not integrated.

- [ ] **Step 3: Implement `src/hooks/useMarkdownRenderWorker.ts`**

Create:

```ts
import { useEffect, useRef, useState } from "react";

import type {
  MarkdownRenderWorkerRequest,
  MarkdownRenderWorkerResponse,
} from "@/lib/markdown-render-worker-core";

interface UseMarkdownRenderWorkerOptions {
  content: string;
  enabled: boolean;
  streaming: boolean;
}

interface MarkdownWorkerState {
  failed: boolean;
  html: string | null;
  pending: boolean;
}

function createMarkdownRenderWorker(): Worker {
  return new Worker(
    new URL("../workers/markdown-render.worker.ts", import.meta.url),
    { type: "module" },
  );
}

export function useMarkdownRenderWorker({
  content,
  enabled,
  streaming,
}: UseMarkdownRenderWorkerOptions): MarkdownWorkerState {
  const workerRef = useRef<Worker | null>(null);
  const requestIdRef = useRef(0);
  const lastHtmlRef = useRef<string | null>(null);
  const [state, setState] = useState<MarkdownWorkerState>({
    failed: false,
    html: null,
    pending: false,
  });

  useEffect(() => {
    if (!enabled || !streaming || typeof Worker === "undefined") {
      workerRef.current?.terminate();
      workerRef.current = null;
      setState({
        failed: false,
        html: null,
        pending: false,
      });
      return;
    }

    if (!workerRef.current) {
      workerRef.current = createMarkdownRenderWorker();
    }

    const worker = workerRef.current;
    const id = requestIdRef.current + 1;
    requestIdRef.current = id;
    setState((prev) => ({
      failed: false,
      html: prev.html ?? lastHtmlRef.current,
      pending: true,
    }));

    worker.onmessage = (event: MessageEvent<MarkdownRenderWorkerResponse>) => {
      const response = event.data;
      if (response.id !== requestIdRef.current) return;

      if (response.type === "rendered") {
        lastHtmlRef.current = response.html;
        setState({
          failed: false,
          html: response.html,
          pending: false,
        });
        return;
      }

      if (response.type === "error") {
        setState({
          failed: true,
          html: lastHtmlRef.current,
          pending: false,
        });
        return;
      }

      setState((prev) => ({
        failed: false,
        html: prev.html ?? lastHtmlRef.current,
        pending: false,
      }));
    };

    worker.onerror = () => {
      setState({
        failed: true,
        html: lastHtmlRef.current,
        pending: false,
      });
    };

    const request: MarkdownRenderWorkerRequest = {
      type: "render",
      id,
      profile: "chat_assistant",
      content,
      streaming,
    };
    worker.postMessage(request);

    return () => {
      worker.postMessage({ type: "abort", id });
    };
  }, [content, enabled, streaming]);

  useEffect(() => {
    return () => {
      workerRef.current?.terminate();
      workerRef.current = null;
    };
  }, []);

  return state;
}
```

- [ ] **Step 4: Integrate the hook in `AiMessageBubble.tsx`**

Import the hook:

```ts
import { useMarkdownRenderWorker } from "@/hooks/useMarkdownRenderWorker";
```

Inside `AssistantBody`, after `markdownContent`:

```ts
const workerRender = useMarkdownRenderWorker({
  content: markdownContent,
  enabled: streaming,
  streaming,
});
```

Update the HTML `useMemo` so streaming can use Worker output and avoid sync render while Worker is healthy:

```ts
const html = useMemo(() => {
  if (streaming && !workerRender.failed) {
    return workerRender.html ?? "";
  }

  try {
    const result = renderMarkdownWithProfile(
      markdownContent || "",
      "chat_assistant",
      {
        streaming,
      },
    );

    return result.output;
  } catch (err) {
    console.warn("[ai-message] Markdown render failed", {
      contentSummary: summarizeLogContent(markdownContent || ""),

      error:
        err instanceof Error
          ? { name: err.name, messageLength: err.message.length }
          : { name: typeof err, messageLength: String(err).length },
    });

    const escaped = (markdownContent || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\n/g, "<br>");

    return `<p class="text-muted-foreground whitespace-pre-wrap">${escaped}</p>`;
  }
}, [markdownContent, streaming, workerRender.failed, workerRender.html]);
```

Keep `streaming=false` fully synchronous so the final answer renders immediately.

- [ ] **Step 5: Run focused tests and verify they pass**

Run:

```bash
npm run test -- tests/assistant-streaming-lifecycle.test.tsx tests/use-markdown-render-worker-source-contract.test.ts tests/ai-code-copy.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Run worker and markdown contract tests**

Run:

```bash
npm run test -- tests/markdown-render-worker-core.test.ts tests/markdown-render-worker-source-contract.test.ts tests/markdown-contract/ai-panel-orchestration.test.ts
```

Expected: PASS.

- [ ] **Step 7: Run lint, typecheck, and focused AI performance contracts**

Run:

```bash
npm run lint
npm run typecheck
npm run test -- tests/assistant-panel-performance-contract.test.ts tests/assistant-stream-rendering-performance-contract.test.ts tests/ai-message-list-scroll-perf.test.ts
```

Expected: all PASS.

- [ ] **Step 8: Commit**

```bash
git add src/hooks/useMarkdownRenderWorker.ts src/components/ai/AiMessageBubble.tsx tests/assistant-streaming-lifecycle.test.tsx tests/use-markdown-render-worker-source-contract.test.ts
git commit -m "perf(ai): 接入流式 Markdown Worker 渲染"
```

---

### Task 8: Final Verification and Manual Performance Pass

**Files:**

- Modify: `docs/ops/performance-guide.md` only if the manual run records a concise summary.

- [ ] **Step 1: Run all required static checks**

Run:

```bash
npm run format:check
npm run lint
npm run typecheck
```

Expected: all PASS.

- [ ] **Step 2: Run all frontend tests**

Run:

```bash
npm run test
```

Expected: all test files PASS.

- [ ] **Step 3: Run Rust checks only if Rust files changed**

If no `src-tauri/**` files changed, record in the final implementation note:

```txt
Rust checks not run because this plan did not modify Rust files.
```

If Rust files changed during execution, run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: all PASS.

- [ ] **Step 4: Run manual cold-start font check**

Run:

```bash
npm run dev:desktop
```

Then open the app in the Tauri/dev browser context, use DevTools Network, and verify:

- No `fonts.googleapis.com` request.
- No `fonts.gstatic.com` request.
- Local `.woff2` files load from `/src/assets/fonts/`.

Stop the dev server after recording results.

- [ ] **Step 5: Run manual AI streaming performance check**

Use DevTools Performance while sending a long assistant prompt that produces:

- headings,
- normal paragraphs,
- a code block,
- a Markdown table,
- at least one citation-like marker such as `[citation:1]`.

Record in `docs/ops/performance-guide.md` or the PR description:

```txt
AI streaming baseline:
- Scenario:
- Build mode:
- >50ms long tasks before:
- >50ms long tasks after:
- Main-thread Markdown parse visible during stream: yes/no
- User scroll/click during stream responsive: yes/no
```

- [ ] **Step 6: Run manual tab surface check**

Open five Markdown files, switch between them twice, and record:

```txt
Editor surface cache check:
- Open tab count:
- Repeated cold loading surface on tab return: yes/no
- Unexpected reingest on hot tab return: yes/no
- Visible stale editor content after switch: yes/no
```

- [ ] **Step 7: Final git review**

Run:

```bash
git status --short
git diff --check
git diff --stat
```

Expected:

- No whitespace errors.
- Only files from this plan changed.
- No user note content, traces, credentials, or large accidental artifacts staged.

- [ ] **Step 8: Final commit if manual notes changed**

If only `docs/ops/performance-guide.md` changed after manual checks:

```bash
git add docs/ops/performance-guide.md
git commit -m "docs(ui): 记录性能优化验证口径"
```

If no files changed after manual checks, do not create an empty commit.

---

## Plan Self-Review

### Spec Coverage

- Cold-start external font dependency: Task 2.
- Baseline and verification discipline: Task 1 and Task 8.
- Streaming deferred rendering: Task 3.
- Finalized message content visibility: Task 4.
- Existing editor surface cache boundary: Task 5.
- Worker rendering through markdown contract: Task 6 and Task 7.
- Privacy and data principles: Task 1 and Task 8.

### Intentional Deferrals

- Full CJK font self-hosting is not implemented in this plan. The design spec rejects bundling full CJK fonts by default because of package size risk.
- `useTabManager` runtime splitting is not implemented in this plan. The design requires Profiler evidence before changing state ownership. Task 5 preserves the current cache boundary and Task 8 records manual evidence.
- No Rust changes are planned. Rust checks are required only if execution touches `src-tauri/**`.

### Execution Notes

- Do not create a worktree unless the user explicitly approves one.
- Do not store DevTools trace files containing note content in git.
- Do not introduce `EditorViewCache.tsx` or WeakRef editor pools.
- Do not bypass `renderMarkdownWithProfile` in Worker code.
- Keep commit messages in Chinese Conventional Commit format.
