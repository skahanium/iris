# Iris 前端性能与 UX 审计报告

| 项     | 值                              |
| ------ | ------------------------------- |
| 审计日期 | 2026-07-02                     |
| 基线版本 | v1.2.1                         |
| 审计范围 | 前端（Tauri 2 + React 19 + TipTap + TailwindCSS + shadcn/ui） |
| 审计模式 | 只读静态分析（未修改任何文件） |
| 审计者  | opencode（AI 辅助）            |

---

## 0. 产物体积基线

来源：`D:\Iris\dist\assets`（生产构建实际产物）。

| 产物                              | 体积     | 性质                 |
| --------------------------------- | -------- | -------------------- |
| `index-*.js`                      | 514 KB   | 主应用包（eager）    |
| `markdown-render.worker-*.js`     | 276 KB   | 渲染 worker          |
| `markdown-ingest.worker-*.js`     | 269 KB   | ingest worker        |
| `prosemirror-*.js`                | 259 KB   | vendor chunk         |
| `markdown-vendor-*.js`            | 251 KB   | lowlight+marked+turndown+dompurify |
| `react-vendor-*.js`               | 194 KB   | vendor chunk         |
| `vendor-*.js`                     | 181 KB   | 其余 vendor          |
| `tiptap-*.js`                     | 135 KB   | vendor chunk         |
| `ui-vendor-*.js`                  | 114 KB   | Radix + cva + clsx   |
| `ManagementCenterPanel-*.js`      | 67 KB    | lazy chunk           |
| `VersionTimeline-*.js`            | 8.5 KB   | lazy chunk           |
| `GraphView-*.js`                  | 5 KB     | lazy chunk           |
| `index-*.css`                     | 101 KB   | 单一合并样式表       |
| 字体（6 × woff2）                 | 21–24 KB/个 | 自托管             |

关键观察：

- `vite.config.ts:101` 设 `chunkSizeWarningLimit: 500`，而 `index` 实际 514 KB，**每次构建必发警告**。
- 两个 worker 各 ~270 KB 属预期（off-main-thread，可接受）。
- 仅 3 个组件 `lazy()`：`GraphView`、`ManagementCenterPanel`、`VersionTimeline`（见 `src/components/layout/AppOverlays.tsx:30-44`）。

---

## 1. 加载速度 / Bundle

### 1.1 入口与关键路径

- `src/main.tsx:1-9` 同步引入 `./App`（→ `App.impl.tsx`）、`ErrorBoundary`、`tauri-runtime`、`tippy.js/dist/tippy.css`、`globals.css`。`tippy.css` 体积很小，但即使从不触发 slash/wikilink 也无条件加载，属轻微浪费。
- `src/App.impl.tsx:1-69` 顶层静态导入 ~30 个 hook 与多个槽位组件（`AppAiPanelSlot`、`AppEditorWorkspace`、`AppOverlays`、`AppShell`、`AppStatusBarSlot`、`TabBar`、`DesktopFrame`、`PreVaultDesktopFrame`、`StartupSplash`、`DocumentTitleField`、`Button`）。
- `src/components/layout/AppEditorWorkspace.tsx:10` 静态引入 `TipTapEditor`，后者在 `src/components/editor/TipTapEditor.tsx:3,23` 静态引入 `lowlight` + `CodeBlockLowlight`。即 **lowlight 位于编辑器关键路径**，即使当前笔记无代码块也会被解析。

### 1.2 可延迟的重依赖

| 依赖                          | 引用点                                         | 实际触发时机                 | 建议 |
| ----------------------------- | ---------------------------------------------- | ---------------------------- | ---- |
| `lowlight` / `highlight.js`   | `src/components/editor/TipTapEditor.tsx:23`、`src/lib/markdown-render.ts:3` | 仅出现围栏代码块时         | 编辑器内改 `await import("lowlight")`，首次遇 code block 再装载；大文档已回退 `CodeBlock`（`TipTapEditor.tsx:384-388`） |
| `tippy.js`                    | `src/components/editor/extensions/WikiLinkExtension.ts:9`、`SlashCommandExtension.ts:4` | 仅 `/` 或 `[[` 触发时       | 首次激活时 `await import("tippy.js")` |
| `turndown` / `turndown-plugin-gfm` | `src/lib/editor-export.ts:10`、`src/lib/markdown.ts:2` | 导出 / HTML→MD              | 导出处理器内 `import()` |
| `dompurify`                   | `src/lib/sanitize.ts:1`                        | 内联 HTML 净化（worker 内已隔离） | 函数内 `import()`，仅留内联路径用 |
| `marked`                      | `src/lib/markdown.ts:1`、`src/lib/markdown-render.ts:2` | ingest / 导出                | 调用点 `import()` |

