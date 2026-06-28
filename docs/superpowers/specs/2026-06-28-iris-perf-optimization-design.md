# Iris 性能优化 — 分层渐进式渲染 & 跟手度提升设计

**日期**: 2026-06-28  
**状态**: 设计中  
**分支**: codex/v1.2.1-alpha

---

## 动机

Iris 在 AI 流式输出和冷启动时存在可感知的卡顿，编辑器 Tab 切换和输入响应也有轻微的不跟手感。用户希望点击、切换、打开、输入输出都能在开始前消除停顿。

当前已有大量优化（rAF + startTransition 流式批处理、虚拟滚动、Markdown LRU 渲染缓存、Web Worker 离线解析、CSS Contain 隔离），但根本瓶颈未解决：

1. 流式 Markdown 解析仍在主线程执行，与用户交互争抢帧预算
2. 大型状态 hook（`useTabManager` 24KB）以单体方式更新，引起连锁重渲染
3. 冷启动依赖 Google Fonts CDN 外部网络
4. CSS `content-visibility` 和 `useDeferredValue` 等现代浏览器/React 原语未充分利用

---

## 总览

三个独立层次，每层可单独交付。

```
Layer 1 (第 1-2 周)          Layer 2 (第 2-4 周)          Layer 3 (第 4-6 周)
┌──────────────────┐  ┌───────────────────────────┐  ┌─────────────────────────┐
│ 字体自托管+预加载 │  │ 状态拆分 (useTabManager)   │  │ Worker 增量 Markdown     │
│ useDeferredValue  │  │ Editor WeakRef 视图缓存    │  │ 解析 + 流式集成          │
│ Suspense 边界     │  │ 预加载最近文档解析         │  │ CSS Contain 策略延伸     │
│ content-visibility│  │                           │  │                          │
└──────────────────┘  └───────────────────────────┘  └─────────────────────────┘
     零风险                 中等重构                     深度重构
```

---

## Layer 1 — 即刻提效

### 1.1 字体自托管 + 预加载

**问题**: 冷启动时 Google Fonts CDN 需要 DNS 解析、TLS 握手、下载 CSS、再解析 gstatic CDN、下载字体文件。离线或网络差时延迟数百毫秒。

**方案**:
- 下载 Inter (wght 400/500/600/700)、JetBrains Mono (wght 400/500)、Noto Sans SC (wght 400/500/600/700)、Noto Serif SC (wght 600/700) 的 woff2 文件到 `src/assets/fonts/`
- 在 `globals.css` 中添加 `@font-face` 声明，全部使用 `font-display: swap`
- 在 `index.html` 中对 Inter（使用最广的字体）使用 `<link rel="preload" as="font" type="font/woff2" crossorigin>`
- 删除 Google Fonts `<link>` 和 `rel="preconnect"`
- Noto Serif SC（仅用于 `--font-title`）使用 `font-display: optional`，允许浏览器在字体未及时到达时使用系统后备字体

**涉及文件**:
- `index.html`: 删除 Google Fonts 外链，添加 `<link rel="preload">`
- `src/styles/globals.css`: 添加 `@font-face` 声明
- `src/assets/fonts/`: 新增目录，存放 woff2 文件

**验证**: 冷启动时间对比（DevTools Performance 录制），离线场景下字体不会被阻塞。

### 1.2 流式渲染优先级调度

**问题**: `useAssistantLlmStream` 已用 `startTransition` 包裹状态更新，但 `AiMessageBubble` 在流式渲染时同步执行 `marked` 解析，可能占用主线程超过 16ms 帧预算。

**方案**:
- 在 `AiMessageBubble.AssistantBody` 中引入 `useDeferredValue`：对流式 `content` prop 做延迟值，React 在繁忙时保留上一个快照，直到有空闲时间处理新版本——用户操作不会被流式渲染阻塞
- 在 `AiMessageList` 虚拟化列表外层包裹 `<Suspense fallback={null}>`：使得 React 并发特性可以中断流式内容更新
- 保留现有 `contain: layout paint style` CSS 隔离策略

**为什么要 `useDeferredValue` 而不是只用 `startTransition`**:
- `startTransition`（已有）降低状态更新优先级
- `useDeferredValue` 更进一步：即使状态更新进来了，组件可以选择先渲染旧版本，等待浏览器有空再切换到新版本
- 组合使用：`startTransition` 控制"什么时候更新状态"，`useDeferredValue` 控制"组件什么时候反映新状态"

