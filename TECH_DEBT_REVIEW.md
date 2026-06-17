# Iris 项目技术债务审查报告

> 审查日期：2026-06-17
> 审查范围：Markdown 语法体系、索引与检索、数据隔离与安全、IPC 契约、性能阻塞
> 结论：原报告方向大体正确，但存在版本事实、严重度和若干安全配置判断不准确。本文件已按当前代码校准，并记录 2026-06-17 对 P0 至 P2 项的修复状态。

---

## 一、真实性评价

原报告可以作为技术债清单的起点，但不能原样作为修复计划执行：

- **成立**：`vault_set` 同步重索引、索引器正则/逐行解析、frontmatter 前后端不对等、部分 IPC 返回裸 JSON、regex 热路径重复编译、`chunk_markdown` 复杂度偏高、文件监听重复读取等判断均有代码依据。
- **需修正**：当前依赖是 `marked@15.0.12`，不是 v14；Markdown syntax kind 当前为 23 种而非 24 种；Tauri CSP 已有严格 `script-src 'self'`；`folder_rename` 已在 `spawn_blocking` 中执行，问题是后台全 vault 重索引，而不是主线程同步阻塞。
- **需降级**：AI 会话明文 SQLite 存储符合 ROADMAP 对“应用运行时状态”的定位，属于隐私/产品策略风险，不是违反 `.md` 权威数据原则；`ApiKeyBundle` 内存未 zeroize 是纵深防御问题，不应与明文落盘同级。
- **需补充**：`vault_set` 已不再删除跨 vault 运行时数据；migration 030 已引入 `vault_id`，但会话等运行时表仍未在写入/查询路径按当前 vault 作用域过滤。后续应补齐按 vault 隔离、保留和清理策略。

---

## 二、Markdown 语法体系

### 2.1 架构优势

- `src/lib/markdown-contract/types.ts` 定义了 23 种 syntax kind、4 级 capability、8 个 profile（其中 5 个为 required profile），并以 tests/markdown-contract 覆盖分类、渲染、摄取和导出路径。
- 原始 HTML 与 HTML 注释通过 `PreserveInlineExtension` / `PreserveBlockExtension` 保存 `originalRaw`，导出时可原样写回。
- footnote reference / definition 已有 TipTap 节点，`footnoteDef` 保存 `originalRaw`；能力分类仍标为 `render_only`，不是 preserve-only。
- callout 通过 `CalloutBlockquoteExtension` 保存 `calloutOriginalRaw`，未编辑时优先写回原文，编辑后再按 callout markdown 重新序列化。
- PM serializer 是编辑器导出热路径，Turndown 仍作为 HTML 降级路径存在，职责基本清楚。

### 2.2 技术债

| 编号 | 严重度 | 问题                                            | 位置                                                                | 校准后的说明                                                                                                                                                                                 |
| ---- | ------ | ----------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1   | 高     | Rust 侧 Markdown 索引器不是 AST 解析            | `src-tauri/src/indexer/*.rs`                                        | Rust 后端用 regex/逐行逻辑提取 wiki-link、image、body tag、frontmatter，与前端 `marked` contract 不是同一语义源。链接、图片、脚注、代码块和 HTML 边界越复杂，前后端越容易分歧。              |
| M2   | 高     | `chunk_markdown` 字符计数存在 O(n²) 风险        | `src-tauri/src/indexer/chunker.rs`                                  | 修复前循环内多次 `current.chars().count()`，长段落分割还用 `char_indices().nth(max_chars)`；本轮已改为维护当前 chunk 字符数并补充分块回归测试。                                              |
| M3   | 中     | Frontmatter 前后端解析能力不对等                | `src/lib/frontmatter.ts`、`src-tauri/src/indexer/frontmatter.rs`    | TS 侧只解析简单 `key: value` / 内联数组；Rust 侧用 `serde_yaml`。多行字符串、嵌套对象、引用、复杂数组会导致 UI 标题/字段和索引层 JSON 表现不一致。                                           |
| M4   | 中     | footnote definition 依赖 `marked` gap reconcile | `src/lib/markdown-contract/fragment-reconcile.ts`                   | `marked` lexer 会把脚注定义识别成 link-reference 类片段，当前用 gap 扫描恢复 `footnote_def`。这是可用 workaround，但升级 `marked` 时必须跑完整 contract suite。当前版本为 `marked@15.0.12`。 |
| M5   | 中     | 表格 cell 内 mark 序列化逻辑重复                | `src/lib/editor-pm-serialize.ts`、`src/lib/callout-pm-serialize.ts` | `cellPlainText` 和 callout 行序列化手写 bold/italic/code/link 等 mark 规则，新增 mark 或修改 serializer 时容易漏同步。应复用统一 inline serializer。                                         |
| M6   | 低     | 能力声明和编辑器扩展分散                        | `src/lib/markdown-contract/*`、`src/components/editor/extensions/*` | syntax capability、TipTap extension、PM serializer、contract tests 分散维护。当前测试覆盖较强，但新增语法族仍需要人工同步多处。                                                              |