`markdown-vendor` 块（251 KB）在以上延迟化后将仅在「需要渲染/导出」的会话中下载。

### 1.3 懒加载覆盖缺口

- 仅 `GraphView` / `ManagementCenterPanel` / `VersionTimeline` 走 `lazy()`。
- **AI 侧栏未懒加载**：`src/components/layout/AppAiPanelSlot.tsx:1` 静态引入 `UnifiedAssistantPanel`（`UnifiedAssistantPanel.impl.tsx`，17 KB 源码，拖入 `AiMessageList`、`AiMessageBubble`、`ContextPacketDrawer`、`AssistantTaskSurfaces`、`PatchPreview`、`CitationCheckView`、`EvidenceChainView`、`SkillsPanel`、`SessionHistoryDropdown`、`AiMentionPopover`、`AiRulesPanel` 等）。AI 面板默认关闭（`aiPanelOpen` 多数流程为 false），属强懒加载候选。
- 应用无路由（单窗口桌面），自然"路由边界"= 编辑器工作区 vs AI 侧栏 vs 管理中心 vs 版本时间线 vs 图谱。后三者已切分，前两者未切。

### 1.4 Worker 与字体

- Worker 良好：`src/workers/markdown-render.worker.ts`、`markdown-ingest.worker.ts` 经 `new Worker(new URL("../workers/...", import.meta.url), { type: "module" })` 实例化（`src/hooks/useMarkdownRenderWorker.ts:20-25`、`src/lib/editor-ingest-async.ts`）。重活已 off-main-thread。
- 字体 preload 部分缺口：
  - `index.html:14-27` 仅 preload Inter 400 / 600。
  - `src/styles/globals.css:3-51` 声明 4 个 Inter 权重（400/500/600/700）+ 2 个 JetBrains Mono。
  - 700（标题加粗）与 JetBrains Mono 400（所有代码块）未 preload → 首次使用时 font swap。
  - `font-display: swap` 全量设置（良好），不会阻塞首屏文本。

### 1.5 启动期

- `src-tauri/tauri.conf.json:23` `visible: false` + `focus: true`：窗口隐藏至前端就绪，配合 `StartupSplash` 的 Knowledge Orbit 启动动画（`docs/design-system.md:170`），感知启动良好。
- `index.html:28-34` 内联主题 bootstrap 脚本避免 FOUC（良好）。
- `index.html:254-286` 内联 preboot splash HTML（良好模式），但其中 `<img src="/brand/iris-mark.svg">` 会在 React 挂载前发起一次 SVG fetch。可考虑直接内联 SVG markup 或 preload 该 SVG。

---

## 2. 渲染性能

### 2.1 已到位的优化（确认良好）

- **虚拟化覆盖**：
  - `VaultNavigator` 文件列表 — `src/components/file/VaultNavigator.tsx:390-409`，`useVirtualizer`，`estimateSize: 40`，`overscan: 10`。
  - `QuickOpen` — `src/components/file/QuickOpen.tsx:167-184`，`estimateSize: 56`，`overscan: 10`。
  - `AiMessageList` — `src/components/ai/AiMessageList.tsx:203-208`，采用**内容感知 `estimateSizeByContent`**（行 186-201），避免定高首次滚动空隙，`overscan: 8`。
