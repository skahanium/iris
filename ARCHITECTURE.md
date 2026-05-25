# 技术架构

> 本文档描述 Iris 的系统架构、数据流、安全模型和关键设计决策。

---

## 概览

```
┌─────────────────────────────────────────────────────────┐
│                     WebView (React)                      │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐ │
│  │ TipTap   │ │ AI Panel │ │ File     │ │ Search     │ │
│  │ Editor   │ │ (内联/AI) │ │ Explorer │ │ Panel      │ │
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
│  │  │ File   │ │ LLM   │ │Search │ │ Crypto  │ │          │
│  │  │ System │ │ Engine│ │Engine │ │ Module  │ │          │
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
- **不负责**: 文件操作、网络请求、加密、数据持久化
- **关键库**:
  - `@tiptap/react` — 编辑器核心
  - `prosemirror-*` — 文档模型、变换、状态管理
  - `tailwindcss` + `shadcn/ui` — 样式系统
  - `@tauri-apps/api` — IPC 调用桥接

### 逻辑层 — Rust (Tauri 2.x)

- **职责**: 所有需要系统权限或原生性能的操作
- **关键 crate**:
  - `rusqlite` — SQLite 数据库操作
  - `fastembed-rs` — 本地嵌入生成（AllMiniLML6V2）
  - `chunk_embeddings` BLOB + Rust 余弦 — v0.1 语义检索（非 sqlite-vec 虚拟表）
  - `aes-gcm` / `argon2` — 加密与密钥派生
  - `reqwest` — HTTP 客户端（LLM API 调用）
  - `tokio` — 异步运行时
  - `serde` / `serde_json` — 序列化
  - `notify` — 文件系统事件监听

### 存储层

- **`.md` 文件**: 用户笔记的权威数据源。每个文件独立存在，可用任意编辑器打开。
- **SQLite 数据库**: 索引和缓存层。所有数据均可从 `.md` 文件重建，数据库删除不影响用户数据。
- **OS 凭据管理器**: 仅存储加密后的 API Key，不存储用户内容。


## 数据流

### 1. 编辑器 → 文件（用户输入）

```
用户键盘输入 → TipTap 编辑操作
  → ProseMirror Transform (内存状态变化)
  → MarkdownSerializer (节点树 → 字符串)
  → Tauri IPC: write_file(path, content)
  → Rust: fs::write()
  → 异步: 更新 SQLite 索引 (解析 frontmatter、标签、链接)
```

### 2. AI 请求 → 编辑器（AI 生成）

```
用户触发 AI 命令 (选中文本 + 操作类型)
  → Tauri IPC: llm_generate(params)
  → Rust LLM Engine:
      1. 收集上下文 (当前文档、关联笔记、system prompt)
      2. 构建请求 (OpenAI-compatible API format)
      3. 通过 reqwest 发送 HTTPS POST
      4. 流式读取 response body (Server-Sent Events / chunked)
      5. 每个 chunk 通过 Tauri Event emit 到 WebView
  → WebView: TipTap 插入 ai-stream Node, 逐 token 更新内容
  → 用户接受: write_file() + 关闭 ai-stream node
  → 用户拒绝: 移除 ai-stream node, 恢复编辑器状态
```

### 3. 语义搜索

```
用户输入自然语言查询
  → Tauri IPC: search_semantic(query)
  → Rust Search Engine (v0.1):
      1. 查询文本 → fastembed (AllMiniLML6V2, 384-dim) → 查询向量
      2. 读取 chunk_embeddings BLOB，在 Rust 中计算余弦相似度并排序
      3. 返回 Top-K 结果 (文件路径 + 片段 + 相似度分数)
  → WebView: 搜索结果列表，点击跳转到对应笔记

注：v0.1 未使用 sqlite-vec 虚拟表；评测见 docs/eval/semantic-search.md。
```

### 4. 文件外部修改同步

```
外部编辑器修改 .md 文件
  → notify crate 监听到文件变更事件
  → Rust: 计算文件哈希 (SHA-256)
  → 策略判断:
      L1: 当前文件未在编辑器中打开 → 静默更新 SQLite 索引
      L2: 文件在编辑器中打开，但未在修改区域 → 静默更新 TipTap 文档树
      L3: 文件在编辑器中打开，且外部修改与当前编辑区域重叠 → 通知 WebView 弹出 diff 视图
  → 用户抉择后 → write_file() 或 discard()
```

### 5. 版本快照

```
触发条件 (Ctrl+S 或 定时扫描)
  → Rust: 计算当前文件内容 SHA-256
  → 与最近一次快照的 SHA-256 比较
  → 无变更 → 跳过
  → 有变更 → 写入 .iris/versions/<file_id>/<timestamp>.md
             → INSERT INTO versions (元数据记录)
             → emit("version:created") 通知 WebView 刷新版本列表
```

### 6. 版本恢复

```
用户在版本面板选择目标版本 → IPC: version_preview(id)
  → Rust: 读取 .iris/versions/<file_id>/<timestamp>.md
  → 返回内容到 WebView 的只读预览窗口

用户点击恢复 → IPC: version_restore(id)
  → 第一步: 对当前编辑器内容执行快照（保护操作前状态）
  → 第二步: 读取目标版本内容 → 替换编辑器内容
  → 第三步: write_file() 写回主 .md 文件
  → 关闭版本面板