---

## 三、数据隔离与安全

### 3.1 架构优势

- API Key 存储通过 OS 凭据管理器，SQLite 只保存配置标记。
- `AppError::serialize` 对 IO/DB/HTTP/Keyring 等错误做泛化输出，避免原始错误文本直接进入前端。
- `.classified/` 笔记使用 AES-256-GCM 文件格式，且 `validate_ai_note_path` 阻止涉密笔记进入 AI 管道。
- CAS 存储层要求加密 key，`VaultKey` 使用 zeroize，并在 lock 时主动清零。
- migration 030 已为 AI 运行时相关表添加 `vault_id` 索引，为后续按 vault 隔离提供基础。
- Tauri CSP 已设置 `default-src 'self'`、`script-src 'self'`、`object-src 'none'`、`frame-ancestors 'none'` 和 Trusted Types 要求；原报告“无 CSP 保护”不成立。

### 3.2 技术债

| 编号 | 严重度 | 问题                                     | 位置                                                                    | 校准后的说明                                                                                                                                                                                                         |
| ---- | ------ | ---------------------------------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S1   | 高     | vault 运行时数据保留与隔离策略未完成     | `src-tauri/src/commands/file.rs`、`src-tauri/src/ai_runtime/session.rs` | `vault_set` 已不再删除 `sessions`、`knowledge_deposits`、`web_page_cache`、`search_cache` 等运行时表，但当前会话写入/列表仍未按 `vault_id` 作用域隔离。它解决了误删问题，但还没有完整解决跨 vault 可见性和保留策略。 |
| S2   | 中     | AI 会话与收件箱内容明文存储在 SQLite     | `src-tauri/migrations/009_ai_runtime.sql`                               | `session_messages.content` 与 `knowledge_deposits.content` 是明文 TEXT。这符合“运行时状态”定位，但若 AI 回复包含敏感笔记摘录，数据库会持久化这些内容。建议提供会话保留期限、清理入口或可选应用层加密。               |
| S3   | 中     | `ApiKeyBundle` 缓存未使用 zeroize 容器   | `src-tauri/src/credentials.rs`                                          | 修复前 `LazyLock<Mutex<Option<ApiKeyBundle>>>` 内部使用普通 `String` 保存凭据值；本轮已在 drop、替换和删除路径主动 zeroize bundle 内容。                                                                             |
| S4   | 中     | 涉密密码经 Tauri IPC 以 `String` 传递    | `src-tauri/src/commands/classified.rs`、`src/lib/ipc.ts`                | IPC 是本机进程间/进程内边界，不是网络明文传输；本轮已在后端命令入口使用 `Zeroizing` 缩短密码驻留生命周期。前端字符串生命周期仍受浏览器运行时限制，不能宣称端到端内存清零。                                           |
| S5   | 低     | CSP 放行域名需随 LLM provider 注册表同步 | `src-tauri/tauri.conf.json`、`src-tauri/src/llm/*`                      | CSP 本身严格，但 `connect-src` 是硬编码域名列表。新增默认 provider 或 search backend 时必须同步 CSP，并保持 `tests/runtime-contracts.test.ts` 覆盖。                                                                 |

---

## 四、前后端耦合与 IPC 契约

### 4.1 架构优势

- `tests/ipc-boundary.test.ts` 保证直接 `invoke()` 只出现在 `src/lib/ipc.ts`。
- 大多数常规命令通过 TS wrapper 暴露给业务组件，业务层不直接拼 Tauri command 名。
- LLM token、research progress、工具确认等长流程使用事件通道，避免单个 IPC 调用长时间持有响应。

### 4.2 技术债

