# Iris 项目技术债务审查报告

> 审查日期：2026-06-17
> 审查范围：Markdown 语法体系、数据隔离与安全、前后端耦合、性能阻塞

---

## 一、Markdown 语法体系

### 1.1 架构优势（值得肯定）

- **契约驱动的分层设计**：`markdown-contract/types.ts` 定义了 24 种语法族 × 4 级能力 × 8 种 profile 的完整矩阵，这是业界少见的严谨做法
- **preserve-only 安全网**：原始 HTML、脚注定义等无法安全编辑的内容以 atom 节点存储 `originalRaw`，导出时原样写回，零数据丢失
- **callout 原文优化**：`CalloutBlockquoteExtension` 的 `appendTransaction` 插件在用户未编辑时写回原文，编辑后才重新序列化
- **双序列化路径**：PM serializer 为热路径，Turndown 为降级兜底，优先级明确

### 1.2 技术债

| 编号 | 严重度 | 问题 | 位置 | 说明 |
|------|--------|------|------|------|
| M1 | **高** | Rust 侧无 Markdown AST 解析器 | `src-tauri/src/indexer/*.rs` | 后端使用 regex 逐行提取 wiki-link、image、frontmatter，与前端 `marked` lexer 的 token 语义不一致。例如 `extract_wiki_links` 不理解嵌套括号 `[[]]` 内部的链接，而 `marked` lexer 能正确处理。随着语法复杂度增长，两侧分类结果可能分歧。 |
| M2 | **高** | `chunk_markdown` O(n²) 时间复杂度 | `src-tauri/src/indexer/chunker.rs:9,19` | 循环内每次调用 `current.chars().count()` 是 O(n)，`char_indices().nth(max_chars)` 也是 O(n)。对 100KB 文档，分块操作可能达到 O(n²)。应增量维护字符计数。 |
| M3 | **中** | Frontmatter 解析器不对等 | `src/lib/frontmatter.ts` vs `src-tauri/src/indexer/frontmatter.rs` | TS 侧用自写的 `parseYamlFields()` 只解析 key:value，Rust 侧用 `serde_yaml` 做完整 YAML 解析。含复杂 YAML（多行值、锚点引用）的 frontmatter 在两侧行为不一致。 |
| M4 | **中** | `marked` v14 的 link-reference 与 footnote 冲突 | `src/lib/markdown-contract/fragment-reconcile.ts` | `marked` 的 lexer 会将脚注定义 `[^label]: text` 消费为 link-reference token，需要 `fragment-reconcile` 做偏移恢复。这是 `marked` 的已知问题，升级版本可能破坏此 workaround。 |
| M5 | **中** | 序列化输出与标准 Markdown 有隐式差异 | `src/lib/editor-pm-serialize.ts` | PM serializer 对 `bold` 使用 `**` 而非 `__`（符合 CommonMark），但 TipTap 内部使用 `bold`/`italic` 而非 `strong`/`em`，需自定义 mark serializer 映射。`cellPlainText` 手动拼接 mark 语法而非复用 serializer 的 mark 渲染逻辑，新增 mark 时容易遗漏。 |
| M6 | **低** | GFM schema 声明分散 | `src/components/editor/gfm-schema.ts` + 各 Extension 文件 | 能力等级在 `gfm-schema.ts` 和各 Extension 的 `addNodeView`/`addMark` 中双重声明，维护时需同步两处。 |

---

## 二、数据隔离与数据安全

### 2.1 架构优势（值得肯定）

- **API Key 存储**：全部通过 OS 原生凭据管理器（`keyring` crate），SQLite 中仅存布尔标记
- **错误日志脱敏**：`AppError::serialize` 对 IO/DB/HTTP/Keyring 错误只返回泛化消息；`log_error` 使用 SHA-256 哈希替代原始内容
- **分类笔记隔离**：`.classified/` 下的笔记 AES-256-GCM 加密存储，且被明确阻止进入 AI 管道（`validate_ai_note_path`）
- **CAS 强制加密**：`store.rs` 在无加密 key 时拒绝写入明文对象
- **内存清零**：`VaultKey` 使用 `ZeroizeOnDrop`，`lock()` 时主动调用 `zeroize()`
- **vault_id 隔离**：migration 030 为所有 AI 相关表添加 `vault_id` 列，防止跨 vault 数据泄漏

