# 技术架构

> 本文档描述 Iris 的系统架构、数据流、安全模型和关键设计决策。

**文档分工**（勿在本文件重复维护版本排期）：

| 文档                                             | 内容                         |
| ------------------------------------------------ | ---------------------------- |
| [ROADMAP.md](./ROADMAP.md)                       | 版本里程碑                   |
| [docs/design-system.md](./docs/design-system.md) | Notion N token、组件、C 原则 |
| [docs/README.md](./docs/README.md)               | 全库文档索引                 |
| 本文档                                           | 分层、IPC、数据流、安全      |

---

## 概览

```
┌─────────────────────────────────────────────────────────┐
│                     WebView (React)                      │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐ │
│  │ TipTap   │ │ AI Panel │ │ File     │ │ Search     │ │
│  │ Editor   │ │ (助手面板) │ │ Explorer │ │ Panel      │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └─────┬──────┘ │
│       │             │            │              │        │
│       └─────────────┴────────────┴──────────────┘        │
│                         │ Tauri IPC (invoke)               │
└─────────────────────────┼────────────────────────────────┘
                          │
┌─────────────────────────┼────────────────────────────────┐
│                    Rust Backend (Tauri 2.x)               │
│                         │                                  │
│  ┌──────────────────────┼──────────────────────┐          │
│  │              Command Router                   │          │
│  │  ┌────────┐ ┌───────┐ ┌───────┐ ┌─────────┐ │          │
│  │  │ File   │ │ LLM   │ │Search │ │ AI     │ │          │
│  │  │ System │ │ Engine│ │Engine │ │Runtime │ │          │
│  │  └───┬────┘ └───┬───┘ └───┬───┘ └────┬────┘ │          │
│  └──────┼──────────┼─────────┼─────────┼──────┘          │
│         │           │          │         │                  │
│  ┌──────┼───────────┼──────────┼─────────┼──────┐          │
│  │              Data Layer                        │          │
│  │  ┌─────┐  ┌──────────┐  ┌──────────────────┐ │          │
│  │  │ .md │  │  SQLite  │  │ OS Credential    │ │          │
│  │  │Files│  │  + vec   │  │ Manager          │ │          │
│  │  └─────┘  └──────────┘  └──────────────────┘ │          │
│  └──────────────────────────────────────────────┘          │
└──────────────────────────────────────────────────────────┘
```

## 层级职责

### 渲染层 — React + TipTap

- **职责**: UI 渲染、用户交互、编辑器状态管理
- **不负责**: 文件操作、网络请求、数据持久化
- **关键库**:
  - `@tiptap/react` — 编辑器核心
  - `prosemirror-*` — 文档模型、变换、状态管理
  - `tailwindcss` + `shadcn/ui` — 样式系统
  - `@tauri-apps/api` — IPC 调用桥接

### 逻辑层 — Rust (Tauri 2.x)

- **职责**: 所有需要系统权限或原生性能的操作
- **关键 crate**:
  - `rusqlite` — SQLite 数据库操作（默认 Rust cosine fallback；sqlite-vec vec0 为 optional/experimental）
  - `fastembed-rs` — 本地嵌入生成（AllMiniLML6V2, 384-dim）
  - `reqwest` — HTTP 客户端（LLM API 调用）
  - `tokio` — 异步运行时
  - `serde` / `serde_json` — 序列化
  - `notify` — 文件系统事件监听

### 存储层

- **`.md` 文件**: 用户笔记的权威数据源。每个文件独立存在，可用任意编辑器打开。
- **SQLite 数据库**: 双重角色——笔记索引（files/chunks/links 可由 .md 重建）和应用状态（AI 会话、网页缓存等运行时数据）。
- **OS 凭据管理器**: 仅存储 API Key，不存储用户内容。

## 数据流

### 1. 编辑器 → 文件

```
用户键盘输入 → TipTap 编辑操作
  → ProseMirror Transform (内存状态变化)
  → editorDocToMarkdown (prosemirror-markdown；callout / wiki / table 等见 docs/markdown-export.md)
  → 失败时回退 HTML Turndown
  → Tauri IPC: file_write(path, content)
  → Rust: fs::write()
  → 异步: 合并防抖后更新 SQLite 索引 (frontmatter、标签、链接)
```

