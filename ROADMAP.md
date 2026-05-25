# 路线图

Iris 采用里程碑式版本规划。每个版本对应一个完整的功能集，不绑定具体日期。

版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。v0.x 阶段 API 可能变动。

### 产品原则与非目标

Iris 是**桌面端、单用户、本地优先**的 Markdown 笔记应用。下列能力为**永久非目标**（不排期、不在「未来探索」中预留评估）：

| 类别 | 永久不做 |
|------|----------|
| 扩展生态 | 第三方插件 / 插件市场 / 应用内加载社区扩展包 |
| 平台 | 移动端（iOS / Android）、Tauri 移动靶 |
| 协作 | 实时多人协作、CRDT 同步（如 Yjs） |
| 输入形态 | 语音转文字笔记、手写笔迹 |
| 外围产品 | Web Clipper 浏览器扩展 |

**扩展方式**（仅此几种）：

- **主线功能**：新能力通过版本里程碑交付。
- **AGPL 源码**：深度定制请 fork 或向上游提 PR。
- **声明式配置（规划中）**：内置 `/` 命令模板、主题 CSS 变量、快捷键等。
- **Vault 外工具链**：笔记为 `.md` 纯文本，可用任意编辑器、脚本、Git 处理；不在 Iris 进程内加载第三方代码。

原 **v0.4.0「插件系统」** 里程碑已删除。

### 体验方向（与路线图绑定）

产品与界面的长期取向：**主攻 [纸墨编辑（B）](docs/design-system.md#b--纸墨编辑主方向)**；**备选 [命令优先（C）](docs/design-system.md#c--命令优先备选原则)**（键盘导航、可收起 AI 侧栏，不常驻占宽）。实现细则、token 与组件规则见 **[docs/design-system.md](docs/design-system.md)**；交互线框见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

各版本**功能里程碑**与**界面交付**一并规划（避免「功能路线图」与「设计稿」两套叙事）：

| 版本 | 功能重心 | 体验 / 界面交付 |
|------|----------|-----------------|
| **v0.1.0** | AI 原生 MVP（已发布） | 可用型 UI（通用深灰 + 紫色 accent），未按纸墨定稿 |
| **v0.1.1** | 设置与质量补齐 | **纸墨阶段 0**：chrome/纸面 token、衬线编辑区、`Ctrl+Shift+A` 收起 AI；见下方 checklist |
| **v0.2.0** | 知识网络 + sqlite-vec | **纸墨阶段 1**：引用卡 / 关联笔记芯片 / `/` 菜单；图谱与标签视图符合纸墨层级 |
| **v0.3.0** | 安全与版本 | 版本时间线、diff、模板库等**低干扰**纸面组件 |
| **v1.0.0** | 稳定与合规 | **纸墨阶段 2～3**（可选）：Zen、标签栏隐藏、克制动效；WCAG 与高对比主题 |

**体验非目标**（与产品非目标并列）：第三方主题/插件换肤、紫色渐变 AI 套路、聊天主屏化。详见 design-system「非目标（视觉）」。

### 待定特色方向（未排期）

下列为 Iris 的**差异化方向**，值得持续讨论与原型验证，**不绑定具体版本号**，实现时机视 v0.2 标签体系与索引能力而定：

- **AI 自动标签与分类**：基于笔记全文与 vault 上下文，由 LLM 建议或写入 frontmatter / `#tag`（须用户确认或可调策略）；与语义检索、关联笔记形成闭环。要求内置实现、可关闭、不依赖第三方插件。

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
- [x] **版本记录系统**：
  - 自动快照：Ctrl+S 保存时 + 定时扫描（内容有变更时生成）
  - 定稿（Finalize）：一键生成正式快照，标记为永久保留，可自定义版本名
  - 自动清理：非定稿快照创建 7 天后自动删除
  - 存储：元数据在 SQLite，内容全文在 `.iris/versions/` 隐藏目录中
  - 版本恢复：预览后恢复到当前编辑器，恢复前自动保存当前状态
- [x] 笔记模板系统：会议纪要、读书笔记、项目复盘等模板库
- [x] 文件导出：Markdown、HTML 格式导出（PDF 推迟，HTML 可浏览器打印为 PDF）
- [ ] 图片拖拽插入/粘贴，本地图片管理 → **推迟至 v1.0**

#### 体验与界面

- [x] 版本时间线、diff、模板选择器沿用纸墨 token（编辑区不被「工具 UI」抢戏）

### 验收标准
- [ ] 加密目录在未解锁时外部编辑器打开为乱码
- [ ] 冲突解决的 L1（静默同步）/ L2（提示合并）/ L3（用户抉择）三层策略生效
- [ ] 版本历史可以精确回退到任意时间点

---

## v1.0.0 — 完整发布

**目标**: API 稳定，性能达标，文档完备，准备长期支持。

### 核心功能

- [ ] **国际化**：至少支持中文（简体/繁体）、英文、日文
- [ ] 性能优化：10000+ 笔记目录下启动时间 < 3 秒
- [ ] 辅助功能：完整的键盘导航、屏幕阅读器支持、**高对比度主题**（纸墨体系的合规变体，非第三套皮肤）
- [ ] **体验收尾**（纸墨阶段 2～3，按需）：Zen 模式、标签栏自动隐藏、克制流式动效 — 见 [design-system](docs/design-system.md)
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

| 阶段 | Epic / 审计 |
|------|-------------|
| v0.1.0（已发布） | [v0.1.0-epic](docs/v0.1.0-epic.md)、[v0.1.0-completion-prs](docs/v0.1.0-completion-prs.md)（冻结） |
| v0.1.1（已发布） | [v0.1.1-epic](docs/v0.1.1-epic.md) |
| 界面 | [design-system](docs/design-system.md) |
| 架构 | [ARCHITECTURE](ARCHITECTURE.md) |