### 2.2 技术债

| 编号 | 严重度 | 问题 | 位置 | 说明 |
|------|--------|------|------|------|
| S1 | **高** | `ApiKeyBundle` 缓存中 `String` 值未 zeroize | `src-tauri/src/credentials.rs:17-18` | `LazyLock<Mutex<Option<ApiKeyBundle>>>` 中的 `BTreeMap<String, String>` 的值是标准库 `String`，不实现 `Zeroize`。bundle 替换时旧值由 `Drop` 释放但不保证内存清零，存在内存取证风险。 |
| S2 | **高** | `vault_set` 切换 vault 时执行全量 re-index 阻塞 UI | `src-tauri/src/commands/file.rs:879-933` | 同步命令，遍历所有文件逐个 `index_file_with_embed`。用户切换 vault 时 UI 完全冻结。同时 `vault_runtime_cleanup_sql` 清除旧 vault 的 session 数据——如果用户误操作切换 vault，AI 历史被清除且不可恢复。 |
| S3 | **中** | AI 会话消息以明文 JSON 存储在 SQLite | `migrations/009_ai_runtime.sql:20-31` | `session_messages.content` 是明文 TEXT。如果用户笔记中有敏感内容被 AI 引用，这些内容会持久化在 SQLite 中。虽然 `.iris/` 目录本身不加密（只有 `.classified/` 和 CAS 加密），但数据库文件未加密。 |
| S4 | **中** | `classified_unlock` 密码经 IPC 明文传递 | `src-tauri/src/commands/classified.rs:449-453` | 密码作为 `String` 参数通过 Tauri IPC 传递。虽然 IPC 是进程内通信（非网络），但密码可能残留在进程内存中。建议前端传 Argon2 哈希后的值。 |
| S5 | **低** | localStorage 使用安全但无 CSP 保护 | 前端各组件 | localStorage 存储的都是 UI 状态（theme、zoom），无安全问题。但 Tauri 的 CSP 配置（`tauri.conf.json`）未设置严格的 `script-src`，需确认生产构建的 CSP 策略。 |

---

## 三、前后端耦合

### 3.1 架构优势（值得肯定）

- **IPC 边界强制收口**：`ipc-boundary.test.ts` 扫描整个 `src/` 确保 `invoke()` 仅在 `src/lib/ipc.ts` 中调用，这是极强的架构不变量
- **1:1 命令映射**：~100 个 Rust command 各有对应的 TS wrapper，无动态 invoke
- **事件驱动的流式通信**：LLM token 流通过 `emit("llm:token")` + `listen()` 实现，command 本身立即返回 `request_id`

### 3.2 技术债

| 编号 | 严重度 | 问题 | 位置 | 说明 |
|------|--------|------|------|------|
| C1 | **高** | 部分 command 返回 `serde_json::Value` 而非类型化结构体 | `ai_commands.rs`, `research_commands.rs` 等 | `ai_send_message`、`research_execute`、`knowledge_reindex` 等返回 `serde_json::json!({...})`，TypeScript 侧的类型定义无法在编译期与 Rust 结构体对齐。一旦 Rust 侧修改了 JSON shape，TS 侧只会在运行时失败。 |
| C2 | **高** | serde 命名约定不一致 | 多处 | IPC DTO 混用 `camelCase`（如 `FileListItem.updatedAt`）和 `snake_case`（如 `ContextPacket.source_type`）。新增 Rust struct 时如果遗漏 `#[serde(rename_all = "camelCase")]`，前端会收到意外的字段名。 |
| C3 | **中** | IPC 类型定义手动维护，无自动生成 | `src/types/ipc.ts` + `src/types/ai.ts` | TS 类型是手写的，Rust 结构体修改后需人工同步。项目有 `classified-ipc.test.ts` 测试 invoke 参数形状，但未覆盖所有 command。 |
| C4 | **中** | 场景枚举反序列化使用字符串拼接 | `ai_commands.rs` 多处 | `serde_json::from_str(&format!("\"{scene}\""))` 将前端字符串手动反序列化为 `AiScene` 枚举。如果前端传入非法值，错误信息不够友好。 |
| C5 | **低** | 混合 async 模式无统一抽象 | `commands/*.rs` | 部分 command 用 `spawn_blocking`，部分同步执行，部分 fire-and-forget + event emit。缺少统一的"长时间操作 + 进度"抽象。 |

