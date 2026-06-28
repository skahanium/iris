# Iris 性能优化设计 - 流式渲染、冷启动与编辑器跟手度

**日期**: 2026-06-28  
**状态**: 设计中  
**适用分支**: `codex/v1.2.1-alpha`

---

## 结论摘要

Iris 当前的主要性能问题不是“缺少缓存”本身，而是几个高频路径仍会在主线程上争抢交互帧预算：

- AI 流式输出期间，`AiMessageBubble` 会在 React render 阶段同步执行 `renderMarkdownWithProfile`。
- 冷启动仍依赖 Google Fonts 外链，离线或弱网时会引入不可控等待。
- `useTabManager` 已经变成大 hook，但是否是当前瓶颈需要先用 Profiler 证明。
- 编辑器 Tab 视图缓存已经存在，不能再按“从零新增 Editor 缓存层”的假设设计。

因此本设计采用“先量测、再低风险减压、最后迁移重负载”的路线：

1. **Phase 0: 建立性能基线与回归门槛**。
2. **Phase 1: 低风险主线程减压**，包括字体自托管、流式 render 延迟反映、非流式消息可见性优化。
3. **Phase 2: 状态与编辑器缓存校准**，先修正 `useTabManager` 和现有 surface 缓存的真实热区，不新增重复缓存实体。
4. **Phase 3: Worker 化流式 Markdown 渲染**，但必须复用 markdown contract，不能绕开现有渲染、安全和引用契约。

---

## 目标与非目标

### 目标

- 冷启动不再依赖外部字体网络请求。
- AI 长流式输出期间，点击、滚动、输入和 Tab 切换保持可响应。
- 已完成消息和离屏内容减少无意义 layout / paint。
- Tab 切换性能优化建立在当前 `AppEditorWorkspace` surface 缓存之上，而不是重复造一套 Editor 缓存。
- 所有改动保持 Markdown 契约、安全净化、引用链接和笔记数据原则不变。

### 非目标

- 不引入新的全局状态库。
- 不更换编辑器、Markdown 解析器或桌面框架。
- 不把用户 `.md` 内容写入新的专有缓存格式。
- 不新增 Service Worker、SSR/SSG 或图片构建管线。
- 不为了优化而新建大而全的抽象；新增实体必须有量测证据和清晰边界。

---

## 现状校准

### 已经存在的优化

- `useAssistantLlmStream` 已使用 rAF 合并 token 更新，并用 `startTransition` 降低流式状态更新优先级。
- `useStreamingContent` 已对流式内容做 80ms / 段落 / 大跳变节流。
- AI 消息列表已使用 `@tanstack/react-virtual` 虚拟滚动。
- `renderMarkdownWithProfile` 对非 streaming 渲染结果已有模块级 LRU 缓存。
- 大文档编辑器 ingest 已可通过 `markdown-ingest.worker.ts` 转到 Worker。
- 编辑器 HTML 预处理缓存和 `preparedEditorHtml` 已存在。
- `AppEditorWorkspace` 已使用 path-stable `surfaceRecords` 保留多个 ready editor surface，并通过 hidden surface 支撑快速回切。
- 流式气泡已有 `contain: layout paint style`。

### 仍然成立的问题

- `index.html` 仍引用 Google Fonts 和 gstatic，冷启动受外部网络影响。
- AI 助手消息的流式 Markdown 渲染仍在主线程同步执行：
  - `AiMessageBubble.AssistantBody`
  - `useStreamingContent`
  - `renderMarkdownWithProfile(..., "chat_assistant", { streaming })`
- streaming 渲染不走 LRU 缓存，每次有效快照仍会构建 fragments、做 streaming repair、渲染、净化。
- `useTabManager` 体积约 24KB，职责包含 tab registry、dirty、frontmatter、打开流程、缓存和关闭生命周期；但当前主要消费者集中在 `App.impl.tsx`，拆分前需要 Profiler 证据。

### 原设计需要修正的假设