- **`content-visibility`**：`globals.css:1672-1675` 对非流式 AI 气泡 `content-visibility: auto; contain-intrinsic-size: auto 320px;`；`:1667-1670` 对流式气泡 `contain: layout paint style;`。`globals.css:1346-1348` 媒体嵌入 `contain: content;`。
- **编辑器 surface pool**：`src/components/layout/AppEditorWorkspace.tsx:457-693` 维护至多 `READY_SURFACE_RETAIN_LIMIT = 8`（行 70）个热 `TipTapEditor` 实例，通过 `data-editor-visibility` + `aria-hidden` + `pointer-events-none opacity-0` 切换（行 704-714）。切标签不重建 ProseMirror。
- **TipTap 配置**：`TipTapEditor.tsx:551-594` `useEditor({ shouldRerenderOnTransaction: false, immediatelyRender: true })`；大文档（`LARGE_DOC_BODY_THRESHOLD = 12_000`，行 99）回退 `CodeBlock` 并提 `depth: 80` undo 历史（行 338）；`BODY_STATS_DEBOUNCE_MS = 400`。
- **无 React Context**：全应用零 `createContext` Provider，显式 props 下传（`AppOverlays.tsx:72-187`、`AppEditorWorkspace.tsx:72-135`），避免 context 驱动的全树重渲染。
- **稳定回调 Map**：`AiMessageList.tsx:288-305` 用 per-index 回调 Map 保 `AiMessageBubble` 的 `memo` 在流式期间不破。
- **memo 化到位**：`TipTapEditor`（`TipTapEditor.tsx:851`）、`EditorOutline`、`HeadingFoldOverlay`、`AiMessageBubble` + `AssistantBody`（`AiMessageBubble.tsx:114,264`）、`AiMessageList`、`ConfirmDialog`、`ConnectivityIndicators`、`DesktopTitleBar`、`StatusBar`、`WelcomeEmpty`、`ConversationSurface`。
- **预取**：`VaultNavigator.tsx:461-466` 对 `folderFiles.slice(0, PREPARE_FOLDER_LIMIT)` 调 `onPrepare` 预取前 8 个可见笔记内容（感知性能良好）。

### 2.2 缺口

| 缺口 | 位置 | 影响 |
| ---- | ---- | ---- |
| `EditorOutline` 未虚拟化 | `src/components/editor/EditorOutline.tsx:346` `entries.map` 全量渲染 | 违反 `docs/design-system.md:93`「50+ 条目虚拟化」规范；长笔记标题多时 jank |
| `GraphView` 力导向 O(n²) 同步主线程 | `src/components/graph/GraphView.tsx:37-102`，于行 188/191/205 调 50/50/100 次迭代 | 大图谱 `initGraph` 阻塞主线程 |
| `GraphView` rAF 每帧 O(n²) | `GraphView.tsx:215-292`，行 269 每帧 `forceSimulate(..., 3)` | 持续转风扇；有 45 空闲帧停止（行 280）但大图谱期间卡 |
| `memo` 边界缺失 | `VaultNavigator`/`VaultNavigatorBody`（`VaultNavigator.tsx:328,1341`）、`QuickOpen`（`QuickOpen.tsx:71`）、`SearchPanel`（`SearchPanel.tsx:21`）、`KnowledgeRelationsPanel`（`KnowledgeRelationsPanel.tsx:21`）、`GraphView`（`GraphView.tsx:145`）、`UnifiedAssistantPanel`、`AppShell`、`AppEditorWorkspace`、`AppOverlays` | `App.impl.tsx` 持 ~30 `useState`，任何变更扇出全树 |
| `VaultNavigator` prepare 无 dedup | `VaultNavigator.tsx:461-466` vs `QuickOpen.tsx:99-101` 的 `preparedKeysRef` | `refresh()` 后 `folderFiles` 身份变化会重复 `onPrepare` 同一文件 |
| `App.impl.tsx:160-162` `tabsRef.current = tabs` 渲染期赋值 | 每次 render 都执行 | 成本极低，可接受 |

---

## 3. UI 风格一致性

### 3.1 已到位（良好）

