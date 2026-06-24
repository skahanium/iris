# 路线图

Iris 采用里程碑式版本规划。当前基线为 **v1.2.0**。

版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## 产品原则与非目标

Iris 是**桌面端、单用户、本地优先**的 Markdown 笔记应用。下列能力为**永久非目标**：

| 类别     | 永久不做                                                 |
| -------- | -------------------------------------------------------- |
| 扩展生态 | 第三方通用插件 API / 插件市场 / 应用内加载任意社区扩展包 |
| 数据安全 | Vault 目录级 AES 加密（与 `.md` 明文主权冲突，不做）     |
| 平台     | 移动端（iOS / Android）、Tauri 移动靶                    |
| 协作     | 实时多人协作、CRDT 同步（如 Yjs）                        |
| 输入形态 | 语音转文字笔记、手写笔迹                                 |
| 外围产品 | Web Clipper 浏览器扩展                                   |

**Skills 与插件的边界**：Iris 支持用户**显式安装** Claude 兼容 `SKILL.md` 提示词包（URL / Git / 本地 / 拖拽，全局或 Vault 级），用于注入 AI 行为。这不等于 Obsidian 式通用插件运行时或插件市场——Skills 不能扩展 UI、不能注册新节点类型、不能执行任意代码。

**扩展方式**（仅此几种）：

- **主线功能**：新能力通过版本里程碑交付
- **Skills 提示词包**：用户自备来源安装 `SKILL.md`，见管理中心 `AI` 分区与 `skills_*` IPC
- **AGPL 源码**：深度定制请 fork 或向上游提 PR
- **声明式配置**：内置 `/` 命令模板、主题 CSS 变量、快捷键等
- **Vault 外工具链**：笔记为 `.md` 纯文本，可用任意编辑器、脚本、Git 处理

## 体验方向