**涉及文件**:
- `src/components/ai/AiMessageBubble.tsx`: 添加 `useDeferredValue` 包裹 streaming content
- `src/components/ai/AiMessageList.tsx`: 添加 Suspense 边界
- `src/hooks/useAssistantLlmStream.ts`: 无需改动

**验证**: React DevTools Profiler 检查流式渲染的调度优先级；性能合约测试通过。

### 1.3 CSS `content-visibility` 选择性应用

**问题**: 测试合约明确说明流式气泡不使用 `content-visibility: auto`（因为 streaming 内容持续增长）。但对于非 streaming 内容，这个原语可以有效跳过离屏渲染。

**方案**:
- 对非 streaming 状态的消息气泡: `.ai-message-bubble:not([data-streaming])` 添加 `content-visibility: auto`
- 对编辑器大纲隐藏区域添加 `content-visibility: auto`
- 设置 `contain-intrinsic-size: auto 500px` 防止滚动条跳动

**涉及文件**:
- `src/styles/globals.css`: 新增规则

**验证**: DevTools Rendering > Paint flashing 确认离屏消息不再触发重绘。

---

## Layer 2 — 架构优化

### 2.1 细粒度状态拆分

**问题**: `useTabManager`（24KB）是单体 hook，管理 tabs、dirty 标记、frontmatter。更新任何一个字段触发所有消费者重渲染。

**方案**:

拆分 `useTabManager` 为三个独立 hook:

```
useTabRegistry         useTabDirtyTracker        useTabFrontmatter
├── tabs               ├── dirtyMap               ├── frontmatterMap
├── activeIndex        ├── markDirty(path)        ├── getFrontmatter(path)
├── openTab(path)      ├── markClean(path)        ├── setFrontmatter(path, data)
├── closeTab(index)    ├── isDirty(path)
├── activateTab(index) ├── dirtyCount
├── reorderTabs(...)
```

**拆分原则**:
- 每个 hook 只管理一个状态域
- 消费者只导入他们需要的 hook，避免不必要的重渲染
- 使用 `useRef` 存储跨 hook 共享的 mutable 值（如 `activePathRef`），避免循环依赖

**涉及文件**:
- `src/hooks/useTabRegistry.ts`: 新建
- `src/hooks/useTabDirtyTracker.ts`: 新建
- `src/hooks/useTabFrontmatter.ts`: 新建
- `src/hooks/useTabManager.ts`: 标记废弃（保留兼容性 wrapper 或直接替换）
- `src/App.impl.tsx`: 更新 hook 导入
- 所有 `useTabManager` 的消费者：改为只导入需要的 hook

### 2.2 Editor 视图缓存与预加载

**问题**: Tab 切换时编辑器被销毁重建（React key-based remount），每次切换都走完整的 Markdown→ProseMirror HTML 消化流程。

**方案**:

**视图缓存 — WeakRef 池**:
- 当从 Tab A 切换到 Tab B 时，不销毁 Tab A 的编辑器 DOM，而是保留为 hidden 状态
- 使用 `WeakRef` 持有最多 3 个活跃编辑器的视图/状态快照
- 回切时直接从缓存恢复（避免 Markdown 消化和 ProseMirror 初始化）
- 超过 3 个缓存时，最早的被回收（WeakRef 自动 GC）

**预加载**:
- 使用 `requestIdleCallback` 预解析最近编辑文档列表中的相邻文档
- 用户连续快速切换 Tab 时取消预加载

**涉及文件**:
- `src/components/editor/EditorViewCache.tsx`: 新建，管理隐藏编辑器视图
- `src/hooks/useEditorCache.ts`: 新建 hook
- `src/components/layout/AppShell.tsx`: 集成缓存

**验证**: Tab 切换时间对比（Performance 录制），缓存命中时消化流程应该接近零延迟。

---

## Layer 3 — 深度重构

### 3.1 Worker 增量 Markdown 解析

**问题**: 流式 Markdown 解析在主线程同步执行。即便有限流（80ms），在 ~20fps rAF 批处理下，每次解析仍争抢帧预算。

**方案**:

**架构**:
```
主线程                           Worker
  │                                │
  │── flushSnapshot() ──────────► │
  │   (rAF + startTransition)     │
  │                                │── marked.parse(content)
  │                                │── sanitize(html)
  │◄────────── html + hash ───────│
  │                                │
  │── setHtml(html)                │
  │   (useDeferredValue)           │
```

**Worker 实现** (`src/workers/markdown-streaming.worker.ts`):
- 接收: `{ type: "render", id, profile, content, streaming }` 和 `{ type: "abort" }`
- 使用 `marked` 解析 + 净化（复用 `src/lib/markdown-contract/` 共享配置）
- 返回: `{ id, html, hash }`
- 维护 `lastRenderedHash`，相同内容跳过避免重复解析
- 处理竞态：`abort` 消息丢弃正在进行的解析

**主线程集成**:
- 保持现有 80ms 限流逻辑
- 需要刷新时 `postMessage` 到 Worker
- `onmessage` 使用 `useDeferredValue` 更新 HTML
- 丢弃过期 id 的结果
- 兜底：Worker 在 100ms 内未响应时回退到主线程同步渲染
- Worker 懒初始化（首次流式渲染时创建），流式结束后可选 terminate

**涉及文件**:
- `src/workers/markdown-streaming.worker.ts`: 新建
- `src/hooks/useStreamingContent.ts`: 集成 Worker 通信
- `src/lib/markdown-contract/contract.ts`: 抽取共享 marked 配置

**验证**: 
- DevTools Performance 录制流式渲染帧，主线程 Markdown 解析应消失
- 性能合约测试验证 Worker 往返延迟 < 100ms

### 3.2 CSS Contain 策略延伸

**问题**: CSS contain 目前仅用于流式气泡和编辑器图片。更多区域可以受益。

**方案**:
- `.ProseMirror` 内长段落 (>500px 估算高度): `contain: layout style`
- AI 消息列表中已完成消息: `contain-intrinsic-size: auto [estimated-height]` 配合 `content-visibility: auto`
- 不可见面板 (知识图谱/侧栏 hidden 状态): `contain: strict`

**涉及文件**:
- `src/styles/globals.css`: 新增规则

---

## 验证策略

### 每层通用验证
- 所有现有单元测试和合约测试通过
- DevTools Performance 录制关键场景
- `prefers-reduced-motion` 回退不受影响

### Layer 1 专项
- 冷启动时间对比（Performance 录制）
- React DevTools Profiler 检查流式更新的优先级标记
- Paint flashing 验证 `content-visibility` 生效

### Layer 2 专项
- Tab 切换时间对比
- React DevTools Profiler 检查重渲染范围缩窄
- 视图缓存命中率验证

### Layer 3 专项
- 帧预算对比：是否有主线程 Markdown 解析
- Worker 通信延迟量测
- 流式过程中用户操作响应时间对比

---

## 风险与缓解

| 风险 | 层级 | 缓解 |
|------|------|------|
| 字体文件版权（Google Fonts 再分发） | L1 | OFL 许可允许再分发 |
| `useDeferredValue` 可能使流式内容落后于实际状态 | L1 | 仅在 streaming 态启用，结束时立即同步 |
| `content-visibility` 影响滚动条高度 | L1/L3 | `contain-intrinsic-size` 提供预估高度 |
| 状态拆分破坏现有消费者 | L2 | 保留 `useTabManager` 作为兼容 wrapper，渐进迁移 |
| WeakRef GC 时机不确定 | L2 | 保守容量（3个），手动清理 oldest 作为补充 |
| Worker 启动延迟（首次通信开销） | L3 | 100ms 兜底回退，懒初始化 |
| isomorphic-dompurify 体积 | L3 | 折中用简化正则净化或接受额外体积 |
| Worker 内 `marked` 版本不一致 | L3 | 抽共享配置到 `src/lib/markdown-contract/` |

---

## 不做什么（YAGNI）

- ❌ 不做全量状态管理库替换（Redux/Zustand/Jotai）— 够用的拆分即可
- ❌ 不做 SSR/SSG — 桌面应用无此需求
- ❌ 不做 Service Worker 离线缓存 — Tauri 已有本地渲染
- ❌ 不做 `vite-imagetools` 图片构建管线 — 非图片主导应用
- ❌ 不做 `content-visibility: auto` 用于 streaming 气泡 — 已验证不适合