```

### 7. 版本清理

```
触发条件 (应用启动 + 每 24 小时)
  → SQL: DELETE FROM versions
         WHERE is_finalized = 0
         AND created_at < datetime('now', '-7 days')
  → 对每条待删记录:
      删除 .iris/versions/<file_id>/<timestamp>.md
      删除 versions 表中的元数据行
  → emit("version:cleanup") 通知 WebView 更新
```

---

## IPC 协议

Tauri 的命令式 IPC 基于 JSON 序列化。所有 Rust 函数通过 `#[tauri::command]` 宏暴露。

### 命令分类

| 前缀 | 模块 | 示例 |
|------|------|------|
| `file_*` | 文件系统 | `file_list`, `file_read`, `file_write`, `file_delete`, `file_rename` |
| `llm_*` | AI 集成 | `llm_generate`, `llm_chat`, `llm_abort`, `llm_providers` |
| `search_*` | 搜索 | `search_keyword`, `search_semantic`, `search_reindex` |
| `index_*` | 索引/元数据 | `index_tags`, `index_links`, `index_stats` |
| `version_*` | 版本快照 | `version_list`, `version_create`, `version_preview`, `version_restore`, `version_delete`, `version_finalize`, `version_rename` |
| `crypto_*` | 加密 | `crypto_lock`, `crypto_unlock`, `crypto_status` |
| `settings_*` | 配置 | `settings_get`, `settings_set`, `settings_reset` |

### 事件（Rust → WebView）

| 事件名 | 触发时机 | 载荷 |
|--------|----------|------|
| `llm:token` | LLM 流式返回每个 token | `{ request_id, token, index }` |
| `llm:done` | LLM 请求完成 | `{ request_id }` |
| `llm:error` | LLM 请求失败 | `{ request_id, error }` |
| `file:changed` | 外部文件变更检测 | `{ path, hash, event_type }` |
| `file:conflict` | 文件冲突需要用户处理 | `{ path, local_hash, external_hash }` |
| `version:created` | 新版本快照已创建 | `{ file_id, version_id, timestamp, is_finalized }` |
| `version:cleanup` | 自动版本清理完成 | `{ cleaned_count, remaining_count }` |

---

## 安全模型

### 原则

1. **最小权限**: Tauri 的 capability 系统声明式授权，不申请不需要的权限
2. **密钥不落盘**: API Key 仅存储在 OS 凭据管理器
3. **传输加密**: 所有 LLM API 请求走 HTTPS
4. **内容隔离**: WebView 中的 JS 不能直接访问文件系统或网络，必须通过 IPC
5. **路径安全**: 所有文件操作限制在用户指定的笔记目录内，禁止路径穿越
6. **输入验证**: 所有 IPC 入口对参数做严格校验，拒绝非法输入
7. **Markdown 渲染安全**: 渲染后的 HTML 经过 DOMPurify 清洗，禁止执行脚本

### 防护清单

| 攻击面 | 防护措施 |
|--------|----------|
| 路径穿越 | Rust 侧 `canonicalize()` + 前缀比对，拒绝 `../` 跳出笔记目录 |
| XSS | DOMPurify 清洗渲染后的 Markdown HTML；CSP Header 禁止内联脚本 |
| IPC 注入 | 所有 `#[tauri::command]` 参数使用 serde 强类型反序列化，拒绝意外字段 |
| SQL 注入 | 使用 `rusqlite` 的参数化查询，禁止字符串拼接 SQL |
| 依赖劫持 | CI 中 `cargo audit` + `pnpm audit`；锁定 `Cargo.lock` / `pnpm-lock.yaml` |
| 中间人攻击 | HTTPS 证书固定 (certificate pinning) for LLM API endpoints |
| 本地数据泄露 | 加密目录由 AES-256-GCM 保护；临时文件写入后立即 `shred` 擦除 |

### Tauri Capabilities（示例）

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "fs:allow-read-text-file",
    "fs:allow-write-text-file",
    "fs:allow-exists",
    "fs:allow-mkdir",
    "fs:scope:notes-directory"
  ]
}
```

`fs:scope:notes-directory` 限制文件访问范围仅限用户选择的笔记目录，WebView 无法通过 IPC 访问系统其他路径。

### Content Security Policy

WebView 加载的 HTML 页面设置严格 CSP：

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self';
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: https:;
  connect-src 'self' https://api.openai.com https://api.anthropic.com https://api.bing.microsoft.com;
  font-src 'self';
```

- 禁止 `unsafe-eval`，杜绝 `eval()` / `new Function()` 执行路径
- `connect-src` 仅放行已知 LLM 和搜索 API 域名
- 前端调用 `invoke()` 走 Tauri IPC，不走 `fetch()`


## 数据库 Schema

### 核心表