- `docs/design-system.md` 存在（276 行）并作为 UI token 唯一来源，`ROADMAP.md:34` 与 `AGENTS.md` 均引用。
- 完整 token 体系：
  - 颜色：HSL CSS 变量于 `globals.css:67-250`（`:root` 暗 + `.light` 亮），经 `hsl(var(--*))` 接入 `tailwind.config.js:7-89`。语义 surface（`--surface-chrome/elevated/inset`）、`ai-workspace`、`overlay-task`、`command-highlight`、`outline-rail`、`iris-rail`、`knowledge-accent`、`editor-*`、`status-*` 齐备。
  - 半径：`--radius-sm/md/lg/xl`（6/8/12/16 px）于 `globals.css:129-133`，映射 `tailwind.config.js:90-97`。
  - 动效：`--motion-fast/base/exit/ease/ease-out` 于 `globals.css:144-148`，映射 `tailwind.config.js:138-146`。
  - 阴影：`--shadow-overlay/floating` 映射 `tailwind.config.js:98-101`。
  - 字体：`font-sans/prose/title/editor/mono` 于 `tailwind.config.js:102-137`，含完整 CJK 回退栈。

### 3.2 不一致 / 风险

| 项 | 位置 | 说明 |
| -- | ---- | ---- |
| `darkMode: ["class"]` 与 `.light` opt-in 模式冲突 | `tailwind.config.js:3` + `globals.css:177` + `index.html:30-33` | 实际：无 class = 暗、`.light` = 亮。Tailwind `class` 策略需 `dark` class 才启用 `dark:` → **未来任何 `dark:` 工具类将永远不生效**。需文档化或改 `darkMode: ["class", '[data-theme="dark"]']` |
| 半径别名反直觉 | `tailwind.config.js:95-96` | `2xl` → `var(--radius-lg)`、`3xl` → `var(--radius-xl)`，即 `rounded-2xl == rounded-lg`、`rounded-3xl == rounded-xl`。疑似有意但易误用 |
| `--overlay-scrim` 含 alpha 混入 | `globals.css:79` + `tailwind.config.js:38-40` | `--overlay-scrim: 0 0% 5% / 0.55;` 含 alpha，其余色 token（如 `--background: 0 0% 10%`）不含。若有人写 `hsl(var(--overlay-scrim) / 0.5)` 会双重衰减 |
| 语义色未全部暴露 | `globals.css:90-91` 等 + `tailwind.config.js` | `--warning`、`--classified-accent`、`--status-*` 仅 CSS 用，未暴露成 Tailwind 颜色键；其他语义色已暴露。风格分裂 |
| z-index 混用 | `tailwind.config.js:147-154` token vs `TipTapEditor.tsx:760` `z-10`、`globals.css:1436` `z-[3]`、`globals.css:1514` `z-index: 4` | 裸值与 token 混用是 stacking bug 温床 |
| `KnowledgeRelationsPanel` 活动态与全局约定不一致 | `KnowledgeRelationsPanel.tsx:92-93` `bg-task-selected` | 与 `VaultNavigator` 的 `bg-accent` / `bg-surface-inset/30` 不同 |
| **文档漂移** | `docs/design-system.md:59` | 仍写"Google Fonts 链接"，实际为自托管 woff2（`src/assets/fonts/`）。违反 `AGENTS.md §4.6`「改 UI 先更 design-system」 |

---

## 4. UI 动效 / 过渡

### 4.1 已到位（良好）

- 无 framer-motion / 动效库（栈锁定正确）。全 CSS keyframes。
- `prefers-reduced-motion` 全局禁动开关：`globals.css:534-548`（`* { animation-duration: 0.01ms !important; transition-duration: 0.01ms !important; }` + motion token 覆盖）。
- 局部覆盖：web-accent line（`globals.css:401-418`）、启动 splash（`globals.css:724-735`）、文档打开加载 scan（`globals.css:1263-1268`）、ai-stream-pulse（`globals.css:1716-1721`）、outline-ghost（`globals.css:1028-1042`）。
- `StartupSplash.tsx:34,61` 还在 JS 内 `matchMedia("(prefers-reduced-motion: reduce)")` 套 `--reduced-motion` 类。
- 过渡统一用 token：`transition-colors duration-base ease-iris-out` 模式（`KnowledgeRelationsPanel.tsx:147,179`、`SearchPanel.tsx:128,153`）。
- 流式 AI 脉冲 `ai-message-stream-pulse.tsx:18-20` 带 `aria-live="polite"` + `aria-label="正在生成回复"`，a11y 良好。
- 微动效（rail hover、outline-ghost-item、iris-rail-tab）均在 150–200 ms token 区间，克制。