### 2. AI 请求 → 编辑器

```
用户触发 AI 命令 (选中文本 + 操作类型)
  → Tauri IPC: llm_generate(params)
  → Rust LLM Engine:
      1. 收集上下文 (当前文档、关联笔记、system prompt)
      2. 构建请求 (OpenAI-compatible API format)
      3. 通过 reqwest 发送 HTTPS POST
      4. 流式读取 response body
      5. 每个 chunk 通过 Tauri Event emit 到 WebView
  → WebView: TipTap 插入 ai-stream Node, 逐 token 更新内容
  → 用户接受: file_write() + 关闭 ai-stream node
  → 用户拒绝: 移除 ai-stream node, 恢复编辑器状态
```

### 3. 语义搜索

```
用户输入自然语言查询
  → Tauri IPC: search_semantic(query)
  → Rust Search Engine:
      1. 查询文本 → fastembed → 查询向量 (384-dim)
      2. Rust cosine fallback（默认可用）
      3. sqlite-vec 虚拟表 vector Top-K（optional/experimental；当前 Windows 构建有阻塞）
      4. 返回 Top-K 结果 (文件路径 + 片段 + 相似度分数)
  → WebView: 搜索结果列表，点击跳转到对应笔记
```

### 4. Agent Task Runtime 完整管线（v1.2.1）

```
用户提问 → AiComposer
  → ai_send_message IPC
  → AgentTaskPolicy: 按 task intent / scope / attachments / privacy 生成执行策略
  → context_planner: 确定上下文策略（hybrid / long_context）与检索子查询
  → retrieval_broker: FTS + vec + link + exact 多路融合检索
  → packet_builder: 组装 ContextPacket（来源、span、hash、score、trust_level）
  → model_gateway: 按 capability slot 选择模型，构建分层 messages
  → 流式返回 token → WebView 逐字渲染
  → tool_executor: 模型提出工具调用 → ToolConfirmDialog → 用户确认 → 执行
  → trace: 记录全链路（不含笔记正文）
  → guardrails: 系统级 prompt injection 防护 + Rust schema 校验
```

旧 scene-shaped 输入仅用于迁移既有会话、连通性显示和兼容 IPC，不作为长期主架构承诺。

### 5. 文件外部修改同步

```
外部编辑器修改 .md 文件
  → notify crate 检测文件变更事件
  → Rust: 计算 SHA-256 hash
  → 策略判断:
      L1: 当前文件未打开 → 静默更新 SQLite 索引
      L2: 文件打开但未修改 → 静默更新 TipTap 文档树
      L3: 外部修改与编辑区域重叠 → ConflictDialog diff 视图
  → 用户抉择后 → file_write() 或 discard()
```

### 6. 版本系统

**双层保存**：

| 层级 | 行为                                                          |
| ---- | ------------------------------------------------------------- |
| 层 1 | 编辑防抖写 `vault/*.md`；用户感知为「自动保存」，不产生版本行 |
| 层 2 | 稀疏检查点写入 `.iris/versions/`                              |

**触发策略**：

| 触发方式                       | 行为                                                              |
| ------------------------------ | ----------------------------------------------------------------- |
| `Ctrl+S`                       | 立即 flush 层 1（仅写当前 `.md`，不创建版本行）                   |
| `Ctrl+Shift+S`                 | flush 层 1 + 后台 `version_save_manual`（`kind=manual`）          |
| 命令面板「保存笔记」           | 同 `Ctrl+S`                                                       |
| 命令面板「保存并创建版本快照」 | 同 `Ctrl+Shift+S`                                                 |
| 空闲 10 分钟                   | 打开中的文档无编辑 → 后台 `version_save_idle`（`kind=auto_idle`） |
| 定稿                           | 对当前正文新建快照，`kind=finalize`，永久保留                     |
| 恢复前                         | `version_restore` 内建 `pre_restore`                              |

**存储结构**：

```
<笔记目录>/
├── 新建文档.md
└── .iris/
    ├── config.json
    └── versions/
        └── <file_id>/
            ├── 20260525143052123.md
            └── ...
```

**清理与配额**：

