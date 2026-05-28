# 路线图

Iris 采用里程碑式版本规划。每个版本对应一个完整的功能集，不绑定具体日期。

版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。v0.x 阶段 API 可能变动。

### 产品原则与非目标

Iris 是**桌面端、单用户、本地优先**的 Markdown 笔记应用。下列能力为**永久非目标**（不排期、不在「未来探索」中预留评估）：

| 类别     | 永久不做                                     |
| -------- | -------------------------------------------- |
| 扩展生态 | 第三方插件 / 插件市场 / 应用内加载社区扩展包 |
| 平台     | 移动端（iOS / Android）、Tauri 移动靶        |
| 协作     | 实时多人协作、CRDT 同步（如 Yjs）            |
| 输入形态 | 语音转文字笔记、手写笔迹                     |
| 外围产品 | Web Clipper 浏览器扩展                       |

**扩展方式**（仅此几种）：

- **主线功能**：新能力通过版本里程碑交付。
- **AGPL 源码**：深度定制请 fork 或向上游提 PR。
- **声明式配置（规划中）**：内置 `/` 命令模板、主题 CSS 变量、快捷键等。
- **Vault 外工具链**：笔记为 `.md` 纯文本，可用任意编辑器、脚本、Git 处理；不在 Iris 进程内加载第三方代码。

原 **v0.4.0「插件系统」** 里程碑已删除。

### 体验方向（与路线图绑定）