### 4.2 缺口

| 缺口 | 位置 | 说明 |
| ---- | ---- | ---- |
| **GraphView canvas rAF 无 reduced-motion 守卫** | `GraphView.tsx:215-292` | CSS 全局规则管不到 canvas；前庭敏感用户仍被持续动画。需 `if (matchMedia?.("(prefers-reduced-motion: reduce)").matches) { 静态渲染; return; }` |
| Suspense fallback 全为 `null` | `AppOverlays.tsx:242,297,320` | `ManagementCenterPanel` 67 KB 块，慢盘下叠层"空白一瞬"。应给 `IrisOverlay` 骨架 + spinner 兜底 |
| 骨架屏仅 1 处 | 仅 `DocumentOpenLoadingSurface.tsx:37`（`globals.css:1186-1208` scan 动画） | 其余加载态全为纯文字"加载中…"：`VaultNavigator.tsx:1141`、`GraphView.tsx:374`、`SearchPanel.tsx:104`、`KnowledgeRelationsPanel.tsx:171`、`AiMessageList.tsx:333` |
| Web-accent line 偏重 | `globals.css:252-399` | 1.5 px 元素上叠 4 个 keyframe + `will-change` + 多重渐变 + `mask-image` + `::before/::after`。reduced-motion 已覆盖。非问题，可观察 |
| `forceSimulate` 主线程 | `GraphView.tsx:37-102` | 大图谱 O(n²) 主线程，建议移 worker 或限节点数 |

---

## 5. UX 缺口

### 5.1 空状态

| 组件 | 状态 | 位置 | 评价 |
| ---- | ---- | ---- | ---- |
| `WelcomeEmpty` | 欢迎空态 | — | 良好（含动作引导） |
| `AiMessageList` | 空对话 | `AiMessageList.tsx:331-337` | 良好（带上下文说明） |
| `VaultNavigator` | 空文件夹/无笔记 | `VaultNavigator.tsx:1142-1145` | 纯文字，无图标/插画 |
| `KnowledgeRelationsPanel` | 无反向链接/无标签 | `KnowledgeRelationsPanel.tsx:140-141,172-173` | 纯文字 |
| `QuickOpen` | 无匹配 | `QuickOpen.tsx:236-240` | 良好（图标+文案） |
| **`SearchPanel`** | **无结果** | — | **缺失**：`keywordHits` 与 `semanticHits` 均空时 ScrollArea 直接空白 |
| **`GraphView`** | **空库** | `GraphView.tsx:180` | **缺失**：`initGraph` 静默 `simRef.current = null`，画布空白无提示 |

### 5.2 错误状态

- `ErrorBoundary.tsx` 顶层于 `main.tsx:37`，作用域边界于 `AppEditorWorkspace.tsx:715`（`scope="editor"`）、`AppOverlays.tsx:319`（`scope="知识图谱"`）、`AppAiPanelSlot.tsx:80`（`scope="AI面板"`），含重试按钮。良好。
- 内联错误：`VaultNavigator.tsx:897`、`App.impl.tsx:771-778`（`role="alert"`）、`KnowledgeRelationsPanel.tsx:134-136`、`SearchPanel.tsx:106-108`、`GraphView.tsx:368-372`。覆盖良好。
- `ErrorBoundary.tsx:25-28` 仅记 `error.message.length`，不记内容 → 符合 `AGENTS.md §1.4` 不泄露笔记内容。良好。

### 5.3 加载状态

见 §4.2 骨架屏缺口。除 `DocumentOpenLoadingSurface` 外均为纯文字。

### 5.4 键盘快捷键

- `useAppShortcuts`（`src/hooks/useAppShortcuts.ts`）+ `useAppKeyboard`（`src/hooks/useAppKeyboard.ts`）于 `App.impl.tsx:626-655` 注册。
- `useListboxKeyboard`（`src/hooks/useListboxKeyboard.ts`）处理 `QuickOpen.tsx:149-159` 箭头/回车。
- `EditorOutline.tsx:202-226` 自处理箭头/回车。
- `EditorFindReplaceBar`、`useZenExitKeyboard`（禅模式退出）齐备。
- 完整快捷键表见 `docs/design-system.md:222-237`。覆盖良好。

