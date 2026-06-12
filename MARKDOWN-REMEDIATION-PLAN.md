# Markdown 体系修复 — 修订版执行指导

> 基于 MIMO-2.5-PRO 代码审查报告和对当前仓库实现的二次审查修订。本文档聚焦 Markdown contract、编辑器 ingest/export、TipTap schema、样式和缓存的可执行修复路线。

## 一、总体思路

按 **风险收敛 → 用户可见完善 → 正确性加固 → 工程卫生** 四阶段推进。每个阶段必须先补测试，再实现，再运行对应质量检查。不要把所有修复混成一个大改动；Markdown 权威来源仍是 `.md` 文件，任何导入/导出改动都必须以“不丢失、不静默改写用户原文”为底线。

当前计划修订后的关键校准：

- `marked` 全局实例污染判断成立，必须优先修。
- Callout 可先做纯 CSS 视觉完善，但应使用现有设计 token；新增语义 token 时必须同步 `docs/design-system.md` 和 `src/styles/globals.css`。
- 脚注不是纯 CSS 问题；当前 ingest 需要修正 HTML 结构和属性转义后，再做交互。
- 行内 raw HTML 不建议做成 mark；更稳妥的是新增 inline atom node，或先做安全白名单渲染。
- `fillFragmentGaps` 除了性能问题，还有潜在重复 fragment 的正确性风险，必须先测后修。

---

## 二、阶段划分

### 阶段 A：风险收敛（0.5-1 天）

#### A1. `marked` 实例化重构

**问题本质**：`src/lib/markdown.ts` 调用 `marked.setOptions({ gfm: true, breaks: true })` 修改全局实例；`src/lib/editor-ingest.ts` 和 `src/lib/markdown-contract/contract.ts` 又直接 `import { marked } from "marked"`，行为会被模块加载顺序污染。`src/lib/markdown-render.ts` 已使用独立 `new Marked()`，方向正确。

**策略**：统一创建本项目内部的 marked 实例，禁止业务模块直接使用 `marked` 全局 singleton。

目标结构：

```text
src/lib/markdown.ts
  ├─ createMarkedInstance(options?) -> Marked
  ├─ editorMarked = createMarkedInstance({ gfm: true, breaks: true })
  └─ markdownToHtml / markdownBodyToEditorHtml 使用 editorMarked

src/lib/editor-ingest.ts
  └─ 使用 createMarkedInstance 或共享 editorMarked，不再 import { marked }

src/lib/markdown-contract/contract.ts
  └─ 使用 contractMarked，不再 import { marked }

src/lib/markdown-render.ts
  └─ 保持 proseMarked 独立实例；repairStreamingMarkdown 只负责字符串修补
```

**测试要求**：

- 新增或扩展 `tests/markdown-render.test.ts` / `tests/markdown-contract/contract-profiles.test.ts`，证明 AI 渲染 hooks 不影响 editor ingest 和 contract lexer。
- 搜索断言：除 `markdown.ts` 的工厂实现外，不再出现业务代码 `import { marked } from "marked"`。

#### A2. `isDangerousHtml` 检测范围补全

**策略**：扩展 contract 层的危险 HTML 分类，但不要把它误认为最终安全边界。最终渲染安全仍依赖 `sanitizeHtml` / DOMPurify。

必须覆盖：

- 危险标签：`script|object|embed|iframe|form|applet|style|svg|math|link|meta|base|frame|frameset`
- 内联事件处理器：`on\w+\s*=`
- 明文危险 URL 属性：`javascript:`、`data:text/html` 出现在 raw HTML 属性中时标记为 `unsupported`

**测试要求**：

- 扩展 `tests/markdown-contract/contract-classify.test.ts`：
  - `<svg onload="alert(1)">` → `unsupported`
  - `<img src=x onerror=alert(1)>` → `unsupported`
  - `<style>body{}</style>` → `unsupported`
  - `<kbd>Ctrl</kbd>` 仍不是危险 HTML；后续由 C2 决定保留方式

---

### 阶段 B：用户可见完善（1-2 天）

#### B1. Callout 视觉体系

**问题本质**：Callout ingest 已输出 `blockquote[data-callout-type]`，`CalloutBlockquoteExtension` 也会保留该属性；当前样式层只有普通 `blockquote`，没有按类型区分。

**策略**：先做纯 CSS，不改 callout HTML 和 serialize 逻辑。默认不新增依赖。