```sql
-- 文件索引
CREATE TABLE files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,          -- 相对于笔记目录的路径
    title       TEXT,                           -- 从文件名或 frontmatter 提取
    frontmatter JSON,                           -- YAML frontmatter 解析结果
    content_hash TEXT NOT NULL,                 -- SHA-256
    word_count  INTEGER DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- 标签索引
CREATE TABLE tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE
);

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
    context    TEXT,                           -- 链接周围的上下文片段
    UNIQUE(source_id, target_id)
);

-- 文档分块（用于向量嵌入）
CREATE TABLE chunks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER REFERENCES files(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,             -- 分块序号
    content      TEXT NOT NULL,                 -- 分块文本
    token_count  INTEGER,
    UNIQUE(file_id, chunk_index)
);

-- 向量嵌入（v0.1 实际 schema：BLOB 存储，见 migrations/001_core.sql）
CREATE TABLE chunk_embeddings (
    chunk_id   INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding  BLOB NOT NULL                    -- f32 小端；AllMiniLML6V2 为 384 维
);

-- v0.2+ 可选：sqlite-vec 虚拟表以加速大规模近似检索
-- CREATE VIRTUAL TABLE vec_chunks USING vec0(...);

-- 版本快照元数据（内容全文存储在 .iris/versions/ 目录中）
CREATE TABLE versions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER REFERENCES files(id) ON DELETE CASCADE,
    version_no   TEXT NOT NULL,                 -- 毫秒级时间戳，如 20260525143052123
    label        TEXT,                           -- 用户可自定义的版本名（可选）
    content_hash TEXT NOT NULL,                 -- 快照内容的 SHA-256
    storage_path TEXT NOT NULL,                 -- 相对于 .iris/versions/ 的快照文件路径
    word_count   INTEGER,
    is_finalized INTEGER DEFAULT 0,             -- 0=自动快照 1=已定稿
    created_at   TEXT NOT NULL,
    UNIQUE(file_id, version_no)
);

CREATE INDEX idx_versions_file_id ON versions(file_id);
CREATE INDEX idx_versions_finalized ON versions(is_finalized);
CREATE INDEX idx_versions_created ON versions(created_at);

-- 应用设置
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value JSON NOT NULL
);
```

### 版本系统

#### 触发策略

| 触发方式 | 行为 |
|----------|------|
| 手动保存 (Ctrl+S) | 立即生成一次快照 |
| 定时快照 | 每 N 分钟自动扫描已打开的笔记，仅在内容有变更时生成快照 |
| 定稿 (Finalize) | 立即创建新快照并标记 `is_finalized = 1`，不可被自动清理 |

#### 存储结构

```
<笔记目录>/
├── note-a.md
├── note-b.md
└── .iris/                  # 隐藏目录
    ├── config.json          # Iris 本地配置
    └── versions/            # 版本快照存储
        ├── <file_id>/
        │   ├── 20260525143052123.md
        │   ├── 20260526120000000.md
        │   └── ...
```

#### 清理策略

- **自动版本**（`is_finalized = 0`）：创建后存留 7 天，到期自动删除
- **定稿版本**（`is_finalized = 1`）：永不自动删除，只能手动删除
- 清理时机：应用启动时 + 每 24 小时定时执行一次
- 清理逻辑：先删快照文件，再删 SQLite 记录，保持文件和元数据一致性

#### 版本号

- 格式：17 位毫秒级 Unix 时间戳（如 `20260525143052123`）
- 全局唯一，天然按时间排序
- 定稿版本可额外设置一个可读的 `label`（如"提交专栏"、"会议前版本"）

#### 版本恢复

1. 用户在版本管理面板选择目标版本 → 预览
2. 点击恢复 → 当前编辑器内容被替换为目标版本内容
3. 替换前自动创建一次快照（保护"恢复前"的状态，可撤销）


## 分块策略

语义搜索的质量更依赖于分块策略而非向量模型选择。Iris 采用以下策略：

1. **Markdown 节点边界分块**: 在标题 (`#`/`##`/`###`)、段落分隔、列表块、代码块边界处分割
2. **最小/最大 chunk 大小**: 最小 100 token，最大 512 token（基于 fastembed 的 tokenizer 计算）
3. **重叠窗口**: 相邻 chunk 之间有 20% 的内容重叠，减少边界信息丢失
4. **动态分段**: 代码块不会被分割；列表项尽量保持在同一个 chunk 内


## Markdown ↔ ProseMirror 往返

这是整个系统中最敏感的数据转换路径。策略：

### 解析（.md → ProseMirror Node Tree）
- 使用 `markdown-it` 或 `remark` 解析为 AST
- AST 映射到 ProseMirror schema 节点类型
- 不支持的 Markdown 语法降级为普通文本节点，不丢失数据

### 序列化（ProseMirror Node Tree → .md）
- ProseMirror 节点类型按 schema 定义的 `toMarkdown()` 方法序列化
- GFM 扩展（表格、任务列表、脚注）有专用序列化器
- 序列化后内容与磁盘文件哈希比对，仅在有变更时写入

### 往返测试套件
```
对每种 Markdown 语法：
  .md string → parse → Node Tree → serialize → .md' string
  assert: .md == .md' or diff is semantically equivalent
```


## UI 架构

### 布局