### 5.5 Tooltip

- `tippy.js` 用于 slash command 与 wikilink 弹层（`SlashCommandExtension.ts:4`、`WikiLinkExtension.ts:9`）。
- 多数按钮用原生 `title=`（如 `VaultNavigator.tsx:809,850`、`TipTapEditor.tsx:762`）。
- **不一致**：部分按钮 tippy（即时、主题化），部分原生 title（延迟、系统样式）。无共享 tooltip 组件。

### 5.6 focus-visible / a11y

- `.iris-focus-soft` 类（`globals.css:736-757`）提供柔焦环；`button.tsx:8` `focus-visible:outline-none` + 依赖 `.iris-focus-soft:focus-visible`。
- **风险**：未挂 `iris-focus-soft` 的按钮无可见焦点环（`VaultNavigator.tsx:1182-1254`、`AiMessageList.tsx:73-95` 等内联按钮）。ROADMAP 将 WCAG 2.1 AA 标为"无限期延后"（`ROADMAP.md:183`），但键盘用户体感差。
- aria 覆盖强：`role="dialog/listbox/tablist/tab/option/menuitem/search/status/separator"`、`aria-live="polite"`、`aria-label`、`aria-hidden`、`aria-pressed`、`aria-current="location"`（`EditorOutline.tsx:288`）。
- **疑似缺口**：`src/components/ui/iris-overlay.tsx:58` 设 `aria-label` 但未用 Radix `Dialog` → Quick Open / Search / Graph / Vault Navigator 等命令叠层**可能不 trap focus**（WCAG 2.4.3）。`dialog.tsx:56` 用了 Radix Dialog，故确认/重命名对话框 OK。需进一步核实。

### 5.7 Toast / 通知系统

- **无 toast 系统**（`package.json` 无 `sonner`/`react-hot-toast` 等；grep `toast|notification` 仅命中内部 `notifyDirty`/`notifyLlmConfigChanged` pub/sub，非用户态 toast）。
- 状态反馈走状态栏 `aiStatus`（`App.impl.tsx:93`）与内联 `<p class="text-destructive">`。
- **剪贴板静默**：`AiMessageList.tsx:317-328` `handleCopyMessage` 成功/失败均吞，无任何可见反馈。轻量 toast 或复用 `aiStatus` 通道可补。

---

## 6. 资产

### 6.1 字体

- `src/assets/fonts/` 6 × woff2，21–24 KB/个 + `OFL.txt`。全 woff2，无 ttf/otf/eot 遗留。良好。
- preload 缺口见 §1.4（Inter 700、JetBrains Mono 400 未 preload）。

### 6.2 图标

- `lucide-react` 全部按具名导入（53 处，如 `import { FileText, Folder } from "lucide-react"`）。`command-list.tsx:1` 仅导入 `LucideIcon` 类型（零运行时）。tree-shake 正确，`icons-vendor` 仅 49 KB。良好。
- `VaultNavigator.tsx:2-20` 单文件导入 17 个图标，均为具名 → tree-shaking 处理。

### 6.3 图片

- `src/assets/` 除字体外无栅格图。品牌为 SVG（`public/brand/iris-mark.svg`），图谱为 canvas。媒体资产为用户 vault 内容经 `asset:` 协议（`tauri.conf.json:32`），非打包。良好。
- `index.html:7` favicon 为 SVG + PNG 回退（行 8-13）。良好。
- preboot splash（`index.html:254-286`）内联 `<img src="/brand/iris-mark.svg">` 两次 → React 挂载前发起 SVG fetch。可内联 SVG markup 或 preload。

---

## 7. 优先级排序（影响 × 性价比）

按"影响大、风险低、改动局部"原则排序。

### P1 — 懒加载 AI 侧栏（最高收益）

`src/components/layout/AppAiPanelSlot.tsx:1` 静态引入 `UnifiedAssistantPanel` 全家桶。包 `lazy(() => import("@/components/ai/UnifiedAssistantPanel"))` + `<Suspense fallback={null}>`。预计从 514 KB `index` 切出 80–120 KB。AI 面板默认关闭，多数冷启动零成本。

### P2 — `EditorOutline` 虚拟化