- `auto_idle`：每篇最多 30 条；启动时删除 7 天前记录
- 定稿（`is_finalized=1`）：不自动删除
- 其他 kind（`manual`、`pre_restore`）：不因 7 天规则被删

**版本恢复**：

1. 预览历史快照（只读）
2. 确认恢复 → 强制创建 `pre_restore` 快照
3. 成功 → 目标快照写回 `.md` → `index_file`
4. 失败 → 当前正文不受影响

**版本清理**：应用启动时执行 `version_cleanup()`，只删 `auto_idle` + 未定稿 + 7 天前。

---

## IPC 协议

Tauri 的命令式 IPC 基于 JSON 序列化。所有 Rust 函数通过 `#[tauri::command]` 宏暴露。

### 命令分类

| 前缀           | 模块        | 示例                                                                   |
| -------------- | ----------- | ---------------------------------------------------------------------- |
| `file_*`       | 文件系统    | `file_list`, `file_read`, `file_write`                                 |
| `llm_*`        | AI 集成     | `llm_generate`, `llm_chat`, `llm_abort`                                |
| `search_*`     | 搜索        | `search_keyword`, `search_semantic`                                    |
| `index_*`      | 索引/元数据 | `index_tags`, `index_links`, `index_stats`                             |
| `version_*`    | 版本快照    | `version_list`, `version_preview`, `version_restore`                   |
| `skills_*`     | AI Skills   | `skills_list`, `skills_paths`, `skills_create_draft`, `skills_confirm` |
| `settings_*`   | 配置        | `settings_get`, `settings_set`                                         |
| `credential_*` | 凭据        | `credential_set`, `credential_get`                                     |
| `template_*`   | 模板        | `template_list`, `template_apply`                                      |
| `corpus_*`     | 语料库      | `corpus_list`, `corpus_upsert`                                         |
| `assistant_*`  | AI 助理     | `context_assemble`, `ai_send_message`, `tool_confirm`                  |
| `citation_*`   | 引用        | `citation_check`                                                       |
| `research_*`   | 研究        | `research_start`, `research_poll`                                      |
| `writing_*`    | 写作        | `writing_suggest`, `writing_apply`                                     |
| `organize_*`   | 整理        | `organize_run`                                                         |
| `profile_*`    | 个性化      | `profile_set_rule`                                                     |
| `llm_config_*` | LLM 配置    | `llm_config_get`, `llm_config_set`                                     |
| `recycle_*`    | 回收站      | `recycle_list`, `recycle_restore`                                      |
| `graph_*`      | 知识图谱    | `graph_data`                                                           |
| `export_*`     | 导出        | `export_html`, `export_markdown`                                       |
| `document_*`   | 文档级      | `document_check`, `document_apply`                                     |

### 事件（Rust → WebView）

| 事件名                    | 触发时机                 | 载荷                                                           |
| ------------------------- | ------------------------ | -------------------------------------------------------------- |
| `llm:token`               | LLM 流式返回 token       | `{ request_id, token, index }`                                 |
| `llm:done`                | LLM 请求完成             | `{ request_id }`                                               |
| `llm:error`               | LLM 请求失败             | `{ request_id, error }`                                        |
| `file:changed`            | 外部文件变更检测         | `{ path, hash, event_type }`                                   |
| `file:conflict`           | 文件冲突需要用户处理     | `{ path, local_hash, external_hash }`                          |
| `version:created`         | 新版本快照已创建         | `{ file_id, version_id, timestamp }`                           |
| `version:cleanup`         | 自动版本清理完成         | `{ cleaned_count, remaining_count }`                           |
| `skills:changed`          | Skill 确认状态或索引刷新 | （无载荷）                                                     |
| `ai:tool_confirm_request` | Agent 工具需用户确认     | `{ request_id, tool_call_id, tool_name, arguments, preview? }` |

### Agent Skills 数据流

面板（`SkillsPanel`）与 Harness Agent 共用 prompt-only Skills 服务：IPC 只保留列出、创建草稿和确认保存；Agent 工具只保留非平台化的 skill awareness。Iris 不再提供外部 SkillHub / Git / 本地安装入口，也不通过 Skill 声明注册 MCP、脚本、资源读取或专用工作区。用户确认后的 `SKILL.md` 哈希写入 `skills-config.json`，并刷新 `skill_activation_index`（关键词 + 描述 embedding）、emit `skills:changed` 刷新 UI。