- “Tab 切换时编辑器被销毁重建”不是当前事实。现有 `AppEditorWorkspace` 已保留 hidden editor surface。
- “新增 Editor WeakRef 池”不是首选方案。WeakRef 的 GC 时机不可控，也会和当前 React surface 生命周期重复。
- “Worker 内 marked.parse + sanitize”不足以表达现有 Markdown 渲染契约。Worker 输出必须等价于 `renderMarkdownWithProfile`，否则会破坏引用、stream repair、warnings 和安全策略。
- Suspense 本身不会让同步 Markdown parse 自动可中断；只有存在真正 suspending resource 时才有意义。低风险阶段应优先使用 `useDeferredValue` 和节流，Worker 阶段再考虑 async resource 边界。

---

## 性能预算与验收口径

所有优化都必须有“改前 / 改后”记录。没有量测结果的改动不能声称性能完成。

| 场景          | 指标                | 目标                                                               |
| ------------- | ------------------- | ------------------------------------------------------------------ |
| 冷启动离线    | 字体网络请求        | 0 个外部字体请求                                                   |
| 冷启动离线    | 首屏可见            | 不因字体 CSS 请求阻塞                                              |
| AI 长流式输出 | 主线程 long task    | 30 秒流式期间无连续交互卡死；记录 >50ms long task 数量并较基线下降 |
| AI 长流式输出 | 用户交互            | 流式期间滚动、点击、输入不被 Markdown 渲染连续阻塞                 |
| AI 消息滚动   | 虚拟列表高度稳定性  | 无明显空白、跳动、反复重测                                         |
| Tab 回切      | editor surface 命中 | 已打开 tab 回切不重新 ingest Markdown                              |
| Tab 打开      | 冷 / 热打开 trace   | 保留现有 `documentOpen` trace，并记录 hot/warm/cold 差异           |

建议在实现计划中补充一个轻量性能手册或脚本，至少覆盖：

- 离线冷启动录制。
- 3000-8000 字 AI 流式输出录制。
- 包含代码块 / 表格 / 引用的 AI 消息流式输出。
- 5 个已打开 tab 之间快速切换。
- 10000+ 文件库的首次文件树与编辑器打开路径。

---

## Phase 0 - 基线与守门

### 0.1 建立可复现实验场景

在改动前先固定三组测试材料：

- **AI 流式样本**：普通段落、代码块、表格、引用、未闭合 Markdown 片段。
- **编辑器样本**：小文档、中等文档、超过 50KB 的大文档。
- **库规模样本**：小库、千级文件、万级文件。

输出内容不需要进入用户笔记；测试 fixture 放在测试目录或开发文档目录，避免污染真实 vault。

### 0.2 补齐性能记录模板

每次优化记录：

- 机器与构建模式。
- 数据集大小。
- 操作步骤。
- 改前 / 改后截图或 trace 文件路径。
- long task、commit 次数、主要耗时函数。
- 是否影响安全、Markdown contract、虚拟滚动测量。

### 0.3 回归门槛

任一阶段完成前必须通过：

- `npm run lint`
- `npm run format:check`
- `npm run typecheck`
- `npm run test`
- 与改动相关的 Rust 检查；如未触及 Rust，可说明不运行原因。

---

## Phase 1 - 低风险主线程减压

### 1.1 字体自托管与冷启动去网络化

**问题**

当前 `index.html` 通过 Google Fonts 加载 Inter、JetBrains Mono、Noto Sans SC、Noto Serif SC。桌面应用冷启动不应依赖外部字体 CDN。

**设计**

- 删除 Google Fonts `preconnect` 和 stylesheet 外链。
- 优先自托管体积较小、使用广泛的 Latin 字体：
  - Inter: 400 / 500 / 600 / 700
  - JetBrains Mono: 400 / 500
- 中文字体默认使用系统字体栈，避免把完整 CJK 字体一次性打进应用包。
- 如果标题字体确实需要 Noto Serif SC，再单独评估子集化或只保留必要字重；不能直接把大体积 CJK 全量字体作为“零风险”改动。
- `@font-face` 使用 `font-display: swap`。标题装饰字体如后续保留，可使用 `optional`。
- `index.html` 仅 preload 首屏一定使用、体积可控的字体。
- 在文档或 license 注释中记录字体来源、版本和 OFL 许可。

