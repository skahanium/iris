# 架构审计报告 — 编辑器交互缺陷深度分析

**日期**: 2026-07-06  
**范围**: TipTap 编辑器与文档生命周期管理  
**关注**: 锁按钮、新建文档、粘贴、光标跳跃四大交互缺陷的根本病因

---

## 目录

1. [锁按钮闪烁 & 文档内容丢失](#一锁按钮闪烁--文档内容丢失)
2. [新建空白文档闪烁加载页](#二新建空白文档闪烁加载页)
3. [粘贴内容到空文档第一次失败](#三粘贴内容到空文档第一次失败)
4. [光标莫名其妙跳到第一行](#四光标莫名其妙跳到第一行)
5. [跨 Bug 的共享架构缺陷](#五跨-bug-的共享架构缺陷)
6. [建议修复方案](#六建议修复方案)

---

## 一、锁按钮闪烁 & 文档内容丢失

### 触发链路

```
用户点击锁按钮
  → TipTapEditor onClick → setLocked(!locked)
  → AppEditorWorkspace → handleLockToggle(true)
  → App.impl.tsx:464 handleLockToggle
    → await flushSave()           (序列化编辑器 → 写盘 → fire onSaved)
    → setFileLocked(path, true)    (更新 UI 状态)
    → await fileSetLock()          (持久化到 SQLite)
    → invalidatePreparedNote()     (清除预热缓存)
```

### 根本病因（三层）

**第一层：首次保存的无条件回调** (`src/hooks/useEditorSave.ts:63-78`)

`lastSavedSnapshotRef` 初始值为 `null`。任何文档的首次 `flushSave` 都会**无条件**触发 `onSaved` 回调，即使编辑器内容零变化、磁盘文件也未修改：

```ts
// useEditorSave.ts
const runSaveOnce = async () => {
  const md = getMarkdownRef.current();
  const last = lastSavedSnapshotRef.current;
  if (last && last.path === target && last.markdown === md) {
    return last.markdown;  // <-- 首次调用 last 为 null，跳过此短路
  }
  const saved = await writeNoteAtPath(target, md);
  if (saved) {
    recordSavedSnapshot(target, saved);
    onSavedRef.current?.(saved);  // <-- 无条件触发全状态级联
  }
  return saved;
};
```

`onSaved` 回调在 `useAppPersistenceLifecycle.ts:93-106`：

```ts
(md) => {
  applySavedMarkdown(md);          // 重置 markdownRef / frontmatterRef
  dirtyRef.current = false;
  setMarkdown(md);                 // React state 更新 → 全树重渲染
  syncTabMarkdownCache(path, md);
  markClean(path, ...);            // 更新 tabs state
}
```

**第二层：双重 state 更新引发级联渲染**

`handleLockToggle` 是 async 函数，状态更新在 `await` 两侧分两次发生：

```
时间线:
  1. await flushSave()
       → onSaved → setMarkdown(md)
       → React 重新渲染 #1 (activeFileLocked 仍为 false)
       → editorTitleSlot 重新创建（JSX 引用变化）
       → TipTapEditor 的 memo 检查：titleSlot 新引用 → 失效 → 全面重渲染

  2. 回到 handleLockToggle
       → setFileLocked(true)
       → React 重新渲染 #2 (activeFileLocked 变为 true)
       → useEffect([editor, locked]) → editor.setEditable(false)
       → 按钮图标从 LockOpen 切换到 Lock
```

第一次渲染中 `TipTapEditor` 被 `React.memo` 包裹，但 **`titleSlot`** 来源是 `useMemo` (`App.impl.tsx:585-595`)，其依赖包含 `activeFileLocked`。虽然 `activeFileLocked` 此时未变，但整个 JSX element 是新引用 → memo 比较失败 → 组件重渲染。用户在此窗口看到的内容可能和锁之后的 UI 状态不一致。

**第三层：内容丢失 —— 序列化的 "幽灵" 覆盖**

当 `flushSave()` 的序列化结果与编辑器当前真实状态不一致时，`onSaved` → `applySavedMarkdown` → 后续异步链路可能触发 `loadBodyIntoEditor` (`useOpenNote.ts:229`)，它在 `dirtyRef.current === false` 时会异步执行：

```ts
// useOpenNote.ts:229-263
const loadBodyIntoEditor = (content) => {
  void ingestMarkdownForEditorAsync({ bodyMarkdown: parsed.bodyMd })
    .then(({ tipTapHtml }) => {
      if (dirtyRef?.current) return;     // dirty 时跳过
      resetEditorContentBaseline(editor, tipTapHtml, {
        parseOptions: EDITOR_PARSE_OPTIONS,
      });  // <-- 整个文档被替换，光标归零
    })
};
```

如果 `getLiveMarkdown` 在序列化时读到中间状态，导致写盘内容不同于编辑器内容，然后 `onSaved` 触发的 `applySavedMarkdown` 更新了 `markdownRef`，下一次自动保存或状态同步时，`loadBodyIntoEditor` 会用**写盘后读回**的内容**异步覆盖**编辑器——用户看到的就是 "内容没了/变成了旧版本"。

---

## 二、新建空白文档闪烁加载页

### 触发链路

```
handleNewNote (useTabManager.ts:673)
  → cancelOpenTransaction()
  → persistAndCacheTab(previous)        (保存当前标签)
  → createDefaultNote()                 (IPC → 后端创建 .md 文件)
  → prepareNoteOpenFromContent()         (同步解析 Markdown)
  → openFile()                          (打开文件流程)
      → stagePendingNoteOpen()           (设置 pendingNoteOpen state)
      → AppEditorWorkspace 检测 pendingNoteOpen
        → currentEditorSurface 建立
        → 编辑器挂载 (TipTapEditor)
        → 解析 content → parsedContentRevision bump → resetEditorContentBaseline
        → 2x RAF 后 → onFirstFrameReady → handleSurfaceFirstFrameReady
        → commitPendingNoteOpen → applyCommittedNoteOpen
        → editorContentTick bump → reingestKey 变化 → 二次内容重置
```

### 根本病因（两层）

**第一层：加载门的虚假正报** (`src/components/layout/AppEditorWorkspace.tsx:577-671`)

当 `pendingNoteOpen` 被设置后，surface record 的 `ready` 为 `false`。加载门通过定时器控制 visibility：

```ts
const scheduleDocumentLoadingVisibility = (identityKey, startedAt) => {
  const elapsed = performance.now() - startedAt;
  const remaining = DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs - elapsed;
  // coldLoadingVisibleMs = 100ms

  if (remaining <= 0) {
    syncDocumentLoadingGate({ ...hiddenGate, visible: true });
    return;
  }

  syncDocumentLoadingGate(hiddenGate);           // 先设为不可见
  loadingVisibilityTimerRef.current = setTimeout(() => {
    // 100ms 后设为可见
    syncDocumentLoadingGate({ identityKey, shownAt: startedAt, visible: true });
  }, remaining);
};
```

对于空白文档（`<p></p>`），整个渲染链路（挂载 → ingest → reset → 渲染）在 < 16ms 内完成，`onFirstFrameReady` 在 ~32ms 触发并清除定时器。但如果有 IPC 延迟（`createDefaultNote` 的 `fileCreate` 走 Rust 后端），从点击新建到 `stagePendingNoteOpen` 之间可能已有较大延迟，使得 `remaining` 等于 0 或非常小，**skeleton 照常显示**。

**第二层：`commitPendingNoteOpen` 中的 `editorContentTick` bump 导致二次渲染** (`useTabManager.ts:301`)

```ts
const applyCommittedNoteOpen = (pending, discardedPreviousPath, skipTickBump) => {
  // ...
  if (!skipTickBump) {
    setEditorContentTick((tick) => tick + 1);  // <-- bump
  }
};
```

`commitPendingNoteOpen` 调用时 `skipTickBump` 默认为 `false`。`editorContentTick` bump 后 → `reingestKey` 变化 → `skipHtmlCache = true` → 内容重新解析 → `parsedContentRevision` bump → `resetEditorContentBaseline` 再次调用。编辑器内容在 < 100ms 内被**完全替换两次**。虽然在空白文档中内容都是 `<p></p>`，但两次替换中间存在一帧的 "过渡" 视觉状态。

---

## 三、粘贴内容到空文档第一次失败

### 触发链路

```
pasteIntoEditor (src/lib/iris-clipboard.ts:133)
  → readClipboardText()
  → editor.chain().focus().insertContent(...)
```

### 根本病因（两层）

**第一层：编辑器 "staging" 窗口期**

从编辑器 DOM 挂载到 `onFirstFrameReady` 之间有一段时间窗口（`TipTapEditor.tsx:598-631`）：

```
0ms:      parsedContentRef 设置
~0ms:     resetEditorContentBaseline 首次执行
~0ms:     onContentReady → contentReady = true（但 ready 仍为 false）
~16ms:    RAF #1
~32ms:    RAF #2 → onFirstFrameReady → ready = true
```

在 `ready` 变为 `true` 之前：
- CSS: `pointer-events: none` + `opacity-0`（`AppEditorWorkspace.tsx:710-713`）
- `data-editor-visibility="staging"`
- ProseMirror view 已存在但处于不可交互状态

如果用户在此时使用 **Ctrl+V 快捷键**，浏览器原生行为可能尝试 paste 到 ProseMirror DOM，但由于 `pointer-events: none`，焦点转移被阻止。ProseMirror 的 paste 处理可能被跳过。

**第二层：二次内容重置覆盖粘贴内容**

粘贴发生在编辑器 ready 后。但 `commitPendingNoteOpen` 的 `editorContentTick` bump 约在 32ms 时触发。如果粘贴恰好在 bump **之后** 但在 `resetEditorContentBaseline` 执行**之前**（按 React batch 的时间窗口），粘贴进来的内容会被随后的 `resetEditorContentBaseline` **整个覆盖掉**。

如果是粘贴发生在 bump **之后**，内容已稳定，则第二次粘贴成功——这就是用户观察到的 "需要粘贴第二次"。

### 时间线可视化

```
0ms:   stagePendingNoteOpen → 编辑器挂载（opacity-0）
0ms:   resetEditorContentBaseline #1（prepared HTML）
16ms:  RAF #1
32ms:  RAF #2 → onFirstFrameReady
       → commitPendingNoteOpen
       → editorContentTick bump
40ms:  reingestKey 变化 → content 重新解析
45ms:  resetEditorContentBaseline #2（二次重置）
       ← 45ms 之前的所有 paste 内容都可能被覆盖
50ms:  编辑器真正稳定，可交互
```

---

## 四、光标莫名其妙跳到第一行

### 触发链路（多条路径）

```
路径 A: editorContentTick bump
  → TipTapEditor reingestKey 变化
  → skipHtmlCache = true
  → 解析 content → parsedContentRevision bump
  → Content Application Effect (TipTapEditor.tsx:594-631)
  → resetEditorContentBaseline(editor, content)
  → selection: TextSelection.atStart(doc)  ← 强制归零

路径 B: 自动保存 round-trip (1200ms debounce)
  → flushSave → onSaved → setMarkdown
  → 级联效应 → loadBodyIntoEditor (dirty=false 时)
  → resetEditorContentBaseline(editor, tipTapHtml)
  → selection: TextSelection.atStart(doc)  ← 强制归零

路径 C: 文件系统监听触发的重读
  → currentFileChangeListener → invalidatePreparedNote
  → 触发 openFile → editorContentTick bump → 同路径 A
```

### 根本病因

**`resetEditorContentBaseline` 唯一 "set content" 方式，却自带破坏性** (`src/lib/editor-baseline.ts:17-38`)

```ts
export function resetEditorContentBaseline(editor, content, options) {
  const doc = createDocument(content, editor.schema, options.parseOptions);
  const nextState = EditorState.create({
    doc,
    plugins: editor.state.plugins,
    schema: editor.schema,
    selection: TextSelection.atStart(doc),  // <-- 光标无条件归零
  });
  editor.view.updateState(nextState);
}
```

`TextSelection.atStart(doc)` 将选择强制设为文档开头。这个函数的设计意图是 "替换整个文档基线"，因此**有意丢弃当前选择状态**。问题在于：

| 调用处 | 真实意图 | 是否应保留选择 |
|---|---|---|
| `TipTapEditor.tsx:611` | 首次设置（文档未打开）| 不需要（适当行为）|
| `TipTapEditor.tsx:611` | reingest 触发的二次替换（文档已打开且互动中）| **需要！** |
| `useOpenNote.ts:244` | save 回调中的异步重置（用户可能正在编辑）| **需要！** |
| `useOpenNote.ts:255` | ingest 失败的 fallback | **需要！** |

**典型脉冲式跳跃场景：**

"刚粘贴内容到空文档、准备在某处打字，一敲键盘光标就跳到标题/第一行"

```
时间线:
  0s:   用户粘贴内容（成功写入 editor）
  0s:   notifyDirty() → debouncedSave 定时 1200ms
  1.2s: debouncedSave 执行 → flushSave
          → getLiveMarkdown 序列化编辑器
          → writeNoteAtPath → onSaved(md)
          → setMarkdown(md)      ← React state 变化
          → applySavedMarkdown(md)
          → markClean(path, title)
          → ...某条异步链触发 loadBodyIntoEditor...
  1.3s: loadBodyIntoEditor 完成
          → resetEditorContentBaseline
          → TextSelection.atStart(doc)
          → 光标从用户当前编辑位置 → 文档开头
  用户正在打字 → 突然光标跳到标题 ← 完全不可预期
```

---

## 五、跨 Bug 的共享架构缺陷

上述四个缺陷共享两个深层架构问题：

### 缺陷 A：`editorContentTick` bump 被滥用为 "force re-ingest" 信号

**位置**：`useTabManager.ts:301` — `applyCommittedNoteOpen`

```ts
if (!skipTickBump) {
  setEditorContentTick((tick) => tick + 1);
}
```

`editorContentTick` 的设计意图：区分 "磁盘加载" 和 "编辑器保存"。但它在**每次文档提交打开**时无条件 bump，驱动：

1. `TipTapEditor.reingestKey` 变化 → `skipHtmlCache = true` → 缓存失效
2. 内容重新解析 → `parsedContentRevision` bump
3. `resetEditorContentBaseline` 调用 → 全文档替换 + 光标归零

**影响面**：每次打**任何**文档（新建、已有、切换标签），编辑器都会经历 **两次** 内容替换（一次 from prepared HTML，一次 from re-ingest 响应 tick bump）。第一次 is correct, 第二次 is harmful.

**解决方向**：将 `editorContentTick` 拆分为两个独立信号：
- `diskLoadVersion` — 标记磁盘加载（驱动前端重新读取）
- `editorReingestSignal` — 按需触发（仅当 prepared HTML 不可用或内容确实变更时）

### 缺陷 B：`onSaved` 回调在内容零变化时仍触发全量 side effect

**位置**：`useEditorSave.ts:63-78` + `useAppPersistenceLifecycle.ts:93-106`

首次保存时 `lastSavedSnapshotRef` 为 `null` → 短路条件不满足 → `onSaved` 无条件执行 → `applySavedMarkdown` + `setMarkdown` + `markClean` + `syncTabMarkdownCache`。

这对于"打开文档后立即点锁定"这类零编辑场景完全多余，且造成了 bug #1 中的级联渲染。

**解决方向**：
- `flushSave` 增加 "content unchanged from initial" 的短路（缓存首次读取时的 markdown，与保存时比较）
- 或者在 `handleLockToggle` 中增加脏检查：只有 `dirtyRef.current === true` 时才调用 `flushSave`

### 缺陷 C：`resetEditorContentBaseline` 缺少选择保留机制

**位置**：`src/lib/editor-baseline.ts:17-38`

该函数作为 "set content" 的唯一方式被 4 处代码调用，但每次都强制 `TextSelection.atStart(doc)`。缺一个 `preserveSelection` 或 `targetSelection` 参数来控制何时这种行为是无害的、何时是破坏性的。

### 缺陷 D：加载门固定延迟与快速渲染的竞态

**位置**：`src/components/layout/AppEditorWorkspace.tsx:364-395`

`DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs = 100ms` 是固定值，但编辑器实际就绪时间可以从 < 16ms（空文档）到 > 5000ms（大文档 + worker ingest）。固定延迟意味着：
- 对空文档：不必要地显示 skeleton
- 对大文档：可能过早显示 skeleton 然后等很久

---

## 六、建议修复方案

### 按优先级排序

| 优先级 | 问题 | 修复方向 | 影响面 |
|--------|------|---------|--------|
| **P0** | 光标跳跃（Bug #4）| `resetEditorContentBaseline` 增加 `targetSelection` 参数，在调用前提取 `editor.state.selection` 并在替换后恢复；或对 re-ingest 路径跳过 `resetEditorContentBaseline`（content 未变时）| `editor-baseline.ts`, `TipTapEditor.tsx`, `useOpenNote.ts` |
| **P0** | 内容丢失（Bug #1）| `flushSave` 在 `handleLockToggle` 中增加脏检查；`onSaved` 回调增加 "content unchanged" 短路 | `App.impl.tsx:464`, `useEditorSave.ts` |
| **P1** | 锁按钮闪烁（Bug #1）| `handleLockToggle` 中 `flushSave` 和 `setFileLocked` 的状态更新合并为一个 `React.startTransition` 或使用 `flushSync` 包裹 | `App.impl.tsx`, `AppEditorWorkspace.tsx` |
| **P1** | 新建文档加载闪烁（Bug #2）| `scheduleDocumentLoadingVisibility` 增加自适应：检查 surface record 是否已 `ready`，如果已就绪则跳过 skeleton | `AppEditorWorkspace.tsx:364` |
| **P2** | 粘贴失败（Bug #3）| `commitPendingNoteOpen` 调用时传入 `skipTickBump = true`（prepared HTML 已渲染，无需 re-ingest）；编辑器 staging 阶段允许 paste 穿透 | `useTabManager.ts`, `AppEditorWorkspace.tsx:553` |
| **P2** | `editorContentTick` 滥用 | 拆分为 `diskLoadVersion`（仅磁盘加载时 bump）和 `contentResetSignal`（按需），在 `applyCommittedNoteOpen` 中消除无条件的 bump | `useTabManager.ts`, `TipTapEditor.tsx`, `App.impl.tsx` |

### 推荐实施顺序

```
  P0 → 光标跳跃修复（最影响用户体验，修复后立即生效）
  P0 → 内容丢失修复（与 P0 并行）
  P1 → 锁按钮闪烁修复
  P1 → 新建文档加载闪烁修复
  P2 → 粘贴失败修复
  P2 → editorContentTick 重构
```

### 关键改动位置速查

| 文件 | 行号 | 改动类型 |
|------|------|---------|
| `src/lib/editor-baseline.ts` | 17-38 | 增加 `selection` 参数，支持保留光标 |
| `src/hooks/useEditorSave.ts` | 63-78 | `runSaveOnce` 增加 "content unchanged" 短路 |
| `src/hooks/useTabManager.ts` | 286-343 (applyCommittedNoteOpen) | 消除无条件 `editorContentTick` bump |
| `src/hooks/useTabManager.ts` | 673-720 (handleNewNote) | `commitPendingNoteOpen` 传入 `skipTickBump` |
| `src/App.impl.tsx` | 464-482 (handleLockToggle) | 增加脏检查 + 合并状态更新 |
| `src/components/layout/AppEditorWorkspace.tsx` | 364-395 | 自适应加载门延迟 |
| `src/lib/document-open-runtime.ts` | - | `DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs` 可配置化 |

---

_审计完成于 2026-07-06。本文档仅做架构分析，不包含代码修改。_