| 编号 | 严重度 | 问题                                          | 位置                                                            | 校准后的说明                                                                                                                                                                                                             |
| ---- | ------ | --------------------------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| C1   | 高     | 多个用户可见 command 返回 `serde_json::Value` | `src-tauri/src/commands/ai_commands.rs`、`research_commands.rs` | 修复前 `ai_send_message`、`tool_confirm`、`ai_list_tools`、`knowledge_reindex`、`research_execute`、`research_status` 等返回裸 JSON；本轮已将这些稳定响应改为命名 DTO。动态工具 schema 内部继续使用 `Value` 是合理边界。 |
| C2   | 高     | Rust/TS 字段命名混用 camelCase 与 snake_case  | `src-tauri/src/ai_types/mod.rs`、`src/types/*.ts`               | `FileListItem.updatedAt`、`FileLinkSummary.inboundCount` 等是 camelCase；大量 AI DTO、session、trace 仍是 snake_case。不是所有混用都错误，但缺少统一边界规则和自动检测。                                                 |
| C3   | 中     | IPC 类型定义手工维护                          | `src/types/ipc.ts`、`src/types/ai.ts`                           | Rust DTO 与 TS interface 没有自动生成或 schema diff。现有契约测试多为源代码字符串检查和重点路径测试，不能覆盖所有 command shape。                                                                                        |
| C4   | 中     | scene enum 反序列化使用字符串拼接             | `src-tauri/src/commands/ai_commands.rs`、`assistant_facade.rs`  | 修复前多处 `serde_json::from_str(&format!("\"{scene}\""))`；本轮已新增 `AiScene::parse_wire(&str)` 并复用到相关入口。                                                                                                    |
| C5   | 低     | 长任务抽象不统一                              | `commands/*.rs`                                                 | 有的 command 同步执行，有的 `spawn_blocking`，有的返回 request_id 并发事件。应定义统一的“后台任务 + 进度 + 取消 + 完成事件”模式。                                                                                        |

---

## 五、性能阻塞

### 5.1 后端性能债

| 编号 | 严重度 | 问题                                                 | 位置                                                                  | 影响与校准                                                                                                                                                                  |
| ---- | ------ | ---------------------------------------------------- | --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P1   | 严重   | `vault_set` 同步全量索引                             | `src-tauri/src/commands/file.rs`                                      | 修复前 `vault_set` 同步遍历所有 `.md` 并逐个索引；本轮已改为后台任务并发进度事件，但尚未实现取消入口，因此只能算部分修复。                                                  |
| P2   | 高     | `folder_rename` 后台全 vault 重索引                  | `src-tauri/src/commands/file.rs`                                      | 修复前重命名一个目录后仍扫描并重索引整个 vault；本轮已限定为移动路径和实际被级联改写的来源文件。                                                                            |
| P3   | 高     | watcher 外部变更路径重复读取                         | `src-tauri/src/watcher/mod.rs`、`src-tauri/src/indexer/scan.rs`       | 修复前 watcher 先读文件计算 hash，再读文件索引；本轮已复用 `index_file_from_content`，避免写入/监听路径重复读盘。                                                           |
| P4   | 高     | `patch_apply` / `organize_apply` 同步文件 I/O 与索引 | `src-tauri/src/commands/writing_commands.rs`、`organize_commands.rs`  | 本轮已让 writing / organize 写入路径复用内存内容索引，降低重复 I/O；批量 command 仍是串行执行，后续若要完全后台化需另建统一任务模型。                                       |
| P5   | 中     | `chunk_markdown` 长文档分块效率偏低                  | `src-tauri/src/indexer/chunker.rs`                                    | 同 M2，本轮已修复主要 O(n²) 风险；后续可补大文件基准测试量化收益。                                                                                                          |
| P6   | 中     | regex 热路径重复编译                                 | `src-tauri/src/indexer/wikilink.rs`、`image_ref.rs`、`frontmatter.rs` | 修复前每次提取 wiki-link、image、body tag 都 `Regex::new()`；本轮已改为 `LazyLock<Regex>`。                                                                                 |
| P7   | 中     | tag 与 wiki-link N+1 查询                            | `src-tauri/src/indexer/scan.rs`、`wikilink.rs`                        | 修复前 tag 和 wiki-link 按项查询；本轮已对 tag 去重并批量查询 id，wiki-link 预加载目标文件映射并去重解析标题。                                                              |
| P8   | 中     | embedding 模型全局 Mutex 串行化                      | `src-tauri/src/embedding/engine.rs`                                   | `fastembed` 模型被 `OnceLock<Mutex<TextEmbedding>>` 保护，注释说明 `embed()` 会修改内部状态。当前做法安全但吞吐有限；优化需先确认 fastembed 并发模型，不能简单换 `RwLock`。 |
| P9   | 中     | semantic cosine fallback 有上限但仍是 Rust 全表扫描  | `src-tauri/src/embedding/engine.rs`                                   | fallback 会在 `chunk_embeddings` 超过 8000 时跳过，避免无限制加载；在 8000 以内仍会加载所有 embedding 并排序。需要 sqlite-vec 可用性、分片或预过滤策略。                    |
| P10  | 中     | CJK bigram 构造复制整篇文档字符                      | `src-tauri/src/indexer/fts.rs`                                        | `text.chars().collect::<Vec<_>>()` 会复制全文字符。CJK 长文档会增加内存和 CPU；可改为 streaming bigram 生成。                                                               |
| P11  | 低     | `prune_stale_file_indexes` 加载所有文件路径          | `src-tauri/src/indexer/scan.rs`                                       | 每次 prune 从 DB 取出所有 path 后逐个访问磁盘。大 vault 下应降低调用频率或使用批处理/后台任务。                                                                             |
| P12  | 低     | `links` 表缺少 source/target 单列索引                | `src-tauri/migrations/001_core.sql`                                   | 修复前只有 `UNIQUE(source_id, target_id)`；本轮已新增 migration 031，为 `source_id` 与 `target_id` 补单列索引及回滚脚本。                                                   |

