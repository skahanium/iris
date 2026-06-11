# Markdown 体系修复 — 战略指导方案

> 基于 MIMO-2.5-PRO 代码审查报告的修复计划。原始报告覆盖 12 项发现（P0-P2），本文档制定分阶段修复策略。

## 一、总体思路

按**风险收敛 → 功能完善 → 质量加固**三阶段推进。每一阶段的产出能独立验证，不相互阻塞。

---

## 二、阶段划分

### 阶段 A：风险收敛（0.5-1 天）

#### A1. marked 实例化重构

**问题本质**：三个文件各自 `import { marked } from "marked"`，共享同一个全局实例，`markdown.ts:105` 的 `setOptions()` 污染了不相关的调用方。

**策略**：**统一工厂，各自创建独立实例**。

```
现状:  markdown.ts → marked.setOptions()  (全局污染)
      editor-ingest.ts → marked.parse()   (受污染)
      contract.ts → marked.parse()        (受污染)
      markdown-render.ts → new Marked()   (正确)

目标:  markdown.ts → createSharedMarked()  → 实例 A (breaks:true, 无 hooks)
      editor-ingest.ts → 复用实例 A
      contract.ts → 复用实例 A（或按需创建）
      markdown-render.ts → 保持独立的 proseMarked
```

- 在 `markdown.ts` 中导出一个 `createMarkedInstance(options)` 工厂函数
- `markdown.ts` 自身调用工厂获取实例，而非操作全局
- `editor-ingest.ts` 和 `contract.ts` 改为通过工厂获取实例
- `repairStreamingMarkdown` 保持现有独立 `proseMarked`

**不做什么**：不需要修改 `repairStreamingMarkdown` 本身，它的 `breaks: true` 和 hooks 是正确的。

#### A2. isDangerousHtml 检测范围补全

**策略**：扩展正则，补全遗漏的危险标签种类。

- 添加 `style|svg|math|link|meta|base|frame|frameset`
- 考虑添加对内联事件处理器（`on\\w+\\s*=`）的检测
- 添加对应的测试用例：`<svg onload=...>` 应标记为 unsupported

**风险平衡**：不要过度追求完整——DOMPurify 在渲染层提供最终防线，contract 层的分类不追求 100% 精确，但应覆盖已知攻击面。

---

### 阶段 B：功能完善（1-2 天）

#### B1. Callout 视觉体系

**问题本质**：Callout 的 HTML 结构已正确生成（`data-callout-type="note|warning|tip|danger|info|example"`），但零行 CSS。

**策略**：**纯 CSS 方案，不动 HTML 结构**。

在 `markdown-prose.css` 中按 callout 类型添加样式，核心思路：

1. **颜色语义化**：6 种类型各配一套前景色/背景色/左边框色（基于 shadcn/ui 的 color token，如 `--success`、`--warning`、`--destructive`）
2. **图标**：用 CSS `::before` 伪元素在标题行前插入对应 SVG 图标（用 `mask-image` + `background-color` 实现，无需额外 DOM）
3. **选择器**：`blockquote[data-callout-type="note"]` → 蓝色系；`[data-callout-type="warning"]` → 黄色系，以此类推
4. **间距**：标题行加粗 + 与正文间 0.5em 间距

**不需要做的事**：不需要修改 CalloutBlockquoteExtension、不需要修改 ingest 或 serialize。HTML 产出完全不变，纯样式层。

#### B2. 脚注渲染完善

**问题本质**：脚注引用是裸 `<sup>`，定义是裸 `<p>`，无交互或样式。

**策略**：**CSS + 轻量 JavaScript 交互**。

分三步：

1. **基础样式**（CSS 层，5 分钟）
   - `sup[data-footnote-ref]` → 上标蓝色点击态样式，cursor pointer
   - `p[data-footnote-def]` → 在文档末尾作为脚注区域，浅色背景 + 左边框

2. **点击滚动**（5 行 JS）
   - 给 `data-footnote-def` 元素加 `id="footnote-{label}"`
   - 点击 `data-footnote-ref` 时 `scrollIntoView` 到对应定义
   - 点击定义标签时返回原文引用位置

   实现位置有两种选择：
   - **方案一（推荐）**：在 TipTap 编辑器内通过 ProseMirror plugin 处理（最自然）
   - **方案二**：在页面级监听 `click` 事件委托

3. **弹窗预览（可选，后续版本）**
   - 鼠标悬停时弹出脚注预览 tooltip
   - 可作为 P2 后续优化

**不需要做的事**：不需要修改 ingest 的分级逻辑（`render_only` 是正确的）。

---

### 阶段 C：质量加固（1-2 天）

#### C1. 行内 raw HTML 段落保护

**问题**：当文档中出现 `<kbd>Ctrl</kbd>` 时，preserve_only 分类导致段落被 `flushNative()` 打断成多段。

**策略**：**区分 inline raw HTML 和 block raw HTML，inline 级别不放 preserve-block div**。

关键改动在 `editor-ingest.ts` 的 fragment 分发循环中：