实现边界：

- 修改 `src/styles/markdown-prose.css`，为 `note|info|tip|warning|danger|example` 增加类型化背景、左边框、标题行样式。
- 优先使用现有 token：`--primary`、`--destructive`、`--muted`、`--border`、`--editor-*`。如果确实需要 `--success` / `--warning` 等新 token，必须同步修改 `docs/design-system.md` 和 `src/styles/globals.css`。
- 图标不是第一阶段硬要求。若使用 CSS `::before`，只用内联 mask/data URI 或纯文本符号，不新增图标依赖，不改变 DOM 结构。

**测试与验收**：

- 增加 contract/style 测试，断言 `markdown-prose.css` 覆盖 6 种 callout selector。
- 视觉验收：编辑器里 6 种 callout 有可区分的左边框和标题样式；普通 blockquote 不受 callout 类型样式污染。

#### B2. 脚注渲染与交互

**问题本质**：当前 ingest 会生成 `sup[data-footnote-ref]` 和 `p[data-footnote-def]`，但定义内容可能出现 `<p>` 嵌套 `<p>` 的非法结构；label 写入 attribute 时也缺少统一转义。修复顺序必须是 **HTML 结构正确 → 样式 → 点击交互**。

**策略**：保留 `footnote_ref` / `footnote_def` 的 `render_only` 分类，但修正 ingest 产物。

实现要求：

- 在 `src/lib/editor-ingest.ts` 中新增脚注 label 解析 helper，所有写入 attribute 的 label 必须经 `escapeHtml`。
- 脚注定义不要生成嵌套段落。可用 `<section data-footnote-def="label" id="footnote-label">...</section>` 或 `<div data-footnote-def="label">...</div>` 包裹 `marked.parse()` 输出。
- 脚注引用输出稳定 id / href 关系，例如：
  - ref: `<sup data-footnote-ref="label" id="footnote-ref-label"><a href="#footnote-label">[label]</a></sup>`
  - def: `<section data-footnote-def="label" id="footnote-label" data-footnote-return="footnote-ref-label">...</section>`
- 点击跳转优先使用原生 anchor 行为；需要返回引用时再在 TipTap 插件中处理点击事件。不要先引入页面级全局监听。

**测试要求**：

- 扩展 `tests/markdown-contract/contract-ingest.test.ts` 或新增 editor ingest 测试：
  - `Text[^a]\n\n[^a]: Body` 生成可定位 ref/def。
  - 脚注定义 HTML 不包含 `<p data-footnote-def...><p>...` 嵌套。
  - 恶意 label 不会逃逸 attribute。
- 样式验收：脚注引用有明确点击态，定义区域与正文可区分。

---

### 阶段 C：正确性加固（1-2 天）

#### C1. `fillFragmentGaps` 正确性优先，再做性能优化

**问题本质**：`fillFragmentGaps` 使用 `findIndex + splice` 存在 O(n²) 风险，但更重要的是 gap 扫描逻辑需要先证明不重复、不漏片段、offset 连续。`scanTrailingGapForFootnoteDefs` 这类 trailing gap 处理必须特别测试。

**策略**：先补覆盖，再修正确性，最后重构为一次遍历。

测试必须覆盖：

- trailing footnote definition：`Text[^1]\n\n[^1]: Body`
- 多个连续 footnote definition：`[^a]: A\n[^b]: B`
- footnote definition 前后有空白和普通文本 gap
- fragments 按 offset 升序、无重叠、无空洞，且拼接所有 `raw` 后等于 source

实现要求：

- 保留 `MarkdownSyntaxFragment` 对外结构不变，除非 C2 需要新增字段；如果新增字段，必须同步类型注释和相关测试。
- 重构后不要依赖在循环中对 `acc.fragments` 反复 splice；构建新数组后一次性替换并排序。

#### C2. 行内 raw HTML 段落保护

**问题**：文档中出现 `Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.` 时，当前 `raw_html` 是 `preserve_only`，ingest 遇到 preserve fragment 会 `flushNative()` 并插入 block preserve div，导致段落被拆散。

**修订后的策略**：不要把 inline raw HTML 做成 mark。mark 会包裹可编辑文本，不适合“原文不可破坏”的语义。优先选择以下两条路线之一：

