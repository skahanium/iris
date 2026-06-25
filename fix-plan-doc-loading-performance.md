# 文档打开/切换性能修复方案

> 日期：2026-06-25
> 状态：方案阶段，待用户确认后执行

---

## 问题总览

| # | 现象 | 根因类别 | 严重程度 |
|---|------|----------|----------|
| 1 | 冷启动文档 2-5 秒 | 管线全程主线程串行阻塞 | P0 |
| 2 | 左侧目录已加载，又进入加载动效 | `pendingOpen` 与 `pendingNoteOpen` 不同步清除 | P1 |
| 3 | 从欢迎页热切换 tab，先闪现之前页面再进入目标页面 | `setHomeActive(false)` 同步但 `activateTab` 异步，中间出现 stale render | P0 |
| 4 | 热切换有亚秒级延迟 | `persistAndCacheTab` 阻塞 I/O，即使 tab 未修改 | P0 |

### 已完成的架构修复（前置工作）

以下修复已在 2026-06-25 应用：

| 修复 | 文件 | 效果 |
|------|------|------|
| 空文档 warm-switch：`if (cached)` → `if (cached !== undefined)` | `useTabManager.ts:440` | 空笔记不再走冷打开 |
| warm-switch 不递增 `editorContentTick` | `useTabManager.ts:248-250,452,573` | 消除 HTML 缓存清空 + 主线程重解析 |
| 移除 100ms 骨架展示延迟 | `AppEditorWorkspace.tsx:61-62` | 骨架立即展示 |
| surface identity 改为 path-stable | `AppEditorWorkspace.tsx:141-143` | 已打开 tab 不再重挂载编辑器 |
| reingestKey 变化时原地更新内容 | `TipTapEditor.tsx:541-550` | 安全网：避免依赖重挂载 |

---

## 根因分析

### 根因 1：冷打开管线全程主线程串行阻塞

从用户点击到编辑器可见，每一步都在主线程上顺序执行：

```
fileRead(path)              → IPC → Tauri invoke → Rust 读盘
parseNoteForEditor()        → 主线程同步 markdown 解析（frontmatter + body 拆分）
ingestMarkdownForEditor()   → 主线程同步 markdown → TipTap HTML 转换
useEditor({ content })      → TipTap 初始化 20+ extension、ProseMirror schema
```

大文档（>50KB）单环节可达 200-800ms，累积 2-5 秒。

**涉及文件**：
- `src/lib/ipc.ts:155-163` — `fileRead()` IPC 调用
- `src/lib/markdown.ts` — `parseNoteForEditor()`
- `src/lib/editor-ingest.ts:359-521` — `ingestMarkdownForEditor()` 同步转换
- `src/lib/editor-ingest-async.ts:39-93` — `ingestMarkdownForEditorAsync()` Worker 版本
- `src/components/editor/TipTapEditor.tsx:420-471` — `initialContent` useMemo + `useEditor`

### 根因 2：`pendingOpen` / `pendingNoteOpen` 双轨 loading 不同步

`AppEditorWorkspace.tsx` 中有两条独立的 loading 控制路径：

```typescript
// 路径 A：由 loading gate state 控制
const showDocumentLoading = Boolean(
  documentLoadingGate.identityKey === currentSurfaceIdentity &&
  documentLoadingGate.visible
);

// 路径 B：由 home pending 控制
const pendingOpenLoading = Boolean(
  pendingOpen && !pendingOpen.error &&
  (!currentEditorSurface || homePendingMatchesPath(...))
);
```

commit 阶段时序：
1. `showDocumentLoading` 关闭（loading gate dismissed）→ editor 可见 → outline 渲染
2. `pendingOpenLoading` 仍为 `true`（`pendingOpen` 未被清除）
3. `DocumentOpenLoadingSurface` 叠加在 editor 上层
4. 下一次 render 才清除 → **用户看到"目录出来了又进加载动效"**

**涉及文件**：
- `src/components/layout/AppEditorWorkspace.tsx:347-354, 370-378, 717-719`
- `src/hooks/useHomeWorkspaceTransitions.ts:107-123` — `clearPendingOpenFromWorkspace`

### 根因 3：Fire-and-forget 异步导致中间渲染 stale 状态

`src/hooks/useHomeWorkspaceTransitions.ts:125-143`：