```
┌──────────────────────────────────────────────────┬──────────────┐
│  TabBar    笔记 A  │  笔记 B  │  笔记 C      [+]               │
├──────────────────────────────────────────────────┤  AI 面板     │
│ [大纲]                                           │  (固定右栏)   │
│                                                  │              │
│              TipTap WYSIWYG Editor                │  对话历史     │
│              （主编辑区，所见即所得）                │              │
│                                                  │  AI 输入框    │
│                                      [反向链接]   │              │
├──────────────────────────────────────────────────┴──────────────┤
│  StatusBar    笔名.md  │  1,420 字  │  AI 空闲                   │
└─────────────────────────────────────────────────────────────────┘
```

| 区域 | 形态 | 说明 |
|------|------|------|
| **编辑器** | 全宽主区域 | 所见即所得 Markdown 编辑，占满所有可用空间 |
| **AI 面板** | 固定右栏，280px 宽 | 可一键折叠。对话式 AI 交互，流式渲染回复 |
| **大纲** | 编辑器左侧悬浮 | 快捷键 `Ctrl+Shift+O` 切换显示，浮动在编辑器左边缘，不占用布局宽度 |
| **反向链接** | 编辑器右侧悬浮 | 同上，浮动在编辑器右边缘，位于 AI 面板左侧 |
| **标签页栏** | 顶部固定 | 多笔记标签页，拖拽排序，Ctrl+W 关闭 |
| **状态栏** | 底部固定 | 当前文件路径、字数、AI 处理状态指示 |

### 悬浮层系统

无左侧固定边栏。所有文件操作和次级功能通过快捷键呼出浮层：

| 快捷键 | 组件 | 形态 | 说明 |
|--------|------|------|------|
| `Ctrl+P` | QuickOpen | 居中 Dialog | 文件搜索/切换，类似 VS Code Quick Open |
| `Ctrl+Shift+F` | SearchPanel | 右侧 Sheet 滑出 | 全文关键词 + 语义搜索 |
| `Ctrl+Shift+V` | VersionTimeline | 右侧 Sheet 滑出 | 笔记版本时间线，预览与恢复 |
| `Ctrl+Space` | AiPanel 或 AiCommand | 聚焦右栏 / 居中 Dialog | 聚焦 AI 对话面板；编辑器内未选中文本时弹出 AI 命令选择 |
| `Ctrl+Shift+O` | OutlineWidget | 编辑器左侧浮动 | 标题大纲树，点击跳转到对应段落 |
| `/` | SlashCommand | 光标处浮动 Popover | AI 和内建命令菜单 |
| 选中文本 | FloatingToolbar | 选区上方浮动 | 粗体/斜体/链接；AI 内联操作；**发送到 AI 面板** |
| 选中文本 + 拖拽 | — | 拖拽到右栏 AI 面板 | 将选区作为上下文传递给 AI，面板自动展开 |

所有 Dialog 和 Sheet 使用 shadcn/ui 组件，蒙版 `bg-black/40`，`Esc` 关闭。Sheet 滑出方向统一为右侧。

### 编辑器 → AI 面板的上下文传递

避免"选中 → 复制 → 粘贴到 AI 输入框"的粗暴流程。文本向 AI 的传递是**结构化的上下文注入**，不是字符串搬运。

#### 传递方式

| 方式 | 交互 | 效果 |
|------|------|------|
| 浮动工具条按钮 | 选中文本 → 点击工具条中的 `SendToAi` 按钮 | 选区作为引用块注入 AI 对话上下文 |
| 拖拽 | 选中文本 → 拖拽到右侧 AI 面板区域 | 同上，拖拽过程显示半透明预览卡 |
| 快捷键 | 选中文本 → `Ctrl+Enter` | 选区作为引用块注入并自动让 AI 面板获得焦点 |

#### 上下文块在 AI 面板中的呈现

选中的文本在 AI 对话中以**引用卡**形式呈现，而非混入用户输入框：

```
┌─ 引用自「笔记 A.md」──────────────────────┐
│                                            │
│   选中的文本内容...                         │
│   (最多显示 5 行，超出显示"展开")           │
│                                            │
│   上下文: 标题「第二节：架构设计」附近       │
│                   [移除引用] [仅此次]       │
└────────────────────────────────────────────┘

用户: 请用更简洁的语言改写这段话           [发送]
```

- 引用卡显示来源文件、所属章节标题、选中内容的摘要
- 用户可移除该引用（不发送）、或设置为"仅此次"（只在当前轮对话中使用）
- 引用不污染 AI 输入框内容，用户问题与上下文在语义上是分离的
- AI 响应中如引用原文，在编辑器侧对应段落有高亮联动

#### 实现要点

- 前端: TipTap 选区 → `editor.getJSON()` 提取选中区域的结构化信息（段落 ID、所在标题层级、前后文边界）
- Rust: 接收结构化上下文后，拼入 LLM 请求的 `system` 或 `user` 消息中，附带位置元数据
- AI 响应中的引用标记解析后在编辑器侧高亮对应段落

### 配色方案

深色主题为默认，面向长时间写作场景。