**涉及文件**

- `index.html`
- `src/styles/globals.css`
- `src/assets/fonts/`
- 如新增字体文件，补充 license 记录。

**验收**

- 离线启动无 Google Fonts / gstatic 请求。
- 应用首屏字体 fallback 不阻塞。
- 包体增量可解释；若 CJK 字体导致包体明显增大，必须拆成后续决策。

### 1.2 流式 Markdown 延迟反映

**问题**

`useAssistantLlmStream` 已降低状态更新优先级，但 `AiMessageBubble` 仍会在 render 阶段同步渲染 Markdown。

**设计**

- 在 `AssistantBody` 中引入 `useDeferredValue`：
  - `renderContent` 仍由 `useStreamingContent` 控制快照频率。
  - `deferredRenderContent = useDeferredValue(renderContent)`。
  - streaming 时用 deferred 内容渲染；streaming 结束时立即使用最终 content。
- 不在此阶段引入 Suspense resource，避免制造空 fallback 或隐藏真实同步成本。
- 保留现有 `startTransition`、rAF 和 `useStreamingContent`。
- 若 deferred 内容落后太多，可增加轻量状态标记，但不显示额外说明文案。

**涉及文件**

- `src/components/ai/AiMessageBubble.tsx`
- 相关测试：至少覆盖 streaming 结束时最终内容同步展示。

**验收**

- 流式期间 Profiler commit 不再紧贴每次 token flush。
- 结束态不会停留在旧快照。
- 引用点击、代码复制、错误兜底行为不变。

### 1.3 非流式消息的 `content-visibility`

**问题**

已完成 AI 消息离屏后仍可能参与无意义 paint。当前流式气泡不适合使用 `content-visibility: auto`，但 finalized bubble 可以试验。

**设计**

- 仅对非 streaming assistant bubble 试验 `content-visibility: auto`。
- 必须搭配 `contain-intrinsic-size`，并优先使用接近虚拟列表估算的值。
- 不对 streaming bubble 使用 `content-visibility: auto`。
- 不对 ProseMirror 正文段落做大范围 `content-visibility`。编辑器 selection、IME、光标和测量都比只读消息更敏感。

**涉及文件**

- `src/styles/globals.css`
- 如虚拟列表测量受影响，回退该优化。

**验收**

- TanStack virtualizer 无空白、跳动或反复重测。
- Paint flashing 显示离屏 finalized 消息减少 paint。
- streaming 消息高度增长稳定。

---

## Phase 2 - 状态与编辑器缓存校准

### 2.1 `useTabManager` 先量测再拆分

**问题**

`useTabManager` 体积大且职责多，但盲目拆成多个 hook 可能只是移动复杂度。当前主要消费者集中在 `App.impl.tsx`，拆分收益需要证据。

**设计**

先用 React Profiler 标出具体热点：

- dirty 标记是否导致整个 `App.impl.tsx` 下游重渲染。
- activePath / markdown / pendingNoteOpen 是否让 AI panel、status bar、overlays 产生无关更新。
- frontmatter ref 是否真的需要 state 化。

再按证据选择最小拆分：

- 如果 dirty 更新是热点：优先抽 `useTabDirtyState` 或把 dirty 查询下沉到 status / tab bar 所需边界。
- 如果 open lifecycle 是热点：把打开流程 helper 移出 hook 文件，但不改变外部 API。
- 如果只是文件过大但不是运行时热点：先拆纯函数和类型，不做状态架构重排。

**边界**

- 保留 `useTabManager` 作为对 `App.impl.tsx` 的门面，避免一次性迁移所有调用。
- 不引入 Redux / Zustand / Jotai。
- 不用跨 hook mutable ref 拼出隐式共享状态，除非已有明确生命周期 owner。

**验收**

- Profiler 能显示重渲染范围缩小或 commit 时间下降。
- 行为测试覆盖打开、关闭、脏标记、重命名、空新笔记丢弃。
- IPC 契约不变。

### 2.2 复用现有 editor surface 缓存

**问题**

