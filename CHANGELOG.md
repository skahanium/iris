# Changelog

本项目的所有显著变更将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

---

## [1.1.0] — Current

当前基线版本。代码实际建设已远超原 v0.x 规划，合并历史版本后统一切换至 v1.1.0。以下按功能域组织，非严格时间线排序。

### 编辑器

- TipTap (Prosemirror) WYSIWYG Markdown 编辑器，核心 GFM 语法支持
- 多标签页编辑器，暗色/亮色主题切换
- 文档标题独立字段（`DocumentTitleField`），与 frontmatter 双向同步
- 章节折叠（H1–H3）、悬浮大纲（`Ctrl+Shift+O`）
- Zen 模式（`Ctrl+.`）、画布缩放 75%–150%
- 图片和链接扩展
- Callout 块引用：`> [!type] Title` 语法，完整序列化往返
- 编辑器查找替换：`Ctrl+F` 查找，`Ctrl+H` 替换，高亮匹配项
- 代码块语法高亮、表格、任务列表

### AI 系统

**内联 AI 与命令**

- 内联 AI：选中文本 → 改写 / 扩写 / 翻译 / 简化，接受 / 重试 / 回退
- `/` 命令菜单（Lucide 图标 + 纸墨样式），结果流式写入 `ai-stream` 节点
- 右键 AI 上下文菜单（`IrisContextMenu`），选区 AI 操作以右键为主

**统一助手面板**（`UnifiedAssistantPanel`）

- 场景自动路由：research、writing、organize、citation、chapter/document、rules、chat
- `AiComposer` 多行输入，`@` 悬浮补全（文件夹/文档），`ContextScopeChips`
- 证据包引用卡（`ContextPacketCard`）、引用抽屉（`ContextPacketDrawer`）
- 工具调用气泡（`ToolCallBubble`）+ 确认弹窗（`ToolConfirmDialog`）
- 规则确认弹窗（`RuleConfirmDialog`）、执行计划预览（`ExecutionPlanPreview`）
- AI 消息选区、会话历史下拉（`SessionHistoryDropdown`）
- Skills 面板、Token 用量条（`TokenUsageBar`）
- Harness 活动条（`HarnessActivityStrip`）、上下文状态栏（`ContextStatusBar`）
- 研究结果消息（`ResearchResultMessage`）+ 专注视图（`ResearchFocusView`）
- 文档检查产物（`DocumentCheckArtifacts`）、补丁预览（`PatchPreview`）
- 引用检查视图（`CitationCheckView`）、证据链视图（`EvidenceChainView`）
- AI 规则面板（`AiRulesPanel`）、身份设置（`AssistantIdentitySection`）

**AI Runtime（Rust）**

- `ai_runtime` 模块：`scene_router`、`context_planner`、`retrieval_broker`、`packet_builder`、`model_gateway`、`tool_executor`、`guardrails`、`trace`
- `harness` 编排调度 + `harness_support`
- `execution_plan` 执行计划、`evidence_mixer` 证据融合
- `session` 会话管理（`sessions` + `session_messages` 表）
- `skills` Skills 系统（全局/Vault，支持 URL / Git / 本地安装）
- `prompt_profile` 提示词配置
- 混合检索（`retrieval_broker`）：FTS + sqlite-vec + 显式链接/标签 四路融合
- `model_registry` 模型能力注册、`model_catalog`（DeepSeek V4 等）
- `packet_cache` 证据包 LRU 缓存

**写作/研究/章节工作流**

- `writing_workflow`：结构建议、改写润色、法规引用（证据包驱动）、一致性检查
- `research_workflow`：有限 agentic loop，子任务分解与结果整合
- `chapter_workflow`：章节/文档级检查与 `PatchProposal` 确认
- `document_workflow`：全文级操作
- `citation_workflow`：引用验证
- `organize_workflow`：AI 整理

**LLM 集成**