| 层级 | CSS Token | 色值 | 用途 |
|------|-----------|------|------|
| 编辑区背景 | --background | `#0d0d0d` | 编辑器主背景 |
| 面板色 | --panel | `#171717` | AI 面板、Sheet、Dialog |
| 卡片色 | --card | `#1a1a1a` | AI 输入框、搜索框 |
| 分割线 | --border | `#262626` | 面板与编辑器的边界 |
| 主色调 | --primary | `#a78bfa` (violet-400) | 按钮、选中态、链接、AI 标识 |
| 主色调悬停 | --primary-hover | `#c4b5fd` (violet-300) | hover/focus 状态 |
| 一级文字 | --text-primary | `#e5e5e5` | 标题、正文 |
| 二级文字 | --text-secondary | `#a3a3a3` | 元信息、辅助文字 |
| 三级文字 | --text-tertiary | `#737373` | 占位符、禁用态 |

亮色主题备选，在设置中切换：

| 层级 | 色值 |
|------|------|
| 编辑区 | `#fafafa` |
| 面板 | `#f5f5f5` |
| 主色 | `#7c3aed` (violet-600) |

### 字体

| 场景 | 推荐 | CSS fallback |
|------|------|-------------|
| 编辑器正文 | JetBrains Mono / Iosevka | `"JetBrains Mono", "Iosevka", monospace` |
| 代码块 | Fira Code / JetBrains Mono | `"Fira Code", "JetBrains Mono", monospace` |
| UI 文本 | 系统原生字体栈 | `-apple-system, "Microsoft YaHei", "PingFang SC", sans-serif` |

### 图标系统

使用 **lucide-react**，shadcn/ui 原生依赖。24px 纯色描边，stroke-width 1.5。

| 功能标识 | 图标 | 
|----------|------|
| 文件搜索 | `Search` |
| AI 功能 | `Sparkles` |
| 版本历史 | `GitBranch` |
| 大纲 | `ListTree` |
| 反向链接 | `Link2` |
| 定稿版本 | `Bookmark` |
| 重试/刷新 | `RotateCw` |
| 确认/接受 | `Check` |
| 发送到 AI | `ArrowRightToLine` |
| 关闭/拒绝 | `X` |

### 组件目录

```
src/components/
├── ui/                       # shadcn/ui 基础组件（不包含业务逻辑）
│   ├── button.tsx
│   ├── input.tsx
│   ├── card.tsx
│   ├── command.tsx
│   ├── dialog.tsx
│   ├── sheet.tsx
│   ├── dropdown-menu.tsx
│   ├── scroll-area.tsx
│   ├── tooltip.tsx
│   ├── badge.tsx
│   └── ...
├── layout/
│   ├── AppShell.tsx           # 主布局：编辑器 + 右栏 AI 面板 + 状态栏
│   ├── StatusBar.tsx          # 底部状态栏
│   └── TabBar.tsx             # 顶部标签页
├── editor/
│   ├── TipTapEditor.tsx       # WYSIWYG 编辑器主组件
│   ├── AiNodeView.tsx         # AI 生成内容的节点渲染（接受/重试/回退）
│   ├── SlashCommand.tsx       # / 弹出命令菜单
│   ├── FloatingToolbar.tsx    # 选中文本浮动工具条
│   ├── OutlineWidget.tsx      # 编辑器左侧大纲悬浮
│   └── BacklinksWidget.tsx    # 编辑器右侧反向链接悬浮
├── ai/
│   ├── AiPanel.tsx            # 右栏 AI 对话面板（含流式渲染）
│   └── AiStatusBadge.tsx      # 状态栏 AI 处理状态指示
├── file/
│   ├── QuickOpen.tsx          # Ctrl+P 文件搜索切换
│   ├── SearchPanel.tsx        # Ctrl+Shift+F 全文/语义搜索
│   └── VersionTimeline.tsx    # Ctrl+Shift+V 版本时间线
└── outline/
    └── OutlinePanel.tsx       # 大纲和反向链接共享的数据逻辑
```

---

## AI 效率架构

> 面向 DeepSeek 的 AI 调用优化策略。

### 模型能力基线

| 特性 | DeepSeek V4 Flash / V4 Pro | 说明 |
|------|-----------------|------|
| 上下文窗口 | 1M tokens | 可单次加载整部书籍 + 数十篇关联笔记全文 |
| 输出长度 | 8K~32K tokens | 支持长文生成（论文、报告、代码审计） |
| 推理模式 | R1 深度思考 | 复杂任务自动切换，响应包含思维链 |
| API 兼容 | OpenAI 格式 | 适配层极薄，切换提供商成本低 |
| Tool Calling | 支持 | 为内置 AI 工具链（如联网搜索、索引查询）预留，非第三方插件 |
| 价格 | 极低 | 后台 AI 任务（自动标签、摘要、查询改写）可高频调用 |

### 上下文缓存（降成本、提速度）

避免同一篇笔记反复对话时重复传输全文。

```
Session 周期内缓存层级：

┌─ L1: System Prompt ───── 应用启动后首次构建，Session 内不变，永久缓存
├─ L2: 当前笔记全文 ────── 按 content_hash 缓存，文件未变即复用
├─ L3: 关联笔记/搜索片段 ── 按 query_fingerprint + doc_hash 组合缓存
└─ L4: 对话历史摘要 ────── 窗口占用超过阈值时自动压缩为摘要
```