1. **推荐路线：新增 `preserveInline` inline atom node**
   - 新建 `src/components/editor/extensions/PreserveInlineExtension.ts`。
   - 节点属性：`originalRaw`、`syntaxKind`；`group: "inline"`、`inline: true`、`atom: true`。
   - parseHTML：`span[data-type="preserve-inline"]`。
   - renderHTML：输出同样 span，`contenteditable=false`，展示简短原文。
   - `editor-pm-serialize.ts` 增加 `preserveInline` node serializer，直接写回 `originalRaw`。

2. **低风险替代：安全 inline HTML 白名单**
   - 对 `kbd|sub|sup|mark|small|abbr` 等安全 inline 标签降级为 native HTML 片段进入 TipTap。
   - 仅当可证明 TipTap/serializer 能稳定回写时使用；否则仍用 `preserveInline`。

分类增强：

- 在 `MarkdownSyntaxFragment` 上新增可选字段 `inline?: boolean`，仅用于 `raw_html` / `html_comment` 等 preserve fragment。
- inline 判断不要只看标签名；还要结合 fragment 在段落中的位置。`<div>`、`<table>`、`<style>`、`<script>` 永远不是 inline preserve。

测试要求：

- `Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.` ingest 后仍是一个段落语义，导出后保留原始 `<kbd>`。
- block raw HTML `<div>raw</div>` 仍使用 `preserveBlock`，不变成 inline。
- dangerous inline HTML `<img onerror=...>` 仍是 `unsupported`，不能进入 inline preserve 正常渲染。

#### C3. 序列化路径契约测试

**问题**：生产保存路径、fallback 路径、contract 测试路径职责不同，不能强行合并；但必须防止语义漂移。

当前路径定义：

- 生产热路径：`serializeOpenNote` → `editorDocToMarkdown` → PM serializer。
- fallback：PM serializer throw 时使用 `editorBodyHtmlToMarkdown(editor.getHTML())`。
- contract/legacy 路径：`editor-export.ts`，文件已标记 deprecated，仅用于 contract 测试与 HTML 片段级导出。

**策略**：建立“语义一致”测试，不要求三条路径字节级完全一致。

实现要求：

- 新增 `normalizeTurndownEscapes()`，集中处理 `.replace(/\\\[/g, "[")` 和 `.replace(/\\\]/g, "]")`。至少 `src/lib/markdown.ts` 与 `src/lib/editor-export.ts` 共享同一 helper。
- 建立 8-12 个代表性 corpus：GFM、callout、脚注、wiki-link、block raw HTML、inline raw HTML、表格、任务列表。
- 对每个 corpus 断言：
  - 生产热路径不丢核心语法。
  - fallback 路径不丢 preserve 原文。
  - deprecated contract 路径只作为兼容参考，不作为生产输出标准。

#### C4. 流式修复扩展

**策略**：仅修高频且可预测的半截语法，避免为了“看起来完整”而伪造复杂结构。

本轮纳入：

- 未闭合图片 `![alt](src`：仅在行尾检测到缺 `)` 时补齐。
- 未闭合链接 `[text](url`：仅在行尾检测到缺 `)` 时补齐。
- 表格行缺尾 `|`：只对已明显是表格行的单行补尾。

暂缓：

- 表格头分隔行缺失：需要状态机，容易误判普通文本。

测试要求：扩展 `tests/markdown-contract/contract-streaming.test.ts`，新增至少 6 个 case，并断言 streaming render 不抛错、不出现明显裸源码噪音。

---

### 阶段 D：工程卫生（0.5-1 天）

#### D1. editor-html-cache 内容摘要

**问题本质**：当前 cache key 只有 note path；同一路径内容变化但未触发 clear 时，可能复用过期 TipTap HTML。

**策略**：缓存值带 digest，并让调用方显式传入当前正文版本。

实现要求：

- `src/lib/editor-html-cache.ts`：
  - `Map<string, { html: string; digest: string }>`
  - `setCachedEditorHtml(path, html, digest)`
  - `getCachedEditorHtml(path, expectedDigest)`：digest 不匹配时返回 `undefined`，并删除陈旧缓存
  - 保持 `MAX_CACHE_SIZE` 和现有 FIFO eviction
- `src/components/editor/TipTapEditor.tsx`：
  - 基于 `initialBodyMarkdown` 计算稳定 digest。可使用轻量同步 hash helper，不新增依赖。
  - 所有 `getCachedEditorHtml` / `setCachedEditorHtml` 调用都传 digest。
  - reloadContentTick 清缓存逻辑保留，作为显式刷新保险。