```typescript
const handleActivateWorkspaceTab = useCallback(
    (path: string) => {
      cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
      setHomeActive(false);        // 同步 → 立即触发 React render
      if (path.startsWith("artifact:")) { ... return; }
      setActiveArtifactId(null);   // 同步 → 触发 render
      void activateTab(path);      // 异步 → fire-and-forget，render 时尚未完成
    },
    ...
);
```

时序：
1. `setHomeActive(false)` → React render → `homeActive = false`, `activePath` = 切换前的旧值
2. `currentEditorSurface` 基于旧 `activePath` 计算 → **旧 tab 的编辑器短暂可见**
3. `activateTab(path)` 完成 → `setActivePath(B)` → 再次 render → 新 tab 可见

### 根因 4：热切换链路内嵌阻塞 I/O

`src/hooks/useTabManager.ts:412-464` — `activateTab()` 内部：

```typescript
const leaving = activePathRef.current;
if (leaving) {
    await persistAndCacheTab(leaving);  // ← 阻塞！等待完成才执行后续
}
// ... 然后才 applyCommittedNoteOpen
```

`persistAndCacheTab` → `persistBeforeLeave` → `persistActiveTabBeforeLeave`：

```
waitForEditorRef(editorRef)     → rAF 轮询（最多 1500ms）
flushSaveForPath → writeNoteAtPath
  → getMarkdown()               → serializeOpenNote() → 序列化全部编辑器内容
  → isNoteSubstantivelyEmpty()  → 检查是否空笔记
  → fileWrite()                 → IPC → 磁盘写入
```

即使 tab 未修改（clean），仍需 `serializeOpenNote()` 序列化与上次保存比较。

**涉及文件**：
- `src/hooks/useTabManager.ts:142-156` — `persistAndCacheTab`
- `src/hooks/useTabManager.ts:424-432` — `activateTab` 中的 persist
- `src/hooks/useAppPersistenceLifecycle.ts:143-194` — `persistBeforeLeaveRef`
- `src/lib/persist-before-leave.ts:20-51` — `persistActiveTabBeforeLeave`
- `src/lib/wait-for-editor.ts:5-21` — `waitForEditorRef`
- `src/hooks/useEditorSave.ts:131-151` — `flushSaveForPath`

---

## 修复方案

### 方案 A：消除欢迎页 → tab 切换的中间渲染（P0）

**文件**：`src/hooks/useHomeWorkspaceTransitions.ts:125-143`

**策略**：将 `setHomeActive(false)` 延迟到 `activateTab` 完成后执行。

```typescript
const handleActivateWorkspaceTab = useCallback(
    async (path: string) => {
      cancelHomeOpenTransitions(homeOpenSequenceRef, setPendingOpen);
      if (path.startsWith("artifact:")) {
        setHomeActive(false);
        activateArtifact(path);
        return;
      }
      setActiveArtifactId(null);
      await activateTab(path);
      setHomeActive(false);
    },
    [activateArtifact, activateTab, setActiveArtifactId, setHomeActive, setPendingOpen],
);
```

**关键点**：
- `activateTab` 从 `void activateTab(path)` 改为 `await activateTab(path)`，确保完成后再解除 home 态
- 从 `useCallback` 改为 `async` 函数，依赖数组中无额外变动
- 用户点击 tab 后维持 welcome 页面不变，`activateTab` 完成后一次性切换到目标 tab

**风险**：`activateTab` 是 async 且超时较长时用户可能感觉卡住（但比闪现旧页面好）

---

### 方案 B：热切换跳过 clean tab 的持久化 I/O（P0）

**文件**：`src/hooks/useTabManager.ts:420-456`

**策略**：`activateTab` 中仅 dirty tab 执行 `persistAndCacheTab`；clean tab 仅更新内存缓存。

```typescript
const activateTab = useCallback(
    async (path: string) => {
      if (!tabsRef.current.some((t) => t.path === path)) {
        await openFile(path);
        return;
      }
      if (activePathRef.current === path) return;

      const seq = ++openFileSeqRef.current;
      const openStartedAt = performance.now();
      cancelPendingNoteOpen();

      const leaving = activePathRef.current;
      if (leaving) {
        const leavingTab = tabsRef.current.find((t) => t.path === leaving);
        if (leavingTab?.dirty) {
          await persistAndCacheTab(leaving);
        } else {
          // clean tab: only update in-memory cache, no I/O
          cacheTabMarkdown(leaving, markdownRef.current);
        }
      }

      if (openFileSeqRef.current !== seq) return;

      const cached = tabMarkdownCacheRef.current.get(path);
      // ... rest unchanged
    },
    ...
);
```