| 缓存层 | 粒度 | 失效条件 | 命中效果 |
|--------|------|----------|----------|
| System Prompt | 每 Session 构建一次 | 用户修改 AI 设置 | 省 500-2000 tokens/请求 |
| 当前笔记全文 | content_hash 未变 | 编辑器内容变更 | 省全文 tokens |
| 关联上下文 | 文件 hash + 查询语义 hash | 任一变更 | 省额外文档 tokens |
| 对话历史摘要 | 每 15 轮自动生成 | 新对话轮次触发重算 | 省历史 tokens，保留关键决策 |

实现方式：
- Rust 侧 `ContextCache` 结构：`key = (note_hash, query_fingerprint)`，`value = (tokens, cached_content, expires_at)`
- 每轮请求前比对缓存的 hash，跳过已缓存内容
- 用户每次编辑后 content_hash 即变，L2 层自动失效

### 1M 上下文窗口策略

1M token 窗口极大简化了对话管理，但需合理利用：

| 窗口占用 | 策略 |
|----------|------|
| < 50%（~500K tokens） | 不压缩，完整保留对话历史和全部上下文 |
| 50-75%（500K-750K） | 压缩早期对话轮次为摘要（保留决策和结论，丢弃中间编辑指令） |
| > 75%（750K+） | 裁剪最早的非关键轮次，提醒用户当前上下文接近上限 |

实际使用场景下，500K tokens 可承载约 30 万汉字，对个人笔记对话绰绰有余。压缩阈值从原来的 70% 上调至 75%，避免过早丢失上下文。

### 请求去重与取消

```
用户快速连点两次"总结"按钮
  → RequestManager 检测到 (note_hash, operation_type) 正在飞行中
  → 返回同一个 Promise，不发起第二个请求
  → 两个按钮的回调共享同一个流式响应
```

- `llm_abort` 命令立即中断飞行中的请求（`reqwest::RequestBuilder` + `AbortHandle`）
- 编辑器内容变更时，自动中止基于旧内容的 AI 请求
- 切换到不同笔记时，中止上一个笔记的对话请求

### 嵌入向量缓存

- 文件未变更 → `chunks` 表已有 embedding BLOB → 跳过重新生成
- 文件变更 → 按段落 hash 比对，仅重新嵌入受影响的 chunk
- 批量嵌入：应用空闲时后台预计算未处理的 chunk

### 对话 Session 管理

```
Session 生命周期：
  打开笔记 → 创建 Session
    ↓
  多轮对话 → 1M 窗口内自由扩展
    ↓ (窗口 > 75% 时)
  压缩早期轮次为摘要 → 继续对话
    ↓
  关闭笔记 → 保留 Session 摘要（200 字）
    ↓
  重新打开 → 提示"续接上次对话？"
```

| 参数 | 默认值 |
|------|--------|
| 最大历史轮次（不压缩） | 20 轮 |
| 压缩触发阈值 | 上下文窗口 75% |
| 摘要保留内容 | 用户确认的决策、AI 产出的结论 |
| 摘要丢弃内容 | 中间编辑指令（"把标题改成"等） |
| 跨 Session 记忆 | 200 字摘要，仅按笔记存储 |

### R1 思维链在 UI 中的呈现

DeepSeek V4 Pro 的深度推理过程以折叠块形式展示：

```
┌─ AI 响应 ──────────────────────────────────┐
│  ◉ 思考过程 (V4 Pro 深度思考)    [展开]     │
│  ┌─────────────────────────────────────────┐│
│  │ 用户问的是"为什么选 SQLite"...            ││
│  │ 需要从成本、复杂度、性能三个角度回答...    ││
│  │ ...                                     ││
│  └─────────────────────────────────────────┘│
│                                             │
│  采用 SQLite 而非 PostgreSQL 的理由：         │
│  1. 零运维成本...                            │
│                                             │
│                         [接受] [重试] [回退]  │
└─────────────────────────────────────────────┘
```

- 思维链默认折叠，点击展开
- 实现：解析 V4 Pro API 返回的 `reasoning_content` 字段
- 思维链内容不参与后续对话轮次的上下文拼接（仅保留 `content` 部分），避免污染

### 模型切换策略

| 任务类型 | 推荐模型 | 理由 |
|----------|----------|------|
| 日常改写/翻译/总结 | DeepSeek V4 Flash | 速度快，成本低 |
| 复杂推理（案情分析、论文大纲、代码审计） | DeepSeek V4 Pro | 深度思考模式 |
| 快速补全/续写 | DeepSeek V4 Flash | 低延迟优先 |
| 语义搜索查询改写 | DeepSeek V4 Flash | 高频调用，成本敏感 |

### 联网搜索

当 AI 需要实时信息时，Rust 后端通过搜索引擎获取结果，作为额外上下文注入 LLM 请求。

**提供商策略**：

| 提供商 | 方式 | 费用 | API Key | 结果质量 | 推荐用途 |
|--------|------|------|---------|----------|----------|
| DuckDuckGo | HTML 抓取 + 解析 | 免费 | 不需要 | 中等 | 默认，开箱即用 |
| Bing Search API v7 | REST API | 1,000 次/月免费 | 需要 | 高 | 对结果质量要求高时升级 |

**降级链**：Bing 已配置 → 优先使用；Bing 请求失败 / 配额耗尽 / 未配置 API Key → 自动降级到 DuckDuckGo。