测试要求：

- 扩展 `tests/editor-html-cache.test.ts`：
  - digest 相同命中。
  - digest 不同 miss 且删除旧值。
  - eviction 行为保持。
- 增加组件级 contract 测试或源码 contract 测试，确保 TipTapEditor 调用 cache API 时传入 digest。

#### D2. 文档与验收补齐

- 若新增 callout token，同步 `docs/design-system.md`。
- 若新增 `preserveInline`，同步 `docs/markdown-export.md`，说明 inline raw HTML 的保留语义。
- 不修改 ROADMAP 的版本承诺，除非该修复改变 alpha 范围。

---

## 三、执行顺序建议

```text
A1 marked 实例隔离
  └─ A2 dangerous HTML 分类
      ├─ B1 callout CSS
      ├─ B2 footnote 结构/样式/交互
      └─ C1 fillFragmentGaps 正确性
           └─ C2 inline raw HTML preserveInline
                └─ C3 序列化契约测试
                     └─ C4 streaming repair 扩展

D1 cache digest 可与 B/C 并行，但要避开 TipTapEditor 同文件冲突。
D2 docs 随对应阶段提交，不单独拖到最后。
```

优先级建议：

1. A1/A2 是 P0/P1 风险收敛，先做。
2. B2 和 C1 都涉及真实数据正确性，优先级高于纯视觉优化。
3. C2 是中等规模 schema 改动，必须单独提交并重点 review。
4. C4 可最后做；表格分隔行状态机暂缓。

## 四、不做清单

- 不添加脚注 library，如 `footnote-js`。
- 不新增 Markdown parser 或替换 TipTap/ProseMirror。
- 不合并生产 PM serializer、Turndown fallback、deprecated contract export 三条路径。
- 不把危险 HTML 交给 editor 直接渲染。
- 不把 inline raw HTML 做成可编辑 mark。
- 不为了 callout 图标新增依赖。
- 不要求三条序列化路径字节级完全一致，只要求语义和 preserve 原文不丢失。

## 五、每阶段完成标准

| 阶段 | 完成标准                                                                                                                                                                                       |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A    | `marked` 全局 singleton 不再被业务模块直接使用；dangerous HTML 新 case 全部覆盖；`npm run lint && npm run typecheck && npm run test -- tests/markdown-contract/contract-classify.test.ts` 通过 |
| B    | callout 6 类型有样式覆盖；脚注 HTML 结构合法、属性转义正确、ref/def 可跳转；相关 ingest/style 测试通过                                                                                         |
| C    | fragments 无 gap/重叠并可拼回 source；inline raw HTML 不拆段且可原样导出；序列化语义 corpus 通过；streaming 新增 case 通过                                                                     |
| D    | cache digest miss 自动失效；TipTapEditor 所有 cache 调用传 digest；相关测试和文档更新通过                                                                                                      |

最终合并前必须运行：

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

如本次修复只改 TypeScript/CSS/文档，Rust 命令仍应在最终声称完成前运行或明确说明无法运行的原因。

## 六、已完成与后续债务

### 已完成

- 已隔离 `marked` 使用路径，生产编辑器、contract、AI 渲染不再共享全局 singleton。
- 已补强 dangerous HTML 分类、callout 样式、footnote 结构、inline raw HTML preserveInline、cache digest、序列化路径契约与 streaming repair 覆盖。
- 已新增仓库文本卫生约束，固定 LF 行尾并清理已转绿测试名中的历史 `[TDD-FAIL]` 标签。
- 已把 deprecated contract export 标注为 contract-only，生产保存/重新打开路径继续走 PM serializer。

### 后续债务

- 持续把 `src/lib/markdown-contract/contract.ts` 中的分类与 token walk 逻辑拆成更小模块，每次只移动代码并保持 contract 测试绿。
- 补充真实浏览器/Playwright 验收，覆盖 callout 视觉、footnote anchor、preserveInline 段落保持和 editor 保存重新打开。
- 为 preserveInline 与 footnote atom 扩展复制、删除、撤销、键盘选中测试，防止 `originalRaw` 被半编辑破坏。
- 继续扩充 `repairStreamingMarkdown` golden corpus，只纳入有真实失败样例支撑的修复，不为表格头分隔行预造状态机。