原方案计划新增 `EditorViewCache` 和 WeakRef 池，但当前 `AppEditorWorkspace` 已经维护 `surfaceRecords`，并用 hidden DOM 保留 editor surface。

**设计**

- 不新增 `EditorViewCache.tsx`。
- 不使用 WeakRef 管理 editor view。React 当前已经是生命周期 owner，WeakRef 会增加不可预测性。
- 改为审计现有缓存：
  - `READY_SURFACE_RETAIN_LIMIT` 是否过大或过小。
  - `openNotePaths` 过滤是否会过早释放 surface。
  - `contentChanged` 判定是否导致不必要 reingest。
  - hidden surface 是否仍触发不必要 stats / outline / resize 更新。
- 如量测确认需要优化，优先在现有 `AppEditorWorkspace` 内做小范围调整。

**验收**

- 已打开 tab 回切不重新调用 ingest。
- 大文档回切时命中现有 surface，不重新显示冷打开 loading surface。
- hidden editor 不参与不必要的 expensive 更新。
- 内存占用随打开 tab 数量可解释，超过保留上限后能释放。

### 2.3 最近文档预热

**问题**

冷打开仍需要读取、parse frontmatter、准备 editor HTML。当前已有 `preparedEditorHtml` 和 warm prepared notes，预热应复用这条路径。

**设计**

- 使用现有 `prepareVisibleNote` / `warmPreparedNotes` 管线预热，不新建并行缓存。
- 预热触发点：
  - recent notes hover。
  - Tab 相邻项 hover 或 keyboard focus。
  - 首页最近文档可见后 idle 预热前 1-3 个。
- 使用 `requestIdleCallback` 时必须提供 timeout fallback。
- 用户主动打开前台文档时取消低优先级预热，避免抢占主线程。

**验收**

- 预热命中时冷打开 trace 变为 warm/hot。
- 快速连续切换时不会堆积过期 parse。
- 不修改 `.md` 文件内容。

---

## Phase 3 - Worker 化 AI 流式 Markdown 渲染

### 3.1 原则

Worker 的目标不是“另一套更快的 Markdown 渲染”，而是把现有 `renderMarkdownWithProfile` 的重活搬离主线程。

必须保持：

- `chat_assistant` profile 输出等价。
- streaming repair 行为等价。
- 引用 linkification 行为等价。
- sanitize 策略等价。
- warnings / preserveFragments / stats 至少不影响现有消费者；如果主线程只需要 HTML，也要保留后续扩展空间。

### 3.2 Worker API

`src/workers/markdown-render.worker.ts`

输入：

```ts
type MarkdownRenderWorkerRequest =
  | {
      type: "render";
      id: number;
      profile: "chat_assistant";
      content: string;
      streaming: boolean;
    }
  | {
      type: "abort";
      id: number;
    };
```

输出：

```ts
type MarkdownRenderWorkerResponse =
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
```

实现要求：

- Worker 内调用共享的 `renderMarkdownWithProfile` 或等价的专门导出函数。
- 不在 Worker 中写简化正则 sanitize。
- 用递增 `id` 丢弃过期结果。
- 维护最新 content hash，相同内容跳过。
- 出错时主线程回退到现有同步渲染路径。

### 3.3 主线程集成

主线程只保留一个清晰的异步渲染边界：

- `useStreamingContent` 继续负责快照节流。
- 优先新增 `useMarkdownRenderWorker` 这类窄 hook；如果实现证明现有 hook 扩展更小，则必须保持同等边界：只管理 Worker 生命周期和异步结果，不接管节流策略。
- `AiMessageBubble` 根据 hook 返回值决定 HTML：
  - Worker ready：使用 worker HTML。
  - Worker pending：继续显示上一个 HTML。
  - Worker error / unsupported：回退同步 `renderMarkdownWithProfile`。

Worker 生命周期：

- 首次 streaming assistant 消息创建。
- 流式结束后保留短时间复用，空闲后 terminate。
- 组件卸载时 abort 当前 id。

### 3.4 兜底策略

不使用固定 100ms 内强制回退作为常态策略。100ms 对复杂代码块可能太激进，会导致主线程和 Worker 双重解析。

回退策略：