### AI Runtime v1.2.1 模块边界

`src-tauri/src/ai_runtime/` 通过 facade 模块保持旧 public import path 可用，同时把大实现迁到 `*_impl.rs`。当前兼容入口：

- `model_gateway.rs` -> `model_gateway_impl.rs`
- `skills.rs` -> `skills_impl.rs`
- `tool_dispatch.rs` -> `tool_dispatch_impl.rs`
- `tool_catalog.rs` -> `tool_catalog_impl.rs`
- `retrieval_broker.rs` -> `retrieval_broker_impl.rs`

调用方继续从原模块名导入。后续新增 runtime 能力应优先落到更小的专责子模块，facade 文件保持薄入口，由模块边界测试保护公开路径。

上下文组装新增短 TTL 内存缓存：`ai_runtime::context_cache`。`context_assemble` 与 `ai_send_message` 复用同一构建路径，缓存 key 包含 scene、note path、query、scope、context strategy、input budget 与 prompt profile。`ai_cache_clear`、runtime clear 与 knowledge reindex 会清空该缓存。

性能基准位于 `src-tauri/benches/ai_benchmarks.rs`，覆盖 skill prompt 注入、长 tool history API body 构造、retrieval query hash 与大文本 guardrail 扫描：

```bash
cargo bench --manifest-path src-tauri/Cargo.toml --bench ai_benchmarks
```

#### Skills 运行时能力边界

| 能力        | 行为                                                                                           |
| ----------- | ---------------------------------------------------------------------------------------------- |
| Prompt 注入 | `rank_skills_for_scene` 匹配后，将 `SKILL.md` 正文拼入 system message                          |
| Prompt only | Skills only contribute confirmed SKILL.md prompt text and scope rules; they do not open tools. |
| 场景匹配    | 优先读 `skill_activation_index` 关键词/描述，文件扫描 fallback；embedding cosine rerank        |
| 确认门禁    | 新建或修改后的 Skill 需要用户确认内容哈希后才会参与注入                                        |
| UI 语义     | **已启用**（config）≠ **已确认**（hash）≠ **本场景注入**（rank>0），`SkillsPanel` 区分展示     |

| 不能做什么      | 说明                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------------ |
| 任意插件执行    | 不会运行 skill 内任意脚本、安装依赖、读取资源目录或注册 MCP                                            |
| 突破 ToolPolicy | `requires_confirmation` 工具仍需用户确认；联网工具受 `web_search_enabled` 约束                         |
| Tool access     | ToolPolicy is derived from task policy, web-search switch, and confirmation strategy, not from Skills. |

Tool 续聊：`ModelGateway.prepare_tool_api_messages` 在每次 LLM 请求前规范化 messages（含 mixed auto+confirm 批次、`skip_stub_ids` 处理 pending confirm）。

---

## 安全模型

### 原则

1. **最小权限**: Tauri capability 系统声明式授权
2. **密钥不落盘**: API Key 仅 OS 凭据管理器存储
3. **传输加密**: 所有 LLM API 请求走 HTTPS
4. **内容隔离**: WebView JS 不能直接访问文件系统或网络
5. **路径安全**: `canonicalize()` + 前缀比对，禁止路径穿越
6. **输入验证**: 所有 IPC 入口 serde 强类型校验
7. **Markdown 渲染安全**: DOMPurify 清洗；CSP 禁止内联脚本

### 防护清单

| 攻击面       | 防护措施                                                                                      |
| ------------ | --------------------------------------------------------------------------------------------- |
| 路径穿越     | `canonicalize()` + 前缀比对                                                                   |
| XSS          | DOMPurify + CSP Header                                                                        |
| IPC 注入     | serde 强类型反序列化                                                                          |
| SQL 注入     | rusqlite 参数化查询                                                                           |
| 依赖劫持     | 本地 `npm run audit:rust`（读取 `.cargo/audit.toml`）/ `npm audit`（CI 尚未接入，见 ROADMAP） |
| 中间人攻击   | HTTPS-only rustls 客户端；证书固定仅在显式 pin 配置时启用                                     |
| 本地数据泄露 | 笔记为明文 `.md`；建议 OS 级全盘加密；临时文件 `secure_delete`                                |