---

## 四、性能阻塞

### 4.1 关键阻塞点

| 编号 | 严重度 | 问题 | 位置 | 影响 |
|------|--------|------|------|------|
| P1 | **严重** | `vault_set` 同步全量索引 | `file.rs:879-933` | 切换 vault 时 UI 完全冻结。1000 个笔记可能阻塞数十秒。必须改为 `async` + `spawn_blocking` + 进度事件。 |
| P2 | **高** | `organize_apply` / `patch_apply` 同步文件 I/O | `organize_commands.rs:284`, `writing_commands.rs:36` | 批量操作时逐个读写文件 + 重建索引，全部在 command 线程执行。 |
| P3 | **高** | `folder_rename` 全量 re-index | `file.rs:435-444` | 重命名文件夹后对**整个 vault** 重新索引，而非仅受影响的文件。 |
| P4 | **高** | 文件双重读取 | `scan.rs:88` + `watcher/mod.rs:115,123` | watcher 收到事件后 `file_hash` 读一次文件，然后 `index_file_with_embed` 再读一次。内容已在内存中却重复 I/O。 |
| P5 | **高** | 嵌入模型全局 Mutex 串行化 | `embedding/engine.rs:17` | `OnceLock<Mutex<TextEmbedding>>` 导致所有嵌入请求串行执行。embedding worker 处理批次时，其他请求排队等待。 |
| P6 | **中** | Regex 热路径重复编译 | `indexer/wikilink.rs:8`, `indexer/image_ref.rs:7` 等 | `extract_wiki_links` 每次调用都 `Regex::new()`。对 1000 个文件的 vault，编译 1000 次同一个正则。应使用 `LazyLock<Regex>`。 |
| P7 | **中** | N+1 查询模式 | `scan.rs:51-68` (`sync_file_tags`) | 每个 tag 执行 3 条 SQL（INSERT + SELECT + INSERT）。10 个 tag = 30 条查询。应批量处理。 |
| P8 | **中** | N+1 wiki-link 解析 | `wikilink.rs:60-86` | 每个 wiki-link 执行一条 SELECT 查询。20 个链接 = 20 条查询。应预加载 title→id 映射。 |
| P9 | **中** | 语义搜索 cosine fallback 全表扫描 | `embedding/engine.rs:142-200` | sqlite-vec 不可用时，加载所有 chunk embedding 到 Rust 内存做余弦相似度。8000 chunks × 384 维 ≈ 12MB，且在 Mutex 保护下串行。 |
| P10 | **中** | CJK bigram 全字符克隆 | `indexer/fts.rs:22` | `text.chars().collect()` 将整个文档字符复制到 Vec。100KB 文档在 CJK 场景下内存翻倍。 |
| P11 | **低** | `prune_stale_file_indexes` 全路径加载 | `scan.rs:504-528` | 从 DB 加载所有文件路径到内存，逐个检查磁盘存在性。 |
| P12 | **低** | `links` 表缺少单列索引 | `migrations/001_core.sql` | `links(source_id, target_id)` 有 UNIQUE 约束，但 `file_backlinks` 查询只用 `target_id`，`sync_wiki_links` 只用 `source_id`。需补充单列索引。 |

### 4.2 前端性能（总体良好）