产品与界面的长期取向：**主攻 [Notion 编辑（N）](docs/design-system.md#n--notion-编辑主方向)**；**备选 [命令优先（C）](docs/design-system.md#c--命令优先备选原则)**（键盘导航、可收起 AI 侧栏，不常驻占宽）。实现细则、token 与组件规则见 **[docs/design-system.md](docs/design-system.md)**；交互线框见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

各版本**功能里程碑**与**界面交付**一并规划（避免「功能路线图」与「设计稿」两套叙事）：

| 版本          | 功能重心              | 体验 / 界面交付                                                                                                                               |
| ------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| **v0.1.0**    | AI 原生 MVP（已发布） | 可用型 UI（通用深灰 + 紫色 accent），未按纸墨定稿                                                                                             |
| **v0.1.1**    | 设置与质量补齐        | **纸墨阶段 0**：chrome/纸面 token、衬线编辑区、`Ctrl+Shift+A` 收起 AI；见下方 checklist                                                       |
| **v0.2.0**    | 知识网络 + sqlite-vec | **纸墨阶段 1**：引用卡 / 关联笔记芯片 / `/` 菜单；图谱与标签视图符合纸墨层级                                                                  |
| **v0.3.0**    | 安全与版本            | 版本时间线、diff、模板库等**低干扰**纸面组件                                                                                                  |
| **v0.3.1-ui** | （仅体验）            | 命令浮层基础设施（部分并入 v0.4.0-ui）                                                                                                        |
| **v0.4.0-ui** | （仅体验）            | **Notion 扁平编辑**：去行线/纸页、Inter、蓝灰 accent — 见 [plans/2026-05-27-notion-ui-rebuild.md](docs/plans/2026-05-27-notion-ui-rebuild.md) |
| **v0.5.0**    | **AI 建设 MVP**       | 场景化 AI 面板、证据包引用卡、工具确认弹窗、AI 记忆与规则设置                                                                                 |
| **v1.0.0**    | 稳定与合规            | 体验收尾（可选）：标签栏自动隐藏；WCAG 与高对比主题                                                                                           |

**体验非目标**（与产品非目标并列）：第三方主题/插件换肤、紫色渐变 AI 套路、聊天主屏化。详见 design-system「非目标（视觉）」。

### AI 差异化方向（已并入 v0.5.0）

下列 Iris 差异化方向不再作为泛化探索项，统一纳入 **v0.5.0 — AI 建设 MVP**。实现时必须服从本地优先、Markdown 权威、写入确认、无第三方插件运行时等产品原则：

- **AI 自动标签与分类**：基于笔记全文与 vault 上下文，由 LLM 建议或写入 frontmatter / `#tag`；写入必须用户确认，可关闭，不依赖第三方插件。
- **AI 证据化问答**：从 Top-K 拼接升级为可追溯 `ContextPacket` 证据包，回答必须能回查来源。
- **AI 写作工作流**：围绕当前草稿做结构建议、改写、法规引用建议和一致性检查，但所有 `.md` 写入必须 diff / 预览确认。

---

## v0.1.0 — AI 原生 MVP

**目标**: 证明 AI 不是笔记软件的附加功能，而是编辑器的一部分。

**状态**: 已于 2026-05-25 发布（见 [CHANGELOG.md](./CHANGELOG.md)）。下列 `[x]` 与 v0.2.0 仓库状态一致。

### 核心功能

#### 编辑器

- [x] 基于 TipTap (Prosemirror) 的 Markdown 所见即所得编辑器
- [x] 语法高亮、表格、任务列表、代码块（**核心 GFM**，非完整规范；见 `src/components/editor/gfm-schema.ts`）
- [x] 文件管理（Sheet / 快捷键）：创建、重命名、删除 `.md` 文件（`Ctrl+P` Quick Open，`Ctrl+Shift+E` 文件树 Sheet）
- [x] 多标签页编辑器（同时打开多篇笔记）
- [x] 暗色 / 亮色主题切换（v0.1.1 起对齐[纸墨设计系统](docs/design-system.md)）

#### AI 集成

- [x] **内联 AI**：选中文字 → 改写 / 扩写 / 翻译 / 简化，结果在编辑器内以可操作节点呈现（接受 / 重试 / 回退）
- [x] **`/` 命令唤起**：输入 `/` 弹出 AI 命令菜单（总结、生成大纲、头脑风暴、修复语法等），结果写入 `ai-stream` 流式节点
- [x] **流式渲染**：LLM 返回的 token 实时渲染，不等待全文返回
- [x] **上下文问答面板**：基于当前笔记与语义 Top-K **关联笔记**注入上下文（`search_semantic`，排除当前文件）
- [x] 多提供商支持：OpenAI、Anthropic Claude（Messages API）、Ollama（本地模型）、自定义 OpenAI 兼容（侧栏与内联、`/` 共用当前 provider）
- [x] **联网搜索**（可选）：Bing API Key（凭据 `iris/bing-search`）或 DuckDuckGo 降级
- [x] API Key 通过操作系统凭据管理器安全存储（LLM Key + Bing 搜索 Key）

#### 存储

- [x] 笔记目录管理（用户指定目录，递归读取 `.md` 文件）
- [x] SQLite 元数据索引（文件路径、标题、**frontmatter JSON**、**frontmatter tags**、更新时间）
  - v0.1 标签仅来自 YAML frontmatter；正文 `#tag` 见 v0.2
- [x] 文件监听（检测外部编辑器对 `.md` 文件的更改；**切换 vault 后自动重建监听**）

#### 搜索

- [x] 全文关键词搜索（SQLite FTS5）
- [x] **向量语义搜索**（v0.1：`fastembed` + `chunk_embeddings` BLOB + Rust 余弦 Top-K；非 sqlite-vec 虚拟表，见 [docs/eval/semantic-search.md](docs/eval/semantic-search.md)）

### v0.1.0 未纳入（延至 v0.1.1 / v0.2）

**功能**

- [x] 自定义 OpenAI 兼容 **API Base URL** 设置界面（IPC 字段 `custom_base_url` 已预留）→ **v0.1.1 已实现**
- [ ] Playwright / WebDriver **全链路 E2E**（v0.1 以 Vitest 场景占位 + 手工验收为准）→ **推迟**
- [x] **sqlite-vec 虚拟表**语义索引 → **v0.2 已实现**

**体验（纸墨 B + 命令 C 基础）** → 详见 **v0.1.1** 与 [design-system 阶段 0](docs/design-system.md#落地阶段与路线图版本对照)

### 验收标准

- [x] 用户可以在 Iris 中创建、编辑、删除笔记
- [x] 用外部编辑器修改 `.md` 文件后，Iris 可以检测并提示同步
- [x] 内联 AI 的接受/重试/回退操作正常工作
- [x] 语义搜索的 Top-5 召回率达到可用水平（Recall@5 ≥ 0.6，fixture 评测见 [docs/eval/semantic-search.md](docs/eval/semantic-search.md)）
- [x] API Key 不落盘明文，存储于操作系统凭据管理器

---

## v0.1.1 — 体验定稿与质量补齐

**目标**: 在 v0.1 功能闭环之上，落实纸墨视觉与发布前质量项；不新增大型功能域。

### 体验（纸墨 · 阶段 0）

- [x] 设计宪章 [docs/design-system.md](docs/design-system.md) 与路线图版本对照
- [x] chrome / 纸面 token、赭石 accent、编辑区衬线栏宽（`globals.css`）
- [x] AI 侧栏 `Ctrl+Shift+A` 收起（命令优先 C 基础）
- [x] 全库文档索引与交叉引用（[docs/README.md](docs/README.md)）
- [x] 全应用组件扫尾：对话框 / Quick Open / 搜索面板统一 chrome 层级
- [x] 设置区收纳 API Key（减少侧栏表单感，仍内置非插件）

### 功能与质量

- [x] 自定义 OpenAI 兼容 **API Base URL** 设置界面
- [ ] Playwright / WebDriver **全链路 E2E**（可选）

### 验收标准

- [x] 默认启动为「深灰外壳 + 暖纸编辑区」，与 design-system 截图级一致
- [x] 无紫色主色；AI 面板收起后编辑区可视宽度明显增加
- [x] `npm run lint` / `typecheck` / `test` 与 Rust CI 保持通过（v0.2.0 验证通过，见 [CHANGELOG](./CHANGELOG.md)）

---

## v0.2.0 — 知识网络

**目标**: 从单篇笔记到笔记之间的连接，建立个人知识图谱。

### 核心功能

- [x] **`[[双向链接]]`**：支持 `[[笔记标题]]` 语法，自动维护链接关系
- [x] 反向链接面板（显示所有链接到当前笔记的笔记）
- [x] **知识图谱可视化**：基于链接关系的力导向图
- [x] 标签系统：`#标签` 语法，标签聚合视图
- [x] 链接自动补全（输入 `[[` 时弹出笔记列表）
- [x] 统计面板：笔记数量、链接数、标签数、写作字数趋势

#### 体验与界面（纸墨 · 阶段 1）

与 [design-system 阶段 1](docs/design-system.md#落地阶段与路线图版本对照) 同步，且服从知识网络信息架构：

- [x] **引用卡**完整形态（章节 meta、「仅此次」、移除引用）— 对齐 ARCHITECTURE 线框
- [x] **关联笔记**以上下文芯片展示，不采用聊天气泡主视觉
- [x] **`/` 命令菜单**纸墨样式（非系统默认下拉）
- [x] **反向链接 / 图谱 / 标签聚合**：chrome 外壳 + 纸面或中性面板，不引入高饱和装饰

#### 搜索与索引（中后期 MVP · 语义检索升级）

v0.1 已用 `fastembed` + `chunk_embeddings` BLOB + Rust 全量余弦（见 [docs/eval/semantic-search.md](docs/eval/semantic-search.md)）。笔记与链接增多后，在 **v0.2** 引入 **sqlite-vec 虚拟表**，把向量检索从应用层暴力扫描迁回数据库侧近似 Top-K：

- [x] **sqlite-vec 集成**：`vec0` 虚拟表存储 chunk 嵌入（384 维），`002_vec.sql` migration
- [x] **增量 migration**：自 `chunk_embeddings` BLOB 回填虚拟表；保留 BLOB 作回退
- [x] **`search_semantic` 改造**：优先 sqlite-vec 向量 Top-K；vec 不可用时降级 cosine fallback
- [ ] **性能验收**：万级 chunk 下语义查询 P95 明显优于 v0.1 全表扫描
- [ ] **Recall 回归**：fixture 与抽检 Recall@5 不低于 v0.1 基线（≥ 0.6）

**sqlite-vec 的作用（简述）**：在单文件 SQLite 内做向量近似最近邻，避免每次把全部 embedding 读入 Rust 算余弦；适合 vault 变大后的语义搜索与 AI 关联笔记（Top-K）场景。

### 验收标准

- [ ] 双向链接在编辑器和索引数据库之间保持一致性
- [ ] 图谱渲染 500+ 节点时保持流畅（>30fps）
- [ ] 重命名笔记时，所有指向它的链接自动更新
- [ ] sqlite-vec 语义检索上线且 Recall@5 不低于 v0.1 基线（见语义检索升级小节）

---

## v0.3.0 — 安全与版本

**目标**: 数据安全不妥协，让专业用户放心使用（单用户；不含实时多人协作）。

### 核心功能

- [x] **笔记目录加密**：可选 AES-256-GCM 加密，应用启动时输入密码解密 → **推迟至 v0.3.1**（crate 已就绪，安全审计待做）
- [x] 文件外部修改冲突解决：三方 diff 视图，用户抉择保留哪个版本
- [x] **版本记录系统**（设计：[docs/plans/2026-05-26-document-version-design.md](docs/plans/2026-05-26-document-version-design.md)）：
  - **双层保存**：编辑防抖写 `.md`（默认 1.2s）；版本快照与写盘解耦
  - **手动版本**：`Ctrl+S` → `version_save_manual`（`kind=manual`）
  - **空闲自动备份**：打开文档连续 10 分钟无编辑 → `auto_idle`（每篇最多 30 条）
  - **定稿**：对当前正文新建快照（`version_finalize_current`），永久保留，可选名称
  - **自动清理**：启动时删除 7 天前的 `auto_idle` 未定稿快照；定稿不自动删
  - **存储**：元数据在 SQLite（含 `kind`），正文在 `.iris/versions/<file_id>/`
  - **恢复**：双栏对比 + 确认；恢复前强制 `pre_restore`，失败则不覆盖当前正文
  - **时间线**：定稿区置顶；按日分组；「自动备份（N）」默认折叠
- [x] 笔记模板系统：会议纪要、读书笔记、项目复盘等模板库
- [x] 文件导出：Markdown、HTML 格式导出（PDF 推迟，HTML 可浏览器打印为 PDF）
- [ ] 图片拖拽插入/粘贴，本地图片管理 → **推迟至 v1.0**

#### 体验与界面

- [x] 版本时间线（折叠自动备份、双栏对比、定稿入口）、模板选择器沿用纸墨 token
- [x] 新建文档命名：`新建文档` / `新建文档（1）`…；标签栏与状态栏展示 `files.title`

### 验收标准

- [ ] 加密目录在未解锁时外部编辑器打开为乱码
- [ ] 冲突解决的 L1（静默同步）/ L2（提示合并）/ L3（用户抉择）三层策略生效
- [x] 版本历史可预览、确认后恢复；恢复前自动保留 `pre_restore` 快照
- [ ] 版本历史可以精确回退到任意时间点（依赖用户主动保存的版本节点，非 Git 式逐字修订）

---

## v0.3.1-ui — 命令浮层与纸墨抛光

**目标**：在 v0.3.0 功能闭环之上，将次级 UI 从「右侧贴边长条」升级为**居中命令浮层**，并统一纸页视口、暗色护眼纸、圆角动效与 Chrome/AI 视觉；**不新增后端/IPC 能力**，不与 v1.0 功能开发抢排期。

**设计宪章**：[docs/design-system.md](docs/design-system.md)（命令浮层、纸页视口、圆角与动效）  
**施工计划**：[docs/plans/2026-05-26-ui-overlay-refresh.md](docs/plans/2026-05-26-ui-overlay-refresh.md)

### 产品决策（已定稿）

- 打开命令浮层时 **保持 AI 侧栏**，全窗蒙层 dim（不裁切编辑区宽度）
- **同时仅一个** 命令浮层
- 纸页 **固定视口高度、仅纸内滚动**（纸边常显）
- 暗色主题 **暗暖灰纸 + 浅字**（方案 A）
- 圆角 **14–20px**；版本浮层近全屏、图谱几乎全屏

### Checklist

#### 基础设施

- [ ] `IrisOverlay` 组件与 `size` 变体（`compact` / `command` / `wide` / `graph`）
- [ ] `useOverlayManager` 单一浮层互斥
- [ ] CSS token：`--radius-*`、`--shadow-*`、`--motion-*`、暗纸 `--editor-paper`
- [ ] 废弃贴边 `SidePanel` 形态

#### 面板迁移（居中浮层）

- [ ] Quick Open — `compact`
- [ ] 文件 / 搜索 / 设置 / 反链 / 标签 — `command`
- [ ] 版本时间线 — `wide`（双栏近全屏）
- [ ] 知识图谱 — `graph`（几乎全屏）

#### 编辑纸页（已由 v0.4.0-ui 取代，不再验收纸墨纸页）

- [ ] ~~`.iris-paper` 定高 + 纸内滚动~~ → 取消
- [ ] ~~暗色护眼纸~~ → 取消
- [ ] ~~空文档纸页~~ → 取消

#### Chrome + AI

- [ ] `TabBar` / `StatusBar` 圆角与动效
- [ ] `AiPanel` 对话泡与引用卡翻新
- [ ] `FloatingToolbar`、基础 `button` / `input` / `dialog` token 对齐

### 验收标准

- [ ] AI 展开时 `Ctrl+Shift+F/V/G` 等为居中浮层，编辑区宽度不变
- [ ] 同时仅一个命令浮层；`Esc` / 点蒙层可关闭
- [ ] 纸页首屏满高、仅纸内滚动；暗色纸面护眼且正文对比度 ≥ 4.5:1
- [ ] `pnpm run lint` / `typecheck` / `test` 通过

---

## v0.4.0-ui — Notion 扁平编辑

**目标**：放弃纸墨/信纸视觉，统一 Notion 式扁平灰阶编辑体验；**不新增后端/IPC**。

**设计宪章**：[docs/design-system.md](docs/design-system.md)  
**施工计划**：[docs/plans/2026-05-27-notion-ui-rebuild.md](docs/plans/2026-05-27-notion-ui-rebuild.md)

### Checklist

- [x] Token：Inter、蓝灰 accent、小圆角、去 `--shadow-paper`
- [x] 编辑区：`.iris-editor-canvas`，无行线、无纸页卡片、左对齐主标题
- [x] Chrome：TabBar / StatusBar / Welcome / AiPanel 对齐 N token
- [ ] 命令浮层组件（若 v0.3.1-ui 未完成）样式对齐
- [ ] 亮/暗对比度与缩放/目录/Zen 手动验收

### 验收标准

- [ ] 编辑器无横线网格、无段首缩进、无浮动纸阴影
- [ ] `pnpm run lint` / `typecheck` / `test` 通过

---

## v0.4.1-ui — Chrome 现代化

**目标**：命令面板、AI 侧栏及全 Chrome 达到 Notion / Vercel 级现代化观感；**不新增后端/IPC**。

**设计宪章**：[docs/design-system.md](docs/design-system.md)（Chrome 控件选型、表面 token）

### Checklist

#### Token 与基础设施

- [x] `--surface-*`、`--command-highlight-*`、`--ai-*` token 与 Tailwind 映射
- [x] 共享原语：`OverlayChrome`、`CommandListOption`、`Kbd`、`AiComposer`、`AiMessage`、`SurfaceCard`

#### 命令与导航

- [x] `CommandPalette` / `QuickOpen`：统一浮层壳、图标、匹配高亮、空状态
- [x] 其余 `IrisOverlay` 面板间距与顶栏对齐

#### AI 侧栏

- [x] `AiPanel` 拆分；`SceneSelector` + `AiComposer`；证据卡/工具卡 token 化
- [x] 去重复 `SCENE_OPTIONS`、去 emerald/purple 主视觉（核心 AI 面板；ResearchPanel 等待 v0.5）

#### 编辑器 Chrome

- [x] `SlashCommandList`：Lucide + `CommandListOption`
- [x] `FloatingToolbar`：pill 组 / 更多菜单

#### 壳层

- [x] `TabBar` / `StatusBar`（缩放 popover）/ `WelcomeEmpty`

### 验收标准

- [ ] 亮/暗主题命令面板与 AI 手动走查；`prefers-reduced-motion` 无阻塞动画（需人工）
- [ ] `pnpm run lint` / `typecheck` / `test` 通过

---

## v0.5.2 — LLM 连接统一

**目标**：四场景厂商/模型路由、统一凭据与 Base URL、底栏 LLM/搜索 API 连通性指示、长上下文预算与 DeepSeek 前缀缓存可观测。

- [x] `settings.llm_routing` + `llm::config::resolve_for_scene`
- [x] 设置页 AI 连接（`LlmRoutingSection`）与 `llm_config_*` IPC
- [x] 底栏 `ConnectivityIndicators` + `connectivity_status`
- [x] 动态 token 预算、`long_context` 笔记全文注入、分层 messages
- [x] DeepSeek V4 catalog、usage 缓存写入 `llm_usage_last`
- 文档：[docs/llm-routing.md](docs/llm-routing.md)

## v0.5.1 — 语料库与范围检索

**目标**：在命令浮层（非侧边栏文件树）前提下，用 `.iris/corpora.toml` 声明场景默认语料库，并在 AI 对话中通过 `@` 指定文件夹/文档检索范围。

### 体验清单

- [x] 命令面板：底栏常驻 `Ctrl+Shift+P`；列表移除「打开命令面板」自指项；边界 `↑↓` 滚动不闪烁
- [x] `corpora.toml` 解析、`corpus_list` / `corpus_upsert` IPC、`RetrievalScope` 路径过滤
- [x] AI 输入 `@` 悬浮补全（文件夹/文档）、Scope 芯片、`context_assemble` / `ai_send_message` 传 `contextScope`
- [x] `VaultNavigator` 树形浮层（由原 FileSheet 演进）、「设为语料库」写回配置
- [x] 法规索引仅对 `kind=regulation` 语料路径；场景默认 corpus 与 exemplar 模板层联调

---

## v0.5.0 — AI 建设 MVP

**目标**：把 v0.1 的内联 AI 与上下文问答升级为 Iris 的本地优先 AI 工作流系统。v0.5.0 只做可治理、可评测、可确认的 AI MVP，不做长期自治代理。

**体系设计**：[docs/superpowers/specs/2026-05-27-ai-system-design.md](docs/superpowers/specs/2026-05-27-ai-system-design.md)

### 产品原则

- **Workflow 优先**：知识查阅、文稿学习、文稿创作采用确定性工作流；研究场景仅保留有限 agentic loop 的设计，不作为 MVP 必交。
- **Rust Runtime 优先**：上下文组装、检索规划、模型调用、工具权限、trace / eval 放在 Rust 后端；React 负责展示、流式渲染和确认交互。
- **证据包优先**：AI 不再只拼接 Top-K 片段，而是基于带来源、hash、span、score 的 `ContextPacket` 回答。
- **Markdown 权威**：`.md` 文件仍是唯一知识资产；SQLite 中的 AI 索引、锚点、会话和收件箱均为缓存或用户可删除状态。
- **写入确认**：任何 frontmatter、正文、规则、模板、标签写入都必须由用户确认。

### MVP 核心功能

#### AI Runtime 基础

- [ ] `ai_runtime` Rust 模块：`scene_router`、`context_planner`、`retrieval_broker`、`packet_builder`、`model_gateway`、`tool_executor`、`guardrails`、`trace`
- [ ] Model capability registry：`fast` / `writer` / `reasoner` / `long_context` / `embedding` / `local_private` 槽位，不硬编码具体商业模型
- [ ] 工具权限系统：`read_index`、`read_note_span`、`network`、`write_cache`、`write_markdown`、`write_settings`
- [ ] 工具调用可观测：AI 面板展示检索、证据包、工具调用、确认状态
- [ ] Prompt injection 基础防护：检索材料不能覆盖 system / tool policy；工具参数由 Rust schema 校验

#### ContextPacket 与检索

- [ ] `ContextPacket` 数据结构：`source_type`、`source_path`、`source_span`、`content_hash`、`retrieval_reason`、`score`、`trust_level`、`citation_label`
- [ ] Hybrid retrieval：FTS + sqlite-vec + 显式链接 / 标签分数融合
- [ ] 证据包引用卡：AI 回答可展示引用来源、标题、片段和相关度
- [ ] Citation verifier：回答引用必须能回查证据包；缺证据时标注材料不足
- [ ] 现有 `search_semantic` 保持兼容，作为 hybrid retrieval 的底层能力之一

#### 知识索引 MVP

- [ ] 稳定语义锚点：使用 `anchor_key`，禁止把 SQLite rowid 暴露为 Markdown 块级链接
- [ ] AI 标签 / 文种建议：由 AI 建议 frontmatter / `#tag` 或 `files.genre`，写入必须确认
- [ ] 法规条款索引 MVP：Rust parser 切分条 / 款 / 项，LLM 只做关键词和摘要，不决定条款边界
- [ ] 文种模板提取 MVP：从用户确认的范文中提取结构、常用表达、风格特征
- [ ] 删除 SQLite 后，AI 派生索引可从 `.md` 重建

#### 场景化 AI 面板

- [ ] `SceneSelector`：知识查阅、文稿学习、文稿创作、学术研究四个场景
- [ ] `ContextStatusBar`：显示当前使用的笔记、证据包数量、联网状态和索引状态
- [ ] `ContextPacketCard`：显示可展开的证据包引用卡
- [ ] `ToolConfirmDialog`：写入 `.md`、规则、模板、标签前展示 diff / 预览
- [ ] `RuleConfirmDialog`：用户说“以后都这样”时，提取规则并二次确认

#### 文稿创作 MVP

- [ ] 当前草稿上下文组装：优先光标邻域 + 大纲摘要 + 必要证据包，不默认每次注入全文
- [ ] 结构建议：基于文种模板和当前草稿生成章节建议
- [ ] 改写润色：沿用内联 AI，但结果进入 diff / 预览确认
- [ ] 法规引用建议：匹配本地法规条款，用户确认后插入标准引用
- [ ] 一致性检查：标注矛盾、重复、缺依据段落，不自动改写

#### AI 记忆与会话

- [ ] 场景会话：`scene + note_path / __global__` 唯一定位，可清空
- [ ] `user_profile` 仅保存用户确认的规则、引用偏好、模型偏好，不做不可见画像
- [ ] `knowledge_deposits` 作为待整理 AI 收件箱，默认不参与知识检索
- [ ] 设置页新增“AI 记忆与规则”入口，可逐条禁用或删除

### 数据库与迁移

- [ ] 新增 `semantic_anchors` + `vec_anchors`
- [ ] 新增 `regulation_index` + `vec_regulations`
- [ ] 新增 `genre_templates`
- [ ] 新增 `sessions` + `session_messages`
- [ ] 新增 `knowledge_deposits`
- [ ] 新增 `user_profile`
- [ ] 新增 `ai_traces`
- [ ] 扩展 `files.genre`、`files.content_hash`、`chunks.embedding_model`
- [ ] 所有 schema 变更提供增量 migration 和对应 down 脚本

### 评测与质量

- [ ] 扩展 `docs/eval/semantic-search.md`：增加中文法规 / 公文 fixture
- [ ] Retrieval eval：Recall@5 不低于现有 fixture 基线，新增 MRR@10
- [ ] Grounding eval：知识查阅回答引用准确率达标，缺证据问题能拒答或标注不确定
- [ ] Tool safety eval：模型提出写入 `.md` 时必须进入确认流程
- [ ] Prompt injection 回归集：笔记 / 网页中的恶意指令不能提升工具权限
- [ ] Trace 默认不记录完整笔记正文；调试采样必须有开关和清理入口

### 非目标

- 不开放第三方插件、Skills、MCP server 运行时
- 不引入外部向量数据库
- 不做长期后台自治代理
- 不默认联网研究
- 不做用户不可见、不可删除的隐式画像
- 不让 AI 静默修改 `.md`

### 验收标准

- [ ] 知识查阅可以基于证据包回答，并展示可回查引用卡
- [ ] 文稿创作可以基于当前草稿和证据包给出结构 / 改写 / 法规引用建议
- [ ] AI 标签、模板、规则、正文写入均有用户确认
- [ ] 删除 SQLite 后，派生 AI 索引可从 `.md` 重建
- [ ] `npm run lint` / `typecheck` / `test` 通过
- [ ] `cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` 通过

---

## v1.0.0 — 完整发布

**目标**: API 稳定，性能达标，文档完备，准备长期支持。

### 核心功能

- [ ] **国际化**：至少支持中文（简体/繁体）、英文、日文
- [ ] 性能优化：10000+ 笔记目录下启动时间 < 3 秒
- [ ] 辅助功能：完整的键盘导航、屏幕阅读器支持、**高对比度主题**（N 体系的合规变体，非第三套皮肤）
- [x] **文档标题与章节**：`noteTitle`、tab 同步、章节折叠、阅读时长 — 见 [design-system](docs/design-system.md)
- [ ] **体验收尾**（按需）：标签栏自动隐藏 — Zen 已交付（`Ctrl+.`）
- [ ] 自动更新：应用内更新检测和增量更新
- [ ] 完整的单元测试和端到端测试覆盖
- [ ] API 文档（Rust 侧 IPC 接口和前端组件文档）
- [ ] 官方中文文档站

### 验收标准

- [ ] 10000 篇笔记目录冷启动 < 3 秒
- [ ] 核心功能测试覆盖率 > 80%
- [ ] WCAG 2.1 AA 级无障碍合规
- [ ] 在 Windows、macOS、Linux 三个平台均通过完整测试

## 贡献

查看 [CONTRIBUTING.md](./CONTRIBUTING.md) 了解如何参与开发。

**文档入口**：[docs/README.md](docs/README.md)（全库索引与维护约定）

| 阶段             | Epic / 审计                                                                                        |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| v0.1.0（已发布） | [v0.1.0-epic](docs/v0.1.0-epic.md)、[v0.1.0-completion-prs](docs/v0.1.0-completion-prs.md)（冻结） |
| v0.1.1（已发布） | [v0.1.1-epic](docs/v0.1.1-epic.md)                                                                 |
| 界面             | [design-system](docs/design-system.md)                                                             |
| 架构             | [ARCHITECTURE](ARCHITECTURE.md)                                                                    |
| v0.5.0 AI 建设   | [AI 体系设计](docs/superpowers/specs/2026-05-27-ai-system-design.md)                               |