- 多提供商：OpenAI 兼容、Anthropic Messages API、Ollama、自定义
- 四场景路由（`llm_routing`）：各场景独立 `providerId` / `model` / `contextStrategy`
- DeepSeek 前缀缓存纪律（分层 messages、同会话同参数）
- 动态 token 预算、`long_context` 笔记全文注入
- 流式渲染 + `llm_abort` 取消
- 四提供商共享同一 provider 选择（内联 AI、`/` 命令、助理面板）

**联网搜索**

- MiniMax Token Plan `coding_plan/search`（主通道）
- DuckDuckGo HTML 抓取降级
- AI 输入关联网开关，发送前自动注入网页摘要
- 搜索结果 30 分钟 LRU 缓存

**知识索引**

- 语义锚点（`semantic_anchors` + `vec_anchors`）
- 法规条款索引（`regulation_index` + `vec_regulations`）
- 文种模板提取（`genre_templates`）
- 语料库（`.iris/corpora.toml`）+ `retrieval_scope` 路径过滤
- `VaultNavigator` 树形浮层（由 FileSheet 演进）

**AI 记忆与个性化**

- 场景会话：`scene + note_path / __global__` 唯一定位
- `user_profile` 用户确认的规则/偏好，可逐条禁用/删除
- `knowledge_deposits` AI 收件箱
- `AiRulesPanel` + `AssistantIdentitySection` 设置入口

### 知识网络

- `[[wiki-link]]` 双向链接：编辑器语法、自动补全、click 导航、links 表索引
- 反向链接面板（`Ctrl+Shift+B`）
- 正文 `#tag` 解析（与 YAML frontmatter tags 合并）
- 标签聚合视图（`Ctrl+Shift+T`）：标签云 + 统计面板
- 知识图谱可视化（`Ctrl+Shift+G`）：Canvas 力导向图，零外部依赖

### 搜索

- FTS5 全文关键词搜索
- sqlite-vec 向量语义搜索（`vec0` 虚拟表 + cosine fallback）
- Hybrid retrieval：FTS + vec + wiki-link + exact 多路融合
- fastembed (AllMiniLML6V2, 384-dim) 嵌入生成
- 语义 Recall@5 ≥ 0.6（fixture 评测）

### 版本系统

- 双层保存：防抖写 `.md`（默认 1.2s）+ 稀疏快照（`.iris/versions/`）
- `Ctrl+S` 层 1 保存当前 `.md`；`Ctrl+Shift+S` 手动版本快照（`kind=manual`）
- 空闲 10 分钟自动备份（`kind=auto_idle`，每篇上限 30 条）
- 定稿当前正文（`kind=finalize`，永久保留）
- 恢复前强制 `pre_restore` 快照
- 双栏对比版本时间线（`Ctrl+Shift+V`）
- 启动时自动清理 7 天前 `auto_idle`

### 文件管理

- Vault 管理：用户指定笔记目录，递归 `.md` 扫描
- 文件 CRUD（创建、重命名、删除）
- Quick Open（`Ctrl+P`）：模糊搜索文件切换
- 命令面板（`Ctrl+Shift+P`）：底栏常驻入口
- 回收站（`RecycleBinSheet`）
- 文件导出：Markdown、自包含 HTML（纸墨 CSS 内嵌）
- 笔记模板：4 个内置 + 自定义 `.iris/templates/*.md`
- 新建文档命名：`新建文档`、`新建文档（1）`…
- 外部文件修改检测 + 冲突解决（L1 静默 / L2 合并 / L3 抉择弹窗）

### 界面系统

- **设计方向**：Notion 式扁平编辑（N），命令优先辅助（C）
  - 编辑区：`.iris-editor-canvas`、`max-width: 45rem`、`Inter` 字体、蓝灰 accent
  - 无行线、无纸页卡片、无紫色渐变
  - 暗色主题 flat gray，亮色主题纯白附近