1. **分类增强**：在 `contract.ts` 中给 `raw_html` 片段增加 `isInline` 属性（判断依据：HTML 标签是否包裹在文本行内，即 non-block-level element）
2. **ingest 处理**：inline raw HTML 不再触发 `flushNative()` + `preserveBlockDiv()`
   - 改为在 `nativeBuf` 中累积为 `<span data-type="preserve-inline" data-original-raw="...">`
   - 这个 span 在 PM 中渲染为 PreserveBlock mark（而非 block node），保持段落的连续性
3. **serialize 处理**：回写时 PreserveBlock mark 的原始内容原样输出到段落中的正确位置

**风险点**：需要给 PreserveBlockExtension 添加 inline mark 变体，不能沿用现有 block node view。

#### C2. 序列化路径收敛

**问题**：PM serializer、Turndown 回退、contract 测试路径三者输出可能不一致。

**策略**：**不要合并路径，要建立契约测试**。

三条路径各司其职：
- PM serializer：正常文档保存（首选，最快）
- Turndown 回退：PM 无法处理的边缘情况
- 纯 Turndown 路径：contract 测试独立验证

**具体措施**：

1. **Golden file 测试**：选取 10-15 个代表性 Markdown 文档（含 GFM + callout + 脚注 + wiki-link + 行内 HTML），分别通过三条路径序列化，将输出保存为 golden file，CI 中做 diff 检测
2. **差异分类**：将允许的差异（如空白规范化）列入白名单；将不允许的差异（如语法结构变化）视为回归
3. **Turndown 转义替换规范化**：将 `.replace(/\\\[/g, "[")` 的逻辑收归一个 `normalizeTurndownEscapes()` 函数，在三个路径中使用同一份规范化逻辑，避免各路径用不同手段绕过 Turndown 转义

#### C3. 流式修复扩展

**策略**：按语法难度排序，逐类覆盖。

| 语法 | 策略 | 难度 |
|------|------|------|
| 未闭合图片 `![alt](src` | 检测无闭合 `)` → 补 `)` | 低 |
| 未闭合链接 `[text](url` | 检测无闭合 `)` → 补 `)` | 低 |
| 未闭合表格行 | 检测行末无 `\|` 但有 `\|` 开头 → 补 `\|` | 低 |
| 中断表格头分隔行 | 当上一行有 `\|` 时补 `\| -- \| -- \|`，更复杂需要状态机 | 中 |

**优先级**：先做图片和链接（高频且简单），表格行（中频且简单），表格分隔行（低频且复杂，可推迟）。

---

### 阶段 D：工程卫生（0.5 天）

#### D1. fillFragmentGaps 性能优化

**策略**：预分配 + 一次遍历替代 findIndex + splice。

```
现状: for (gap) { findIndex(O(n)) → splice(O(n)) }  → O(n²)
改进: 一次遍历构建新的片段数组，合并间隙 → O(n)
```

改动局限在 `fillFragmentGaps` 函数内部，无外部接口变化。但鉴于函数逻辑边界复杂（空格间隙、脚注定义、前后空白），需要**密集的单元测试覆盖现有行为后**再重构。

#### D2. editor-html-cache 内容哈希

**策略**：将 `Map<string, string>` 改为 `Map<string, { html: string; digest: string }>`。

- `setCachedEditorHtml(path, html, digest)` — 存储时记录内容摘要
- `getCachedEditorHtml(path, expectedDigest?)` — 可选校验：摘要不匹配时返回 undefined
- 调用方（switch-tab 逻辑）在请求缓存时传入当前文件的 `sha256` 或 `mtime` 作为摘要
- 摘要不匹配时自动失效，无需调用方显式 clear

**不做什么**：不引入 LRU 库。现有的手动 eviction 逻辑保持。

---

## 三、执行顺序建议

```
A1 (marked 重构)  ──+
A2 (安全加固)    ──+──→ 并行推进，无依赖
                    │
B1 (callout CSS) ──+
B2 (脚注渲染)    ──+──→ 并行推进，仅依赖 A1（工厂函数）
                    │
C1 (行内 HTML)    ──┐
C2 (序列化归一)   ──+──→ 顺序推进，C3 可选
C3 (流式修复)     ──┘
                    │
D1 (性能)         ──+
D2 (缓存)         ──+──→ 并行推进，无依赖
```

## 四、不做清单

- **不添加脚注 library**（如 `footnote-js`），功能简单不需依赖
- **不修改 callout HTML 结构**，现有 `data-callout-type` 已足够 CSS 选择
- **不合并三条序列化路径**，它们的职责不同
- **不过度扩展 Wiki-link**，保持 alpha 范围，在 ROADMAP 中记录即可
- **不添加 inline raw HTML 的完整编辑支持**（仅做段落保护），完整支持属于独立 feature

## 五、每阶段完成标准

| 阶段 | 完成标准 |
|------|----------|
| A | `cargo clippy` + `npm run lint && typecheck` 零错误；`marked` 全局实例不再被外部文件使用 |
| B | 可视化验收：callout 有明显颜色/图标区分；脚注引用可点击跳转 |
| C | Golden file 新增 ≥10 个；`repairStreamingMarkdown` 新增 ≥6 个测试 |
| D | `fillFragmentGaps` 重构后所有已有测试通过；缓存摘要校验测试通过 |