### 5.2 前端性能观察

- 已使用 `@tanstack/react-virtual` 的位置包括 `AiMessageList`、`EditorOutline`、`QuickOpen`、`VaultNavigator`。
- `AiMessageBubble` 通过 `useStreamingContent` 节流流式 markdown 渲染。
- `VersionTimeline` 已用 `useMemo(() => groupVersions(versions), [versions])` 包住分组计算；原报告“多次 filter 未 memoize”应降为低优先级观察，不构成当前性能债。
- `src/App.impl.tsx` 仍有多个 `useEffect` 与较高协调复杂度，但入口 facade 已拆薄；后续应继续下沉 tab、外部同步、overlay 调度和关闭保存逻辑。

---

## 六、剩余优先级建议

### P0（仍需补齐）

1. **S1: 运行时数据按 vault 隔离**：为 sessions / messages / deposits / cache 等运行时路径补齐 `vault_id` 写入、查询过滤、迁移回填和跨 vault 可见性测试。
2. **P1: `vault_set` 后台任务取消**：现有后台索引已有进度事件，但缺少取消入口、任务状态查询和并发切换 vault 时的请求代际保护。

### P1（近期收敛）

3. **C2/C3: IPC 边界规则自动化**：明确 Rust/TS 字段命名边界，并以 schema 生成或更强契约测试降低手工同步风险。
4. **M5: 统一 inline mark serializer**：消除表格 cell 和 callout 行内 mark 序列化重复逻辑。
5. **P3/P4/M2/P5 回归基准**：本轮已修主要实现问题，后续应补大文件、批量写入和 watcher 场景的性能基准。

### P2（策略评估）

6. **M1: 评估 Rust Markdown AST 解析器**：本轮未做 parser 替换；如推进需先验证许可证、GFM/Obsidian 语法覆盖和 AGPL 兼容。
7. **M3: frontmatter 解析策略统一**：当前是“前端支持子集且保留复杂 YAML 原文”；是否引入完整 YAML parser 仍需产品和依赖评估。
8. **P8/P9: embedding/semantic search 路径实测**：建立 1k/10k vault 基准，再决定 sqlite-vec、预过滤、模型实例池或队列策略。
9. **S2: 敏感运行时数据治理**：在现有清理入口基础上补保留期限、按 vault 清理、可选应用层加密等产品策略。

---

## 七、2026-06-17 修复状态