### Content Security Policy

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self';
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: https:;
  connect-src 'self' https://api.openai.com https://api.anthropic.com https://api.minimaxi.com https://api.deepseek.com https://html.duckduckgo.com;
  font-src 'self';
```

---

## 数据库 Schema

### 核心表

```sql
-- 文件索引
CREATE TABLE files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    title       TEXT,
    frontmatter JSON,
    content_hash TEXT NOT NULL,
    word_count  INTEGER DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- 标签
CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE);
CREATE TABLE file_tags (
    file_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
    tag_id  INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (file_id, tag_id)
);

-- 双向链接
CREATE TABLE links (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  INTEGER REFERENCES files(id) ON DELETE CASCADE,
    target_id  INTEGER REFERENCES files(id) ON DELETE CASCADE,
    context    TEXT,
    UNIQUE(source_id, target_id)
);

-- 文档分块（向量嵌入）
CREATE TABLE chunks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER REFERENCES files(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,
    content      TEXT NOT NULL,
    token_count  INTEGER,
    UNIQUE(file_id, chunk_index)
);

CREATE TABLE chunk_embeddings (
    chunk_id   INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding  BLOB NOT NULL
);

-- 版本快照
CREATE TABLE versions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER REFERENCES files(id) ON DELETE CASCADE,
    version_no   TEXT NOT NULL,
    label        TEXT,
    content_hash TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    word_count   INTEGER,
    is_finalized INTEGER DEFAULT 0,
    kind         TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    UNIQUE(file_id, version_no)
);