产品与界面的长期取向：**主攻 [Notion 编辑（N）](docs/design-system.md#n--notion-编辑主方向)**；**备选 [命令优先（C）](docs/design-system.md#c--命令优先备选原则)**（键盘导航、可收起 AI 侧栏）。

实现细则、token 与组件规则见 **[docs/design-system.md](docs/design-system.md)**；交互线框见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

当前视觉系统刷新目标为 **Iris Rail 完整界面系统**：常驻品牌轨道、Rail Segments Tab、沉静编辑画布、边缘目录轨、AI 协作侧车、任务舱 Overlay 与管理中心；当前验收规则直接维护在 [docs/design-system.md](docs/design-system.md)。

**体验非目标**（与产品非目标并列）：第三方主题/插件换肤、紫色渐变 AI 套路、聊天主屏化。

---

## v1.2.0（当前基线）— 已实现

以下按功能域组织，概述已落地的完整能力。详细的变更记录见 [CHANGELOG.md](./CHANGELOG.md)。

### 编辑器

- TipTap (Prosemirror) WYSIWYG Markdown 编辑器，核心 GFM 支持
- 多标签页编辑、暗色/亮色主题切换
- 文档标题字段（`DocumentTitleField`）、章节折叠、Ghost Spine 悬浮大纲（边缘把手）
- Zen 模式（`Ctrl+.`）、画布缩放 75%–150%
- 图片与链接：`ImageExtension`、PM 热路径 `![alt](src)` 往返；`EditorImageDropExtension` 拖放/粘贴写入 `assets/`（`vault_asset_write`）；`LinkExtension` 支持 `Ctrl+K` / `Cmd+K`
- Callout 块引用：`> [!type] Title` 语法，完整序列化往返（`CalloutBlockquoteExtension`）
- 编辑器查找替换：`Ctrl+F` 查找，`Ctrl+H` 替换，高亮匹配项（`FindHighlightExtension`）
- Markdown 块间距：`contract` `space` → `IrisParagraphExtension`（`data-iris-spacer`）；编辑区与 AI 共用 `.iris-markdown-content`（`markdown-prose.css`）

### AI 系统

**内联与命令**

- 内联 AI：选中文本 → 改写 / 扩写 / 翻译 / 简化，接受 / 重试 / 回退
- `/` 命令菜单：总结、大纲、头脑风暴、修复语法等，结果流式写入
- 右键 AI 菜单（`IrisContextMenu`），无自动浮动工具条

**统一助手面板**（`UnifiedAssistantPanel`，`Ctrl+Shift+A`）

- TaskPlan 自动路由（chat、ask_notes、creative_write、rewrite_selection、citation_check、research、organize、document_check、chapter、vision_chat、skill_management），同一会话每轮可重新判定任务类型
- `AiComposer` 多行输入，`@` 语料库/文档范围检索（`ContextScopeChips`）
- 证据包引用卡（`ContextPacketCard`）、工具确认弹窗（`ToolConfirmDialog`）、规则确认（`RuleConfirmDialog`）
- AI 流式渲染、Markdown-first 消息气泡（`AiMessageBubble`）、会话历史（`SessionHistoryDropdown`）
- Skills 管理入口位于管理中心 `AI` 分区；Token 累计与工具状态在底栏（`StatusBarTokenUsage`）；证据统计在 `ContextPacketDrawer` 标题行

**Agent Task Runtime（Rust 后端）**

- 完整管线：`AgentTaskPolicy` → `context_planner` → `retrieval_broker` → `packet_builder` → capability `model_gateway` → `tool_executor`
- `guardrails`（prompt injection 基础防护）、`trace`（可观测性）、`harness`（编排调度）
- 旧 scene-shaped 会话与配置只作为迁移输入读取，执行策略以 task intent、scope、capability slot 为准
- 混合检索：FTS + 向量（默认 Rust cosine fallback；sqlite-vec vec0 为 optional/experimental，当前 Windows 构建有阻塞）+ 显式链接/标签融合
- `ContextPacket` 证据包：来源路径、span、hash、score、trust_level、citation_label
- 引用验证（Citation verifier）、证据链可视化（`EvidenceChainView`）
- 文稿创作工作流：结构建议、改写润色、法规引用、一致性检查
- 研究工作流：TaskPlan 调度 + Web Evidence Broker；对话区保留普通 Markdown 摘要，来源详情只在有证据时进入只读临时 tab
- 章节/文档工作流：`PatchProposal` 确认体系

**LLM 路由与连通性**

- 模型与供应商设置采用供应商连接、模型目录、能力路由三层；供应商级连通性与模型级文本 / 视觉验证分离，未知模型默认只进入文本候选
- 长上下文策略（`long_context`）、动态 token 预算
- DeepSeek 前缀缓存纪律（分层 messages、同会话同参数）
- 底栏 LLM/联网连通性指示器（`ConnectivityIndicators`）
- MiniMax Token Plan 联网检索（WebSearch Broker）+ DuckDuckGo 降级；助手工具 `fetch_web_page` 受控抓取 HTTPS 正文（用户确认）

**知识索引**

- 语义锚点（`semantic_anchors` + `vec_anchors`）
- 法规条款索引（`regulation_index` + `vec_regulations`）
- 文种模板提取（`genre_templates`）
- 语料库（`.iris/corpora.toml`）+ `RetrievalScope` 路径过滤 + `VaultNavigator`

**AI 记忆与个性化**

- 场景会话（`scene + note_path / __global__`）
- `user_profile` 规则、偏好存储，可逐条禁用/删除
- `knowledge_deposits` AI 收件箱
- `prompt_profile` 设置

### 知识网络

- `[[双向链接]]`：自动补全、click 导航、links 表索引
- 知识关联任务舱：反向链接与标签页签合并，通过管理中心 `知识库` 分区或上下文入口打开
- 正文 `#tag` 解析 + 标签聚合页签
- 知识图谱可视化：Canvas 力导向图，通过底栏图谱按钮直达

### 搜索

- FTS5 全文关键词搜索
- 向量语义搜索：默认 Rust cosine fallback 可用；sqlite-vec vec0 为 optional/experimental，不作为默认质量门禁
- Hybrid retrieval：FTS + vec + link + exact 多路融合
- 语义 Recall@5 评测集（[docs/eval/semantic-search.md](docs/eval/semantic-search.md)）

### 版本系统

- 双层保存：防抖写 `.md`（层 1）+ 稀疏快照（层 2）
- `Ctrl+S` 层 1 保存；空闲自动 `auto_idle` 默认为 10 分钟，可在管理中心 `笔记` 分区调整开关和间隔；手动检查点与定稿在版本面板内操作
- 恢复前强制 `pre_restore` 保护
- 启动时 7 天 `auto_idle` 清理；每篇上限 30 条
- 双栏对比版本时间线（`Ctrl+Shift+V`）

### 文件管理

- Vault 管理、文件 CRUD、外部修改检测与冲突解决（L1/L2/L3）
- Quick Open（`Ctrl+P`）、全库搜索（`Ctrl+Shift+F`）、管理中心（`Ctrl+,` / 底栏齿轮）
- 文件树直达（`Ctrl+Shift+E` / `Cmd+Shift+E`）进入管理中心 `笔记 → 浏览笔记库`
- 回收站位于管理中心 `笔记` 分区；文件导出（Markdown / HTML）
- 笔记模板系统（内置 4 个 + 自定义 `.iris/templates/*.md`）
- 新建文档自动命名：`新建文档`、`新建文档（1）`…

### 界面系统

- **设计方向**：Notion 式扁平编辑（Inter、蓝灰 accent、无行线/纸页）
- 居中任务浮层系统（`IrisOverlay`）：compact / command / management / wide / graph；管理型设置、文件树与回收站进入顶部标签式 `ManagementCenterPanel`
- `IrisSurfaceMenu`、`IrisContextMenu` 右键菜单
- Chrome 现代化（`--surface-*`、`--command-highlight-*`、`--ai-*` token）
- 启动首帧稳定：隐藏窗口直到 chrome/theme 就绪，并以 Knowledge Orbit 启动层过渡到客户端界面
- 桌面窗口 chrome：macOS 使用原生 decorated overlay 与左侧系统红黄绿，并通过配置期 traffic-light inset 与 Iris 顶栏中线对齐；Windows/Linux 保持无边框自绘控件（Windows 11 DWM 圆角）

### 安全与存储

- API Key 存储于 OS 凭据管理器，禁止落盘
- SQLite 本地索引 + `.md` 权威数据源
- HTTPS-only rustls 客户端、路径穿越防护、DOMPurify XSS 清洗；证书固定仅保留显式 pin 配置的 opt-in API
- 临时文件安全删除（`secure_delete`）
- CSP 严格策略
- 敏感数据保护：API Key 仅存 OS 凭据管理器；笔记为明文 `.md`，建议 OS 级全盘加密（BitLocker / FileVault / LUKS）

### CAS 内容寻址存储

- 完整 CAS 系统：哈希、存储、加密、补丁、引用计数、垃圾回收、写保护
- 版本快照使用 CAS 存储（`cas:` 前缀）
- 每日凌晨 3:00 自动 GC（`scheduler.rs`）
- 级联重命名：重命名文件时自动更新所有 wikilink 引用和会话路径

### 工程质量

- 前端 126 测试文件（Vitest）、Rust 集成测试 4 文件
- E2E：Vitest 场景测试（v1.2.0 目标：Playwright 全链路 + 覆盖率 > 80%）
- 25 个数据库 migration（001_core 到 025_knowledge_scalar_backfill，含 up/down；sqlite-vec 相关迁移为 best-effort/optional）
- CI：`cargo fmt/clippy/test` + `npm lint/typecheck/test`（RustSec 使用 `npm run audit:rust`，例外清单集中在 `.cargo/audit.toml`；`npm audit` 为本地推荐，尚未接入 CI）

---

## v1.2.0 — 稳定发布（无限期延后）

**目标**：API 稳定，性能达标，文档完备，无障碍合规，准备长期支持。

> **注意**：以下功能已无限期延后，不纳入当前版本规划。

### 待交付（无限期延后）

- [ ] **国际化**：简体中文 + 英文界面与文案
- [ ] **性能达标**：10000+ 笔记目录冷启动 < 3 秒
- [ ] **WCAG 2.1 AA**：全应用无障碍合规（含 AI 面板、知识图谱等）
- [ ] **图片完整工作流**：拖拽/粘贴插入、Vault 内本地图片复制与引用解析（alpha 已有 ImageExtension 节点）
- [ ] **自动化测试**：Playwright 全链路 E2E + 核心功能测试覆盖率 > 80%
- [ ] **自动更新**：应用内更新检测和增量更新（Tauri updater）
- [ ] **Notion 官方文档站**：用户向（快速开始、AI 配置、快捷键、FAQ；简中为主，链出英文页）

### 验收标准（无限期延后）

- [ ] 10000 篇笔记目录冷启动 < 3 秒
- [ ] 核心功能测试覆盖率 > 80%
- [ ] WCAG 2.1 AA 级无障碍合规
- [ ] 在 Windows、macOS、Linux 三个平台均通过完整测试
- [ ] `npm run lint` / `typecheck` / `test` 通过
- [ ] `cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` 通过

---

## 历史版本

下列版本号已归档，不再独立维护。对应功能已并入 v1.2.0。

| 版本          | 重心                  | 状态      |
| ------------- | --------------------- | --------- |
| v0.1.0        | AI 原生 MVP           | 已发布    |
| v0.1.1        | 体验定稿与质量补齐    | 已发布    |
| v0.2.0        | 知识网络 + sqlite-vec | 已发布    |
| v0.3.0        | 安全与版本            | 已发布    |
| v0.3.1-ui     | 命令浮层基础设施      | 并入 v0.4 |
| v0.4.0-ui     | Notion 扁平编辑       | 已发布    |
| v0.4.1-ui     | Chrome 现代化         | 已发布    |
| v0.5.0–v0.5.2 | AI 建设 MVP 全线      | 已发布    |

旧 Epic、施工计划与审计记录已从工作树移除；必要时通过 git 历史追溯。

---

## 贡献

查看 [CONTRIBUTING.md](./CONTRIBUTING.md) 了解如何参与开发。

**文档入口**：

| 文档                                           | 用途                 |
| ---------------------------------------------- | -------------------- |
| [docs/design-system.md](docs/design-system.md) | 界面 token、组件规则 |
| [ARCHITECTURE.md](./ARCHITECTURE.md)           | 分层、IPC、数据流    |
| [CHANGELOG.md](./CHANGELOG.md)                 | 版本变更记录         |
| [AGENTS.md](./AGENTS.md)                       | AI/人协作开发规范    |