| 优先级 | 条目         | 状态     | 修复说明                                                                                                                                                                                                |
| ------ | ------------ | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0     | P1           | 部分修复 | `vault_set` 不再同步全量索引，改为后台索引任务并通过 `vault:index_progress` 发 started/progress/completed/failed 事件；尚未实现取消入口。                                                               |
| P0     | S1           | 部分修复 | `vault_set` 不再自动删除运行时表，避免误删；但 sessions / deposits / cache 等运行时数据尚未全面按 `vault_id` 写入和过滤，不能宣称完整 vault 隔离。                                                      |
| P0     | P2           | 已修复   | `folder_rename` 仅重索引移动路径和实际被级联改写的来源文件，不再全 vault 重索引。                                                                                                                       |
| P1     | P3 / P4      | 已修复   | watcher、writing、organize 写入路径复用 `index_file_from_content`，减少重复读盘和重复哈希。                                                                                                             |
| P1     | P6           | 已修复   | wiki-link、image reference、body tag regex 改为 `LazyLock<Regex>`。                                                                                                                                     |
| P1     | M2 / P5      | 已修复   | `chunk_markdown` 改为维护字符计数并避免长段落切分产生空 chunk。                                                                                                                                         |
| P1     | C1 / C4      | 已修复   | 新增 `AiScene::parse_wire`，去除 scene JSON 字符串拼接；`ai_send_message`、`tool_confirm`、`harness_resume`、`ai_list_tools`、`knowledge_reindex`、`research_execute`、`research_status` 改为命名 DTO。 |
| P1     | P12          | 已修复   | 新增 migration 031，为 `links.source_id` / `links.target_id` 添加单列索引及 down migration。                                                                                                            |
| P2     | M1           | 部分收敛 | 未引入新 Rust Markdown parser；本轮没有完成 AST 解析器评估或替换，只保留现有 contract suite 作为防线。后续如引入 AST parser 需另做许可证和语法覆盖评估。                                                |
| P2     | M3           | 已收敛   | 明确前端 frontmatter 是受支持子集解析器，复杂 YAML 在保存时保留原始行；新增测试覆盖复杂 YAML 不被重写。                                                                                                 |
| P2     | P7           | 已修复   | tag 同步去重后批量查询 id；wiki-link 索引预加载目标文件并去重解析标题。                                                                                                                                 |
| P2     | P8 / P9      | 已收敛   | 保持 embedding 全局 Mutex 的安全策略；新增契约测试固定 cosine fallback 的 8000 chunk 上限和跳过分支。                                                                                                   |
| P2     | S2 / S3 / S4 | 已收敛   | 保留 AI 运行时状态定位；已有 `ai_cache_clear` 清理入口，本轮补充 `ApiKeyBundle` drop/替换/删除 zeroize，以及 classified IPC 密码后端 `Zeroizing` 生命周期控制。                                         |

---

## 八、原报告修正清单

- 将 “24 种语法族” 修正为当前代码中的 23 种 syntax kind。
- 将 `marked v14` 修正为 `marked@15.0.12`。
- 删除 “localStorage 使用安全但无 CSP 保护” 条目；当前 Tauri CSP 已严格设置。
- 将 `folder_rename` 从“同步阻塞 UI”修正为“后台执行但全 vault 重索引过度”。
- 将 “AI 会话明文存储”从安全红线问题降级为隐私/保留策略风险。
- 将 “前端传 Argon2 哈希后的值”修正为不推荐的简化方案；更合适的是缩短明文生命周期和 zeroize。
- 补充修复前 `vault_set` 清空运行时表与 migration 030 `vault_id` 方向冲突；本轮已停止自动清表，但尚未补齐运行时表 `vault_id` 写入/过滤。
- 补充 `index_file_from_content` 已解决 `file_write` 重复读取；本轮已继续扩展到 watcher / writing / organize 写入路径。
- 补充 cosine fallback 已有 8000 chunk 上限，风险不是无限制全表加载，而是上限内仍然全扫描。

---

## 九、本轮核验的主要文件

### 配置与事实源

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `ROADMAP.md`
- `docs/audits/2026-06-11-project-review-v1.1.0.md`

### Rust 后端

- `src-tauri/src/commands/file.rs`
- `src-tauri/src/commands/writing_commands.rs`
- `src-tauri/src/commands/organize_commands.rs`
- `src-tauri/src/commands/classified.rs`
- `src-tauri/src/commands/ai_commands.rs`
- `src-tauri/src/commands/research_commands.rs`
- `src-tauri/src/indexer/scan.rs`
- `src-tauri/src/indexer/wikilink.rs`
- `src-tauri/src/indexer/image_ref.rs`
- `src-tauri/src/indexer/frontmatter.rs`
- `src-tauri/src/indexer/chunker.rs`
- `src-tauri/src/indexer/fts.rs`
- `src-tauri/src/watcher/mod.rs`
- `src-tauri/src/embedding/engine.rs`
- `src-tauri/src/credentials.rs`
- `src-tauri/migrations/001_core.sql`
- `src-tauri/migrations/009_ai_runtime.sql`
- `src-tauri/migrations/030_runtime_vault_scope.sql`

### TypeScript 前端

- `src/lib/ipc.ts`
- `src/types/ipc.ts`
- `src/lib/frontmatter.ts`
- `src/lib/markdown-contract/types.ts`
- `src/lib/markdown-contract/fragment-reconcile.ts`
- `src/lib/editor-pm-serialize.ts`
- `src/lib/callout-pm-serialize.ts`
- `src/components/editor/extensions/FootnoteExtension.ts`
- `src/components/editor/extensions/PreserveInlineExtension.ts`
- `src/components/editor/extensions/PreserveBlockExtension.ts`
- `src/components/version/VersionTimeline.tsx`
- `src/components/version/version-timeline-groups.ts`
- `tests/ipc-boundary.test.ts`
- `tests/runtime-contracts.test.ts`