- ✅ 使用 `@tanstack/react-virtual` 虚拟化长列表（AiMessageList、QuickOpen、VaultNavigator）
- ✅ 流式内容节流：`useStreamingContent.ts` 限制 markdown 重渲染到 ~8fps
- ✅ `AiMessageList` 使用 `memo()` 包裹
- ⚠️ `App.impl.tsx` 有 7 个 `useEffect`，组件复杂度偏高
- ⚠️ `VersionTimeline` 多次 `.filter()` 创建新数组未 memoize

---

## 五、优先级排序建议

### P0（应立即修复）

1. **P1: `vault_set` 阻塞 UI** — 改为 `async` + `spawn_blocking` + 进度事件
2. **S2: vault 切换清除 AI 历史无确认** — 添加二次确认对话框
3. **P3: `folder_rename` 全量 re-index** — 限制为仅受影响文件

### P1（近期修复）

4. **P4: 文件双重读取** — 传递已读取内容避免重复 I/O
5. **P6: Regex 热路径编译** — 改用 `LazyLock<Regex>`
6. **C1: `serde_json::Value` 返回类型** — 替换为类型化结构体
7. **C2: serde 命名不一致** — 统一 `camelCase` 并添加 lint 检查
8. **M2: chunker O(n²)** — 增量维护字符计数

### P2（技术债还清）

9. **P5: 嵌入模型 Mutex** — 考虑 `RwLock` 或每线程模型
10. **P7/P8: N+1 查询** — 批量 SQL 优化
11. **S3: AI 会话明文存储** — 评估是否需要应用层加密
12. **M1: Rust 侧无 Markdown AST** — 评估引入 `pulldown-cmark` 的 ROI
13. **S1: String 未 zeroize** — 使用 `zeroize::Zeroizing<String>` 包装

---

## 附录：审查覆盖的文件

### Rust 后端

- `src-tauri/src/commands/file.rs` — 文件操作、vault_set
- `src-tauri/src/commands/organize_commands.rs` — 批量整理
- `src-tauri/src/commands/writing_commands.rs` — 写作工作流
- `src-tauri/src/commands/classified.rs` — 涉密笔记
- `src-tauri/src/commands/ai_commands.rs` — AI 交互
- `src-tauri/src/indexer/scan.rs` — 文件索引
- `src-tauri/src/indexer/wikilink.rs` — wiki-link 提取
- `src-tauri/src/indexer/chunker.rs` — Markdown 分块
- `src-tauri/src/indexer/frontmatter.rs` — Frontmatter 解析
- `src-tauri/src/embedding/engine.rs` — 嵌入引擎
- `src-tauri/src/error.rs` — 错误处理与脱敏
- `src-tauri/src/credentials.rs` — 凭据管理
- `src-tauri/src/cas/encryption.rs` — CAS 加密
- `src-tauri/src/crypto/vault_key.rs` — Vault 密钥
- `src-tauri/src/storage/db.rs` — 数据库连接池
- `src-tauri/src/security/` — 安全策略
- `src-tauri/migrations/001-030` — 全部迁移脚本

### TypeScript 前端

- `src/lib/ipc.ts` — IPC 封装层
- `src/lib/editor-pm-serialize.ts` — PM 序列化热路径
- `src/lib/editor-ingest.ts` — Markdown 摄取
- `src/lib/markdown.ts` — Markdown 工具函数
- `src/lib/markdown-contract/` — 契约系统
- `src/lib/frontmatter.ts` — Frontmatter 处理
- `src/components/editor/TipTapEditor.tsx` — 编辑器主组件
- `src/components/editor/extensions/` — 17 个 TipTap 扩展
- `src/types/ipc.ts` — IPC 类型定义
- `src/types/ai.ts` — AI 类型定义
- `src/hooks/useAssistantLlmStream.ts` — 流式渲染
- `src/hooks/useStreamingContent.ts` — 内容节流
- `tests/ipc-boundary.test.ts` — IPC 边界测试