**触发方式**：
- AI 对话输入框旁设 `联网` 切换开关，默认关闭
- 开启后，当前轮及后续对话轮次自动携带搜索上下文
- `/` 命令中可指定"联网搜索 + 查询"，无需手动开启开关

**数据流（DuckDuckGo，默认路径）**：

```
用户在 AI 面板提问（联网开关开启）
  → Rust: 提取用户最近一次提问作为搜索 query
  → reqwest → DuckDuckGo HTML 搜索页
      GET https://html.duckduckgo.com/html/?q={query}
      Header: User-Agent (标准浏览器 UA)
  → scraper crate 解析 HTML → 提取 top-5 结果的 (title, url, snippet)
  → 格式化为引用块拼入 LLM 请求的 user 消息：
      """
      以下是与问题相关的网页搜索结果，请参考这些信息回答：
      [1] 标题: xxx
          链接: https://...
          摘要: xxx...
      [2] ...

      用户问题: {原始问题}
      """
  → LLM 响应中引用搜索来源时保留 [n] 标记，UI 渲染为可点击链接
```

**数据流（Bing，升级路径）**：
```
  → reqwest → Bing Search API v7
      GET https://api.bing.microsoft.com/v7.0/search?q={query}&count=5&mkt=zh-CN
      Header: Ocp-Apim-Subscription-Key: {BING_API_KEY}
  → 解析 JSON 响应 → 后续步骤同上
```

**API Key 配置（仅 Bing 需要）**：
- 存储在 OS 凭据管理器，key 名 `iris/bing-search`
- 在设置 → AI → 联网搜索 中选择提供商、填入 Key
- 未配置任何提供商时，DuckDuckGo 自动生效

**防反爬保护（DuckDuckGo）**：
- 每次请求间隔最小 2 秒，避免被 IP 封禁
- 使用常见浏览器 User-Agent 伪装
- 连续失败 3 次后暂停 5 分钟，通知用户"暂时不可用，已降级至其他搜索"

**缓存**：
- 相同 query 的搜索结果缓存 30 分钟，避免重复请求
- 缓存 key = `md5(query)`，存储在内存 LRU（不落盘）

**安全**：
- Bing API 请求走 HTTPS，Key 仅出现在请求 Header 中
- DuckDuckGo 请求匿名，不携带任何用户标识
- 用户笔记内容不随搜索请求发送，仅发送用户主动输入的查询关键词
- 搜索结果不在本地持久化，仅当前会话内有效

用户可在设置中为不同任务类型预设模型，AI 面板也可手动切换当前对话使用的模型。

---

## 性能标准

> Iris 的性能目标是 10,000 篇笔记规模下体验零卡顿。对标 Obsidian 的冷启动（5-10 秒）和 Notion 的页加载（2-5 秒），Iris 必须做到数量级优化。

### 性能目标

| 指标 | 目标值 | 测量条件 |
|------|--------|----------|
| 冷启动时间 | < 1 秒 | 10,000 篇笔记，首次启动（SQLite 索引预热后） |
| 热启动时间 | < 400ms | 应用已在后台最小化，恢复前台 |
| 打开笔记 | < 50ms | 任意大小的 .md 文件（上限 10MB） |
| 打字延迟 | < 16ms | 单次键盘到屏幕更新的端到端延迟（保持 60fps） |
| 关键词搜索 | < 100ms | 10,000 篇笔记全文搜索，返回 Top-20 |
| 语义搜索 | < 500ms | 10,000 chunk 向量检索 + 排序 |
| 文件切换 | < 30ms | Ctrl+P 键入后结果即时显示；选中后 < 30ms 打开 |
| AI 首 token | < 1 秒 | 含上下文拼接时间（不含 LLM API 延迟） |
| 内存占用 | < 80MB（空闲）/ < 200MB（10K 文件） | 不含 WebView 的 GPU 纹理内存 |
| 打包体积 | < 10MB | 不含 WebView2（系统自带） |

### Rust 后端性能策略

#### 文件 I/O
- 所有文件操作通过 `tokio::fs` 异步执行，不阻塞主线程
- 大文件（> 1MB）使用 `BufReader` 分块读取，避免一次性加载全部内容到内存
- `notify` (文件监听) 使用 debounce 策略，500ms 内同一文件的连续事件合并为一次处理
- 写入操作先写临时文件 → `fs::rename()` 原子替换，防止写入中断导致文件损坏

#### SQLite 优化
```sql
PRAGMA journal_mode=WAL;         -- 写操作不阻塞读，并发性能显著提升
PRAGMA synchronous=NORMAL;        -- 平衡安全与性能（WAL 模式下崩溃恢复可靠）
PRAGMA cache_size=-8000;          -- 8MB 页缓存（默认 2MB）
PRAGMA mmap_size=268435456;       -- 256MB 内存映射 I/O
PRAGMA temp_store=MEMORY;         -- 临时表放在内存
PRAGMA busy_timeout=5000;         -- 锁等待 5 秒后报错而非无限等待
```

- 批量写入（如首次索引 10,000 文件）使用显式事务，单次 commit vs 10,000 次 commit 性能差 ~100 倍
- PRAGMA optimize 在应用退出前执行，为下次启动的分析器预热
- 向量检索使用 `sqlite-vec` 的近似搜索模式（若支持）而非全量余弦计算