- 首次 Worker 初始化期间允许显示上一个快照。
- 若 Worker 报错或连续超时，禁用本轮 Worker 并同步渲染。
- 超时阈值按内容长度分级，例如短内容 150ms，中等 500ms，大内容 1000ms。
- 所有过期 Worker 结果直接丢弃。

**验收**

- 流式 trace 中主线程不再出现主要 Markdown parse / lowlight / sanitize 长任务。
- Worker 输出与同步 `renderMarkdownWithProfile` 在核心 fixture 上一致。
- 引用点击、代码块、表格、未闭合 Markdown、错误 fallback 均通过测试。

---

## 测试策略

### 单元与契约测试

- `renderMarkdownWithProfile` 同步与 Worker 输出等价测试。
- streaming repair fixture 测试。
- 引用链接 fixture 测试。
- `useStreamingContent` 与 deferred 结束同步测试。
- `content-visibility` 相关测试可用源码契约或浏览器验证补充。

### 集成测试

- AI 长流式输出期间最终消息完整展示。
- 复制代码按钮仍可用。
- citation click 仍传回正确 ref。
- Tab 快速切换后 active editor、title、dirty 状态正确。

### 手工性能验证

- DevTools Performance: AI 长流式输出。
- React Profiler: `AiMessageBubble`、`AiMessageList`、`App.impl.tsx`。
- Paint flashing: finalized message 离屏 paint。
- 离线启动: Network 面板确认字体请求。

---

## 风险与缓解

| 风险                                     | 阶段 | 缓解                                       |
| ---------------------------------------- | ---- | ------------------------------------------ |
| 字体自托管导致包体显著增加               | P1   | 默认只自托管 Latin 字体，CJK 字体单独评估  |
| 字体许可记录不完整                       | P1   | 记录来源、版本和 OFL license               |
| deferred 内容让流式输出略滞后            | P1   | 仅 streaming 使用，结束时同步最终 content  |
| `content-visibility` 干扰虚拟列表测量    | P1   | 小范围启用，测量异常立即回退               |
| `useTabManager` 拆分制造更多共享状态     | P2   | Profiler 先行，保留门面 API，按热点拆      |
| 新 Editor 缓存与现有 surfaceRecords 重复 | P2   | 禁止新增 EditorViewCache，优先审计现有实现 |
| Worker 输出与同步渲染不一致              | P3   | 复用 markdown contract，增加等价 fixture   |
| Worker 回退造成双重解析                  | P3   | 分级超时，错误后本轮禁用 Worker            |
| Worker bundle 体积增加                   | P3   | 只搬 chat assistant 渲染所需路径，量测产物 |

---

## 建议交付顺序

1. **基线记录**：补性能记录模板和 fixture。
2. **字体去网络化**：先移除外部字体依赖，控制包体。
3. **流式 deferred 渲染**：低风险减少交互抢占。
4. **非流式消息 `content-visibility` 试验**：小范围、可回退。
5. **Profiler 审计 `useTabManager` 和 `AppEditorWorkspace`**：决定是否拆分。
6. **Worker 化 AI Markdown 渲染**：作为深水区，先做等价测试再接 UI。

---

## 不做清单

- 不新增 `EditorViewCache.tsx` 或 WeakRef editor 池。
- 不新增全局状态管理库。
- 不绕过 `renderMarkdownWithProfile` 写第二套 Markdown 渲染。
- 不把 streaming bubble 设为 `content-visibility: auto`。
- 不直接打包完整 CJK 字体作为默认方案。
- 不修改用户 `.md` 笔记内容。
- 不改变 IPC、API Key、安全日志和 SQLite 数据原则。

---

## 后续实施计划提示

进入实现计划时，建议拆成三个 PR：

1. **PR 1: 基线与字体去网络化**

   风险低，收益确定，便于单独回归。

2. **PR 2: 流式主线程减压**

   包含 `useDeferredValue`、必要测试和 finalized message `content-visibility` 试验。

3. **PR 3: Worker 渲染深水区**

   先建立同步 / Worker 等价测试，再接入 `AiMessageBubble`。若等价测试成本过高，应先暂停，不把简化 Worker 合入主线。