- 居中命令浮层（`IrisOverlay`）：`compact` / `command` / `wide` / `graph` 四档
- Chrome 现代化（`--surface-*`、`--command-highlight-*`、`--ai-*` CSS token）
- 共享原语：`OverlayChrome`、`CommandListOption`、`Kbd`、`AiComposer`、`AiMessage`、`SurfaceCard`
- `IrisSurfaceMenu`（`/` 命令菜单 + 右键菜单）、`IrisContextMenu`
- 无边框窗口：macOS `transparent: true` + 前端裁切；Windows 11 DWM 圆角
- 桌面圆角 `--window-radius`（12px）、浮层阴影 `--shadow-overlay`
- 动效 150–200ms，`prefers-reduced-motion` 降级
- AI 侧栏默认 360px，左缘拖拽调整（280px–560px），`Ctrl+Shift+A` 收起

### 安全

- API Key 仅存储于操作系统凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service）
- 所有 LLM API 请求走 HTTPS
- 路径穿越防护（`canonicalize()` + 前缀比对）
- DOMPurify 清洗 Markdown 渲染 HTML；CSP 禁止内联脚本
- 临时文件安全删除（`secure_delete`）
- HTTPS 证书固定（certificate pinning）
- SQL 参数化查询，禁止字符串拼接

### 存储与数据库

- SQLite（WAL 模式）+ `.md` 权威数据源
- 17 个增量 migration（含 up/down 脚本）：
  - `001_core`：files、tags、file_tags、links、chunks、chunk_embeddings、settings、FTS5
  - `002_vec`：sqlite-vec `vec0` 虚拟表
  - `003_versions` / `006_versions_kind`：版本系统
  - `004_files_dedupe` / `005_drop_iris_metadata_files`：去重清理
  - `007_recycle_bin` / `008_chunks_char_count`：回收站 + 字符计数
  - `009_ai_runtime`：sessions、session_messages、knowledge_deposits、user_profile、ai_traces
  - `010_knowledge_index`：semantic_anchors、regulation_index、genre_templates、block_links
  - `011_eval_results` / `012_session_title` / `013_ai_trace_checkpoint`
  - `014_web_page_cache`：联网搜索网页缓存
  - `015_search_cache`：搜索结果缓存
  - `016_cas_refs`：CAS 对象引用计数
  - `017_rename_cascade`：级联重命名支持

### 工程质量

- 前端测试 126 文件（Vitest），覆盖 AI 管线、编辑器、命令面板、UI 组件、版本系统等
- E2E 测试 5 文件（Vitest）：验收、AI 工作流、编辑器上下文操作、统一助手契约
- Rust 集成测试 4 文件：文件操作、frontmatter、语义召回、vault 监听
- `cargo fmt` / `cargo clippy -D warnings` / `cargo test` Rust CI
- `npm lint` / `npm format:check` / `npm typecheck` / `npm test` 前端 CI
- 品牌图标系统（`IrisMark`、`iris-mark-paths`、桌面图标全套）

### Changed

- v0.4.0-ui 起：废弃纸墨（B）/ 信纸视觉，切换为 Notion N 扁平灰阶
- 旧 `AiPanel` 多入口合并为 `UnifiedAssistantPanel` 单入口自动路由
- `file_write` 仅持久化 `.md`，不再自动创建版本快照

### Known limitations (v1.1.0)

- 无 Playwright 全链路 E2E（Vitest 场景测试已有；无限期延后目标 Playwright + 覆盖率 > 80%）
- 无国际化（仅中文界面；无限期延后目标简中 + 英文）
- 性能未在 10000+ 笔记规模下基准测试
- 无 WCAG 无障碍认证（无限期延后目标全应用 WCAG 2.1 AA）
- 图片：ImageExtension 节点已有；拖拽/粘贴与 Vault 内资产管理无限期延后
- 无自动更新
- Notion 官方文档站无限期延后

---

## 变更分类

- **Added** — 新增功能
- **Changed** — 现有功能的变更
- **Deprecated** — 即将移除的功能
- **Removed** — 已移除的功能
- **Fixed** — 漏洞修复
- **Security** — 安全相关修复

## 历史版本

v0.1.0–v0.5.2 的功能记录已合并入上方的 v1.1.0。各版本 Epic 与施工计划见 [docs/history/](docs/history/)。