`src/components/editor/EditorOutline.tsx:346` 全量 `entries.map`。违反 `docs/design-system.md:93` 规范。仿 `VaultNavigator.tsx:390-409` 接 `useVirtualizer`。

### P3 — Toast 系统 + 剪贴板反馈

无 toast 系统（§5.7）。`AiMessageList.tsx:317-328` 复制静默。新增轻量 toast primitive 或复用 `aiStatus` 状态栏通道。

### P4 — 骨架屏补齐

仅 `DocumentOpenLoadingSurface` 有骨架。补 `VaultNavigator` / `SearchPanel` / `KnowledgeRelationsPanel` / `GraphView` 加载态。

### P5 — GraphView reduced-motion 守卫 + 力导向 worker 化

`GraphView.tsx:215-292` rAF 无守卫（§4.2）；`forceSimulate` O(n²) 主线程（行 37-102）。加 `matchMedia` 守卫；大图谱移 worker 或限节点数。

### P6 — `lowlight` 编辑器内按需加载

`TipTapEditor.tsx:3,23,95,388` 静态 `createLowlight(common)`。首次遇 code block 再 `import("lowlight")`。需配合 round-trip 测试。

### P7 — `turndown`/`dompurify`/`marked` 延迟到调用点

`src/lib/markdown.ts:1-2`、`src/lib/editor-export.ts:10`、`src/lib/sanitize.ts:1`。函数内 `await import()`。`markdown-vendor`（251 KB）仅在需要时下载。

### P8 — Suspense fallback 兜底

`AppOverlays.tsx:242,297,320` 的 `fallback={null}` → 改 `IrisOverlay` 骨架 + spinner。优先 `ManagementCenterPanel`（67 KB）。

---

## 8. 次要改进（建议跟踪，非 Top 8）

- 预加载 Inter 700 与 JetBrains Mono 400（`index.html:14-27`）。
- 给 `VaultNavigatorBody`、`QuickOpen`、`SearchPanel`、`KnowledgeRelationsPanel`、`UnifiedAssistantPanel`、`AppShell` 加 `React.memo` 边界。
- 核实 `IrisOverlay`（`src/components/ui/iris-overlay.tsx`）是否 trap focus；若否，接 Radix Dialog 或实现焦点陷阱（WCAG 2.4.3）。
- 更新 `docs/design-system.md:59` 失真"Google Fonts 链接"为自托管 woff2 描述。
- 调和 `darkMode: ["class"]` 与 `.light` opt-in 模式；要么文档化"禁用 `dark:` 前缀"，要么改 `darkMode: ["class", '[data-theme="dark"]']`。
- 将 `--warning`、`--classified-accent`、`--status-*` 暴露为 Tailwind 颜色键，统一语义色风格。
- 统一 z-index：废弃裸 `z-10`/`z-[3]`/`z-index:4`，全部走 token。
- `VaultNavigator.tsx:461-466` 的 `prepare` effect 引入 `preparedKeysRef` dedup（仿 `QuickOpen.tsx:99-101`）。
- 提升 `chunkSizeWarningLimit` 至 ~600 或拆分 `index` 以消除每构建警告。
- 统一 tooltip：废弃原生 `title=`，引入共享 tippy 包装组件。
- `SearchPanel` 与 `GraphView` 补"无结果/空库"空状态（仿 `QuickOpen.tsx:236-240`）。
- 半径别名 `2xl`/`3xl` 加注释说明或改名。

---

## 9. 建议迭代编排

| 迭代 | 内容 | 风险 | 预期收益 |
| ---- | ---- | ---- | -------- |
| 迭代 A「冷启动与反馈感」 | P1 + P3 + P4 | 低 | 冷启动感知大幅提升；日常反馈体感改善 |
| 迭代 B「长文档与图谱」 | P2 + P5 | 中 | 长笔记 / 大图谱流畅度 |
| 迭代 C「依赖按需化」 | P6 + P7 + P8 | 中（涉编辑器/导出关键路径，需 round-trip 测试） | bundle 进一步瘦身 |

---

_本报告基于 2026-07-02 时刻的代码快照，仅作优化参考，不构成版本承诺。版本排期以 [ROADMAP.md](./ROADMAP.md) 为唯一来源。_