**关键点**：
- `tabsRef.current` 包含 `dirty` 标记，在用户编辑时由 `markDirty()` 设置
- clean tab 仅 `cacheTabMarkdown()`，不做 `serializeOpenNote` / `fileWrite` / `waitForEditorRef`
- dirty tab 保持原有完整持久化流程

**风险**：低。clean tab 的内存缓存已包含最新内容，`markdownRef.current` 在离开前通过 `persistBeforeLeave` 更新过。

---

### 方案 C：同步清除 `pendingOpen` 与 `pendingNoteOpen`（P1）

**文件**：`src/components/layout/AppEditorWorkspace.tsx:399-428`

**策略 A（最小改动）**：`pendingOpenLoading` 检查中加入 `activeSurfaceRecord?.ready` 条件。

```typescript
const pendingOpenLoading = Boolean(
    !homeActive &&
    !activeArtifactTab &&
    !activeMediaTab &&
    pendingOpen &&
    !pendingOpen.error &&
    (!currentEditorSurface ||
      homePendingMatchesPath(pendingOpen, currentEditorSurface.path)) &&
    !activeSurfaceRecord?.ready  // ← 新增：surface 就绪即禁止 loading
);
```

**策略 B（更彻底）**：将 `pendingOpenLoading` 的含义从"home pending 处理中"改为"loading 状态统一由 loading gate 管理"，删除独立的 `pendingOpenLoading` 逻辑。

**推荐**：先采用策略 A，风险最低。

---

### 方案 D：冷打开管线性能优化（P1-P2）

#### D1：全部文档走 Web Worker ingest

**文件**：`src/components/editor/TipTapEditor.tsx:420-471`

当前仅大文档（>50KB）用 Worker，小文档走主线程同步 `ingestMarkdownForEditor`。

改为：始终使用 `ingestMarkdownForEditorAsync`（Web Worker）。对首帧渲染必要时 fallback 空状态。

#### D2：已挂载 editor 时跳过 `initialContent` 重计算

**文件**：`src/components/editor/TipTapEditor.tsx:420-471`

`initialContent` useMemo 当前依赖 `reingestKey`，每次变化都重新同步 ingest。已挂载的 editor 不需要此值——`useEditor` 仅在初始化时使用 `content`。

```typescript
const initialContent = useMemo(() => {
    if (editorRef.current && !editorRef.current.isDestroyed) {
      return editorRef.current.getHTML(); // 已挂载：直接取当前内容
    }
    // ... 原有逻辑
}, [...]);
```

#### D3：TipTap extension 按需加载

`useEditor` 初始化 20+ extension。非核心 extension（如 CodeBlockLowlight、Table 系列、Footnote）可延迟加载或按需注册。

---

## 执行优先级

| 优先级 | 方案 | 预期效果 | 风险 |
|--------|------|----------|------|
| **P0** | B：热切换跳过 clean tab I/O | 热切换延迟从 ~200-500ms 降至 ~10ms | 低 |
| **P0** | A：消除 render gap | 热切换无旧页面闪现 | 中（需改函数签名） |
| **P1** | C：loading 双轨同步 | "目录→loading→editor" 闪烁消除 | 低 |
| **P1** | D1：全量 Worker ingest | 冷启动减少 100-800ms | 中（首帧可能空白） |
| **P2** | D2：跳过无效 useMemo | 减少不必要的 CPU 工作 | 低 |
| **P2** | D3：extension 懒加载 | 冷启动减少 50-200ms | 中（需验证 schema 一致性） |

---

## 验证方法

1. **typecheck / lint / format**：`npm run typecheck && npm run lint && npm run format:check`
2. **单元测试**：`npm run test`
3. **手动验证场景**：
   - 冷启动打开大文档（>50KB）→ 用 Performance 面板记录 `fileRead` → `parseNoteForEditor` → `ingestMarkdownForEditor` → `useEditor` 各阶段耗时
   - 欢迎页热切换 tab（clean tab）→ 测量点击到可见的延迟
   - 打开新文档 → 观察是否出现"目录→loading→editor"闪烁
4. **合同测试**：`document-open-first-frame.test.tsx` 确认更新后的 loading 行为