#### 并发策略
- 索引重建运行在独立 `tokio::task::spawn_blocking` 中，不阻塞 UI 线程
- 向量嵌入生成使用 `fastembed-rs` 的批量模式 + Rayon 并行，充分利用多核
- LLM 流式请求与 UI 渲染异步解耦：Rust 侧 emit event → WebView 收到后批量更新 DOM
- IPC 命令按优先级分级：编辑器操作（写文件）> AI 请求 > 索引更新 > 向量计算

#### 内存管理
- 不在内存中持有超过 5 篇笔记的完整文本，不活跃的 Tab 节点树序列化到磁盘缓存
- AI 对话历史使用环形缓冲区，保留最近 100 轮，超过后压缩为摘要
- 向量嵌入仅在检索时加载，不常驻内存；使用 mmap 读取 `vec_chunks` 虚拟表

### React 前端性能策略

#### 编辑器 (TipTap/ProseMirror)
- 大文件（> 10 万字）开启 ProseMirror 的增量解析模式，分批渲染节点树
- 代码块语法高亮使用 `shiki` 的异步 `createHighlighter`，不阻塞编辑器初始化
- 使用 ProseMirror 的 `Decoration` 系统而非 React state 驱动高亮，避免重渲染
- 拖拽选区到 AI 面板时使用浏览器原生 `DragEvent`，不经过 React 状态管理

#### 组件渲染
- 文件树使用虚拟滚动（`@tanstack/react-virtual`），10,000 文件时仅渲染可视区域
- 搜索结果显示前 20 条，滚动到底时增量加载
- `React.memo` 包裹不常变的组件（TabBar、StatusBar、工具栏）
- 悬浮面板（大纲、反向链接）使用 `lazy(() => import(...))` + `Suspense` 延迟加载
- shadcn/ui 的 `Sheet` / `Dialog` 使用 `lazy` 加载，不影响首屏渲染

#### 事件处理
- 关键字搜索输入使用 `debounce`(150ms)，避免每次按键触发全文遍历
- 文件保存使用 `debounce`(500ms)，连续输入时不频繁调用 IPC write_file
- AI 流式 token 批量累积 50ms 后一次性更新 DOM，而非每个 token 触发一次 React render
- 文件监听回调在前端侧做 hash 去重（同一文件连续事件合并）

### 构建优化

```toml
# Cargo.toml
[profile.release]
opt-level = "s"       # 体积优化（Tauri 桌面应用对速度不敏感体积敏感）
lto = true            # 链接时优化
codegen-units = 1     # 单代码生成单元，最大化内联
strip = true          # 去除符号表
panic = "abort"       # panic 直接终止，避免 unwinding 代码膨胀
```

```typescript
// vite.config.ts — 前端构建
build: {
  target: 'esnext',
  minify: 'esbuild',
  rollupOptions: {
    output: {
      manualChunks: {
        'prosemirror': ['prosemirror-state', 'prosemirror-view', 'prosemirror-model'],
        'tiptap': ['@tiptap/react', '@tiptap/core'],
        'ui': ['@radix-ui/*', 'lucide-react'],
      }
    }
  },
  chunkSizeWarningLimit: 500,
}
```

### 性能回归防火墙

- CI 流程中加入性能基准测试：启动时间、搜索延迟、打字延迟的自动化测量
- 如果任一指标较上一次 commit 退化 > 20%，CI 标记为失败
- 开发期间使用 `cargo bench` 对关键路径（文件解析、向量检索、Markdown 序列化）进行微基准测试
- 前端使用 React Profiler + Chrome DevTools Performance 面板定期检查

---

## 关键设计决策

### 为什么不是 Electron
Electron 的打包体积（150MB+）、内存占用（300-500MB）、安全模型（JS 直接运行在 Node 上）在 2026 年已经不具竞争力。Tauri 2.x 提供更小的体积（5-10MB）、更低的内存占用（50-100MB）、Rust 后端的权限声明制和天然安全隔离。

详见 [README.md](./README.md#技术栈)。

### 为什么不做 CRDT 实时同步
Iris **永久不做**实时多人协作。CRDT（如 Yjs）会使 TipTap/Prosemirror 状态、Markdown 往返与索引一致性成本倍增；核心场景是单用户本地笔记。外部文件冲突仅通过 v0.3 的 diff/抉择流程处理，而非进程内协同编辑。

### 为什么 SQLite 而不是 LanceDB 或 Qdrant
本地部署场景下，PostgreSQL + pgvector 太重；Qdrant 需要独立进程；LanceDB 偏向列式分析。Iris v0.1 在 SQLite 中存 `chunk_embeddings` BLOB，检索时在 Rust 做全量余弦（笔记体量 < 数万 chunk 可接受）。后续可引入 sqlite-vec 等近似索引，见 [docs/eval/semantic-search.md](docs/eval/semantic-search.md)。

### 为什么不用内容块（Block-based）编辑器
Notion 式的 Block 编辑器牺牲了 Markdown 的纯文本可移植性。Iris 的文件即数据哲学要求所有内容必须可以完整表示为 `.md` 纯文本。ProseMirror 可以在结构化操作和标准 Markdown 之间取得平衡。