-- AI Runtime
CREATE TABLE sessions (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    scene      TEXT NOT NULL,
    note_path  TEXT,
    title      TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE session_messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER REFERENCES sessions(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    tool_calls JSON,
    created_at TEXT NOT NULL
);

-- 知识索引
CREATE TABLE semantic_anchors (…);
CREATE TABLE regulation_index (…);
CREATE TABLE genre_templates (…);

-- 其余：knowledge_deposits、user_profile、ai_traces、eval_results、web_page_cache、search_cache、cas_refs
-- 完整 schema 见 src-tauri/migrations/（当前目标态覆盖 001_core 到 042_web_evidence_provider_registry，均含 up/down 脚本）
```

---

## UI 架构

### 布局

```
┌──────────────────────────────────────────────────┬──────────────┐
│  TabBar    笔记 A  │  笔记 B  │  笔记 C      [+]               │
├──────────────────────────────────────────────────┤  AI 面板     │
│ [大纲]                                           │  (可收起)    │
│                                                  │              │
│              TipTap WYSIWYG Editor                │  对话历史     │
│              （主编辑区）                          │              │
│                                                  │  AI 输入框    │
├──────────────────────────────────────────────────┴──────────────┤
│  StatusBar    笔名.md  │  1,420 字  │  LLM 就绪 · 联网 关       │
└─────────────────────────────────────────────────────────────────┘
```

| 区域         | 形态             | 说明                             |
| ------------ | ---------------- | -------------------------------- |
| **编辑器**   | 全宽主区域       | WYSIWYG Markdown                 |
| **AI 面板**  | 右栏，默认 360px | 可拖拽调宽 + `Ctrl+Shift+A` 收起 |
| **大纲**     | 编辑器左侧悬浮   | `Ctrl+Shift+O`                   |
| **标签页栏** | 顶部固定         | 多笔记切换，Ctrl+W 关闭          |
| **状态栏**   | 底部固定         | 路径、字数、AI/联网状态          |

### 命令浮层系统

AI 侧栏为唯一常驻右侧 dock。其余功能通过居中命令浮层（`IrisOverlay`）打开：

| 快捷键         | 组件            | 浮层 size | 说明             |
| -------------- | --------------- | --------- | ---------------- |
| `Ctrl+P`       | QuickOpen       | `compact` | 文件搜索/切换    |
| `Ctrl+Shift+E` | VaultNavigator  | `command` | 文件管理         |
| `Ctrl+Shift+F` | SearchPanel     | `command` | 全文 + 语义搜索  |
| `Ctrl+Shift+V` | VersionTimeline | `wide`    | 版本时间线       |
| `Ctrl+Shift+B` | BacklinksPanel  | `command` | 反向链接         |
| `Ctrl+Shift+T` | TagView         | `command` | 标签聚合         |
| `Ctrl+Shift+G` | GraphView       | `graph`   | 知识图谱         |
| `Ctrl+,`       | SettingsPanel   | `command` | 设置             |
| `Ctrl+S`       | （编辑器）      | —         | 保存笔记（层 1） |
| `Ctrl+Shift+S` | （编辑器）      | —         | 保存 + 版本快照  |
| `Ctrl+Shift+A` | 统一助手侧栏    | —         | 收起/展开        |
| `/`            | SlashCommand    | Popover   | 命令菜单         |

**浮层行为**：

- 全窗 scrim（约 45–55%），盖住含 AI 在内的整窗
- **同时仅一个** 命令浮层；新开替换旧开
- `Esc`、点击 scrim、关闭按钮均可关闭；焦点陷阱

### 编辑器结构

```
.iris-editor
  └── .iris-editor-zoom-scroll（滚动）
        └── .iris-editor-canvas（居中栏 + zoom，max-width: 45rem）
              └── .iris-editor-body
                    └── .ProseMirror
```

**无** `.iris-paper` 卡片、**无** 行线、**无** 纸页阴影。

### 组件目录

```
src/components/
├── ui/                       # shadcn/ui 基础组件（无业务逻辑）
│   ├── button.tsx, input.tsx, card.tsx, dialog.tsx, …
│   ├── ai-composer.tsx, ai-message.tsx, ai-message-stream-pulse.tsx
│   ├── command-list.tsx, kbd.tsx, overlay-chrome.tsx
│   ├── iris-overlay.tsx, iris-surface-menu.tsx, iris-context-menu.tsx
│   └── surface-card.tsx, markdown-error-boundary.tsx
├── layout/                   # 布局组件
│   ├── AppShell.tsx, TabBar.tsx, StatusBar.tsx
│   ├── ConnectivityIndicators.tsx
│   ├── WelcomeEmpty.tsx, DesktopFrame.tsx
│   ├── AppBrandZone.tsx, EditorZoomControl.tsx
│   └── MinimalWindowChrome.tsx, WindowControls.tsx
├── editor/                   # 编辑器相关
│   ├── TipTapEditor.tsx, AiNodeView.tsx
│   ├── SlashCommandList.tsx, EditorOutline.tsx
│   ├── DocumentTitleField.tsx, DocumentTitleContextMenu.tsx
│   └── extensions/（AiStream, HeadingFold, Image, Link, WikiLink, SlashCommand, IrisDocument, gfm-schema）
├── ai/                       # AI 组件
│   ├── UnifiedAssistantPanel.tsx
│   ├── AiComposerContextMenu.tsx, AiMentionPopover.tsx
│   ├── AiMessageBubble.tsx, AiMessageList.tsx, AiMessageSelectionUi.tsx
│   ├── AiRulesPanel.tsx, SkillsPanel.tsx, AssistantAvatar.tsx
│   ├── ContextPacketCard.tsx, ContextPacketDrawer.tsx, ContextScopeChips.tsx
│   ├── ContextStatusBar.tsx, TokenUsageBar.tsx
│   ├── ToolCallBubble.tsx, ToolConfirmDialog.tsx, RuleConfirmDialog.tsx
│   ├── CitationCheckView.tsx, EvidenceChainView.tsx
│   ├── ExecutionPlanPreview.tsx, PatchPreview.tsx, ResearchResultMessage.tsx
│   ├── HarnessActivityStrip.tsx, SessionHistoryDropdown.tsx
│   └── assistant/（ResearchFocusView, DocumentCheckArtifacts）
├── file/                     # 文件管理
│   ├── QuickOpen.tsx, SearchPanel.tsx, FileSheet.tsx
│   ├── BacklinksPanel.tsx, ConflictDialog.tsx
│   ├── VaultNavigator.tsx, RecycleBinSheet.tsx, TemplateEditor.tsx
│   └── version/（VersionTimeline, version-timeline-groups, version-restore-confirm）
├── settings/                 # 设置
│   ├── SettingsPanel.tsx
│   ├── LlmRoutingSection.tsx
│   ├── PromptProfileSection.tsx, AssistantIdentitySection.tsx
├── graph/                    # 知识图谱
│   └── GraphView.tsx
├── tag/                      # 标签
│   └── TagView.tsx
├── brand/                    # 品牌
│   └── IrisMark.tsx, iris-mark-paths.ts
└── common/                   # 通用
    └── ConfirmDialog.tsx
```

---

## 关键设计决策

### 为什么不是 Electron

Electron 的打包体积（150MB+）、内存占用（300-500MB）、安全模型不具竞争力。Tauri 2.x 提供 5-10MB 体积、50-100MB 内存、Rust 后端的声明式权限和天然安全隔离。

### 为什么不做 CRDT 实时同步

Iris **永久不做**实时多人协作。CRDT 会使 Prosemirror 状态、Markdown 往返与索引一致性成本倍增。核心场景是单用户本地笔记。

### 为什么 SQLite 而不是 LanceDB 或 Qdrant

本地部署场景下，PostgreSQL + pgvector 太重；Qdrant 需要独立进程；LanceDB 偏向列式分析。Iris 使用 SQLite 在单文件内完成全文、向量 fallback 与图关系检索；sqlite-vec 作为 optional/experimental 加速路径保留，当前 Windows 构建有阻塞，不作为默认门禁。

### 为什么不用 Block 编辑器

Notion 式 Block 编辑器牺牲了 Markdown 的纯文本可移植性。Iris 的文件即数据哲学要求所有内容必须可以完整表示为 `.md` 纯文本。ProseMirror 在结构化操作和标准 Markdown 之间取得平衡。

---

## 性能标准

| 指标        | v1.2.1 目标                      | 当前状态                        |
| ----------- | -------------------------------- | ------------------------------- |
| 冷启动时间  | < 3 秒（10000 篇笔记）           | 待基准测试                      |
| 热启动时间  | < 400ms                          | 达标                            |
| 打开笔记    | < 50ms                           | 达标                            |
| 打字延迟    | < 16ms（60fps）                  | 达标                            |
| 关键词搜索  | < 100ms（10000 篇）              | 达标                            |
| 语义搜索    | < 500ms                          | 达标（vec0 或 cosine fallback） |
| AI 首 token | < 1 秒（含上下文拼接）           | 达标                            |
| 内存占用    | < 80MB 空闲 / < 200MB 10000 文件 | 待大规格实测                    |
| 打包体积    | < 10MB                           | 达标                            |

### SQLite 优化

```sql
PRAGMA journal_mode=WAL;  PRAGMA synchronous=NORMAL;
PRAGMA cache_size=-32000; PRAGMA mmap_size=268435456;
PRAGMA temp_store=MEMORY; PRAGMA busy_timeout=5000;
```

### 构建优化

```toml
# Cargo.toml [profile.release]
opt-level = 3; lto = true; codegen-units = 1; strip = true; panic = "abort"
```

---

## 设计文档参考

| 文档                                                           | 内容                 |
| -------------------------------------------------------------- | -------------------- |
| [docs/design-system.md](./docs/design-system.md)               | 界面 token、组件规则 |
| [docs/llm-routing.md](./docs/llm-routing.md)                   | LLM 路由与连通性     |
| [docs/eval/semantic-search.md](./docs/eval/semantic-search.md) | 语义搜索评测         |

### Web Evidence Provider Boundary

MCP is a persisted Web Evidence Provider only when mapped to `web.search` and/or `web.fetch`. DuckDuckGo is the only native web evidence fallback surfaced in Management Center; MiniMax remains a normal LLM provider and is not a web evidence backend. Provider configuration stores transport JSON and OS credential references; raw secrets are rejected. Ordinary evidence detail DTOs omit provider process fields, while audit/diagnostics paths may retain provider id, provider kind, raw result hash, extraction method, mapping status, and circuit state.
