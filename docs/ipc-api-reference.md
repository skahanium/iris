# Iris IPC API Reference

本文档定义 Iris 前端与 Tauri 后端之间的 IPC 命令接口。

**命令总数**：110 个  
**来源文件**：`src-tauri/src/lib.rs` (invoke_handler 注册，第 86-204 行)  
**命令模块目录**：`src-tauri/src/commands/`

---

## 一、设置 (Settings) — `commands::settings`

### settings_get

```rust
pub fn settings_get(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> AppResult<Option<Value>>
```

- **描述**：根据 key 从 SQLite `settings` 表读取一条设置值（JSON）。不存在时返回 `None`。
- **IPC 安全**：经过 `validate_settings_key` 策略校验。

### settings_set

```rust
pub fn settings_set(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: Value,
) -> AppResult<()>
```

- **描述**：写入/更新一条设置项（upsert 模式）。key 冲突时覆盖旧值。`llm_routing` 不允许通过通用 settings 写入，必须使用 `llm_config_set`。

### settings_reset

```rust
pub fn settings_reset(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> AppResult<()>
```

- **描述**：删除指定 key 的设置项，恢复为默认值。

### credential_set

```rust
pub fn credential_set(
    service: String,
    value: String,
) -> AppResult<()>
```

- **描述**：通过操作系统凭据管理器（Windows Credential Manager / macOS Keychain）存储敏感凭证。禁止明文存储。

### credential_has

```rust
pub fn credential_has(
    service: String,
) -> AppResult<bool>
```

- **描述**：检查指定 service 的凭证是否存在。

### credential_delete

```rust
pub fn credential_delete(
    service: String,
) -> AppResult<()>
```

- **描述**：删除指定 service 的凭证。

---

## 二、文件管理 (File) — `commands::file`

### file_list

```rust
pub fn file_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<FileListItem>>
```

- **描述**：列出所有受追踪的用户笔记（每篇文档一条，不含版本快照和 `.iris/` 内部路径）。返回 `FileListItem { path, title, updated_at }`。

### file_read

```rust
pub async fn file_read(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<String>
```

- **描述**：读取指定路径的笔记内容（UTF-8 文本）。仅允许读取用户笔记路径，拒绝内部元数据路径。

### file_write

```rust
pub async fn file_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<FileEntry>
```

- **描述**：写入笔记内容。先写临时文件再原子重命名，写入后自动更新索引和嵌入向量。返回 `FileEntry { id, path, title, updated_at, word_count }`。

### vault_asset_write

```rust
pub async fn vault_asset_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    data_base64: String,
) -> AppResult<String>
```

- **描述**：写入二进制资源文件（如编辑器拖入/粘贴的图片）。限制 20MB。路径必须在 `assets/` 下。

### file_delete

```rust
pub fn file_delete(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<()>
```

- **描述**：将笔记及其所有版本快照移入回收站（15 天保留期）。

### file_discard

```rust
pub fn file_discard(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<()>
```

- **描述**：永久删除空白笔记（不经过回收站）。

### file_rename

```rust
pub async fn file_rename(
    state: State<'_, Arc<AppState>>,
    path: String,
    new_path: String,
) -> AppResult<FileEntry>
```

- **描述**：重命名/移动笔记。级联更新所有引用该笔记的 wikilink（`[[旧名]]` -> `[[新名]]`）和 AI session 关联。

### file_create

```rust
pub async fn file_create(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: Option<String>,
) -> AppResult<FileEntry>
```

- **描述**：创建新笔记文件。content 为空时使用空白内容。文件已存在则报错。

### vault_set

```rust
pub fn vault_set(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<()>
```

- **描述**：设置笔记库（vault）目录。清除内存中 AI 状态以防跨库数据泄露，重启文件监听，触发增量索引。

### vault_get

```rust
pub fn vault_get(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Option<String>>
```

- **描述**：获取当前笔记库目录的绝对路径。未设置时返回 `None`。

### index_rescan

```rust
pub async fn index_rescan(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<FileEntry>>
```

- **描述**：全量重扫描笔记库：先清理已删除文件的索引，再执行增量索引。返回索引后的文件列表。

### file_backlinks

```rust
pub fn file_backlinks(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<Vec<BacklinkEntry>>
```

- **描述**：查询引用了指定笔记的所有反向链接。返回 `BacklinkEntry { source_path, source_title, context }`。

### folder_list

```rust
pub fn folder_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<String>>
```

- **描述**：列出笔记库下所有子目录（含空目录），使用前向斜杠路径。

### folder_create

```rust
pub fn folder_create(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<()>
```

- **描述**：在笔记库下创建文件夹。已存在则报错。

### folder_rename

```rust
pub async fn folder_rename(
    state: State<'_, Arc<AppState>>,
    old_path: String,
    new_path: String,
) -> AppResult<()>
```

- **描述**：重命名/移动文件夹。级联更新所有受影响文件的 wikilink 和 session 引用，然后全量重索引。

### folder_delete

```rust
pub fn folder_delete(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<()>
```

- **描述**：删除空文件夹。非空文件夹会报错。

### path_sync_suggest

```rust
pub fn path_sync_suggest(
    state: State<'_, Arc<AppState>>,
    current_path: String,
    title: String,
) -> AppResult<PathSyncSuggest>
```

- **描述**：根据显示标题建议人类可读的文件路径（处理冲突，如 `笔记（1）.md`）。返回 `PathSyncSuggest { current_path, suggested_path, needs_sync, conflict_resolved }`。

---

## 三、回收站 (Recycle) — `commands::recycle`

### recycle_list_cmd

```rust
pub fn recycle_list_cmd(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<RecycleBinItem>>
```

- **描述**：列出回收站中所有条目。返回 `RecycleBinItem { id, original_path, title, deleted_at, expires_at, version_count }`。

### recycle_restore_cmd

```rust
pub fn recycle_restore_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> AppResult<String>
```

- **描述**：从回收站恢复指定笔记。返回恢复后的路径。

### recycle_purge_cmd

```rust
pub fn recycle_purge_cmd(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> AppResult<()>
```

- **描述**：永久清除回收站中的指定条目。

---

## 四、搜索 (Search) — `commands::search`

### search_keyword

```rust
pub fn search_keyword(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<KeywordHit>>
```

- **描述**：基于 FTS5 全文关键词搜索。返回 `KeywordHit { path, title, snippet }`，snippet 带高亮标记。默认 limit=20。

### search_semantic

```rust
pub fn search_semantic(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<SemanticHit>>
```

- **描述**：基于本地 embedding 的语义搜索；默认使用 `chunk_embeddings` BLOB + Rust cosine fallback，sqlite-vec vec0 为 optional/experimental 加速路径。返回 `SemanticHit { chunk_id, path, title, snippet, score }`。默认 limit=5。

### search_reindex

```rust
pub fn search_reindex(
    state: State<'_, Arc<AppState>>,
) -> AppResult<usize>
```

- **描述**：强制重建搜索索引（增量扫描 + FTS/向量更新）。返回索引的文件数。

---

## 五、LLM 引擎 (LLM) — `commands::llm`

### llm_providers

```rust
pub fn llm_providers() -> Vec<LlmProviderInfo>
```

- **描述**：列出所有可用的 LLM 提供商信息。无需 app state。

### llm_generate

```rust
pub async fn llm_generate(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    params: LlmGenerateParams,
) -> AppResult<String>
```

- **描述**：调用 LLM 生成文本（流式）。`LlmGenerateParams { provider, model, messages, system, stream, custom_base_url }`。自动解析 provider 配置。

### llm_chat

```rust
pub async fn llm_chat(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    params: LlmGenerateParams,
) -> AppResult<String>
```

- **描述**：聊天模式调用 LLM（当前实现等同 `llm_generate`）。

### llm_abort_cmd

```rust
pub fn llm_abort_cmd(
    request_id: String,
) -> AppResult<()>
```

- **描述**：中止指定 request_id 的 LLM 请求。

---

## 六、LLM 配置 (LLM Config) — `commands::llm_config_commands`

### llm_config_get

```rust
pub fn llm_config_get(
    state: State<'_, Arc<AppState>>,
) -> AppResult<LlmConfigGetResponse>
```

- **描述**：获取当前 LLM 路由配置、可用提供商列表和模型目录。返回 `LlmConfigGetResponse { routing, providers, catalog }`。providers 不包含 `ollama`；旧 `ollama` 路由读取时会 sanitize 为 DeepSeek 默认模型。

### llm_config_set

```rust
pub fn llm_config_set(
    state: State<'_, Arc<AppState>>,
    routing: LlmRoutingConfig,
) -> AppResult<()>
```

- **描述**：保存 LLM 路由配置（各场景的 provider/model 映射、自定义端点等）。校验 provider 合法性和 HTTPS URL；任何 `http://` base URL（包括 localhost）都会被拒绝。

### llm_config_apply_deepseek_defaults

```rust
pub fn llm_config_apply_deepseek_defaults(
    state: State<'_, Arc<AppState>>,
) -> AppResult<LlmRoutingConfig>
```

- **描述**：应用 DeepSeek 默认配置并保存。返回新的路由配置。

### connectivity_status

```rust
pub fn connectivity_status(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
) -> AppResult<ConnectivityStatusDto>
```

- **描述**：检查指定场景（或默认 KnowledgeLookup）的 LLM 连通性状态。

### llm_config_test

```rust
pub async fn llm_config_test(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<LlmConfigTestResult>
```

- **描述**：测试指定 LLM provider 的连接。先探测 `/models` 接口，失败时回退到最小对话请求。返回 `LlmConfigTestResult { ok, message }`。

---

## 七、联网证据配置 (Web Evidence Provider)

MiniMax 只保留普通 LLM provider 身份，不再公开 `minimax_config_*` 联网检索 IPC。联网证据配置通过 `web_evidence_provider_*` 命令管理 MCP provider；DuckDuckGo 是唯一内置原生托底。

### web_evidence_provider_upsert

```rust
pub fn web_evidence_provider_upsert(
    state: State<'_, Arc<AppState>>,
    input: WebEvidenceProviderInput,
) -> AppResult<WebEvidenceProviderSummary>
```

- **描述**：新增或更新 MCP 联网证据 provider。输入包含 `transport_config_json`、`credential_refs_json`、`search_mapping`、`fetch_mapping`；凭据字段只能保存 OS credential service 引用，不能保存明文 secret。

### web_evidence_providers_list

```rust
pub fn web_evidence_providers_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<WebEvidenceProviderSummary>>
```

- **描述**：列出可配置 MCP provider；MiniMax 不作为联网 provider 返回。

### web_evidence_provider_diagnostics

```rust
pub async fn web_evidence_provider_diagnostics(
    state: State<'_, Arc<AppState>>,
    provider_id: Option<String>,
    live_check: Option<bool>,
) -> AppResult<WebEvidenceProviderDiagnostics>
```

- **描述**：返回 transport、mapping、credential、broker 可用性和 live probe 诊断。只有 `live_check == true` 时才访问远端 MCP 服务。

---

## 八、知识图谱 (Graph) — `commands::graph`

### graph_data

```rust
pub fn graph_data(
    state: State<'_, Arc<AppState>>,
) -> AppResult<GraphData>
```

- **描述**：获取知识图谱数据（节点 + 边）。节点为所有已索引文件（含 link_count），边为所有 wikilink 关系。返回 `GraphData { nodes: Vec<GraphNode>, edges: Vec<GraphEdge> }`。

---

## 九、版本管理 (Version) — `commands::version`

### version_list_cmd

```rust
pub fn version_list_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> AppResult<Vec<VersionEntry>>
```

- **描述**：列出指定笔记的所有版本快照。返回 `VersionEntry { id, file_id, version_no, label, content_hash, word_count, is_finalized, kind, created_at }`。

### version_preview_cmd

```rust
pub fn version_preview_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
) -> AppResult<String>
```

- **描述**：预览指定版本的 Markdown 内容。

### version_restore_cmd

```rust
pub fn version_restore_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
    current_content: String,
) -> AppResult<VersionRestoreResult>
```

- **描述**：恢复到指定版本。当前内容会先保存为新版本。返回 `VersionRestoreResult { content }`。

### version_delete_cmd

```rust
pub fn version_delete_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
) -> AppResult<()>
```

- **描述**：删除指定版本快照。

### version_finalize_current_cmd

```rust
pub fn version_finalize_current_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    label: Option<String>,
) -> AppResult<Option<VersionEntry>>
```

- **描述**：将当前内容定稿为一个命名版本（带可选 label）。

### version_cleanup_cmd

```rust
pub fn version_cleanup_cmd(
    state: State<'_, Arc<AppState>>,
) -> AppResult<usize>
```

- **描述**：清理过期版本快照。返回清理的数量。

### version_save_manual_cmd

```rust
pub fn version_save_manual_cmd(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<()>
```

- **描述**：入队手动快照任务（立即返回）。完成后通过 `version:save_complete` 事件通知前端。

### version_save_idle_cmd

```rust
pub fn version_save_idle_cmd(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
) -> AppResult<()>
```

- **描述**：入队空闲自动快照任务（立即返回）。完成后通过 `version:save_complete` 事件通知前端。

---

## 十、模板 (Template) — `commands::template`

### template_list

```rust
pub fn template_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<TemplateInfo>>
```

- **描述**：列出所有笔记模板（含内置模板：会议纪要、读书笔记、项目复盘、每日记录）。首次调用时自动创建内置模板。返回 `TemplateInfo { name }`。

### template_create

```rust
pub fn template_create(
    state: State<'_, Arc<AppState>>,
    path: String,
    template_name: String,
) -> AppResult<FileEntry>
```

- **描述**：使用模板创建新笔记。模板不存在时使用 `# 标题` 格式。

### template_read

```rust
pub fn template_read(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> AppResult<String>
```

- **描述**：读取指定模板的 Markdown 内容。

### template_save

```rust
pub fn template_save(
    state: State<'_, Arc<AppState>>,
    name: String,
    content: String,
) -> AppResult<()>
```

- **描述**：保存/更新模板内容。不存在时自动创建。

### template_delete

```rust
pub fn template_delete(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> AppResult<()>
```

- **描述**：删除指定模板（含内置模板也可删除）。

---

## 十一、标签 (Tag) — `commands::tag`

### tag_list

```rust
pub fn tag_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<TagGroup>>
```

- **描述**：列出所有标签及其关联的文件。返回 `Vec<TagGroup>`，其中 `TagGroup { name, files: Vec<FileListItem> }`。

---

## 十二、导出 (Export) — `commands::export`

### export_file

```rust
pub fn export_file(
    _state: State<'_, Arc<AppState>>,
    dest_path: String,
    content: String,
) -> AppResult<()>
```

- **描述**：将内容导出到指定文件路径。禁止导出到系统敏感目录（如 `C:\Windows\`、`/etc/`）。

---

## 十三、语料库 (Corpus) — `commands::corpus_commands`

### corpus_list

```rust
pub fn corpus_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<CorpusListItem>>
```

- **描述**：列出所有语料库配置（从 `.iris/corpora.toml` 加载）。返回 `CorpusListItem { id, name, path_prefix, kind, scenes }`。

### corpus_upsert

```rust
pub fn corpus_upsert(
    state: State<'_, Arc<AppState>>,
    entry: CorpusUpsertPayload,
) -> AppResult<()>
```

- **描述**：插入或替换语料库条目（写入 `.iris/corpora.toml`）。`CorpusUpsertPayload { id, name, path_prefix, kind, scenes }`。

---

## 十四、写作工作流 (Writing) — `commands::writing_commands`

### writing_execute

```rust
pub async fn writing_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: WritingTaskInputIpc,
) -> AppResult<WritingTaskOutput>
```

- **描述**：执行写作任务（续写、改写、补充证据、生成提纲、统一语气等）。自动检测写作意图，检索本地+网络证据，调用 LLM 生成补丁建议。完成后发射 `ai:writing_complete` 事件。

### patch_apply

```rust
pub fn patch_apply(
    state: State<'_, Arc<AppState>>,
    patch: PatchProposal,
) -> AppResult<PatchApplyResult>
```

- **描述**：应用一个已验证的编辑补丁到文件（读取 -> 验证 hash -> 写入）。返回 `PatchApplyResult { success, new_content_hash, error, warnings }`。

---

## 十五、引用检查 (Citation) — `commands::citation_commands`

### citation_check

```rust
pub async fn citation_check(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: CitationCheckInput,
) -> AppResult<CitationCheckResult>
```

- **描述**：执行引用检查任务：(1) 从段落中提取事实声明 (2) 搜索本地证据 (3) 可选联网搜索 (4) 输出引用覆盖度评估 (5) 给出补充引用或改写建议。完成后发射 `ai:citation_check_complete` 事件。

---

## 十六、整理工作流 (Organize) — `commands::organize_commands`

### organize_execute

```rust
pub async fn organize_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: OrganizeTaskInput,
) -> AppResult<OrganizeTaskResult>
```

- **描述**：执行整理任务（全量审计、标题建议、标签建议、目录建议、链接建议等）。检索文件元数据并生成批量变更计划。完成后发射 `ai:organize_complete` 事件。

### organize_apply

```rust
pub fn organize_apply(
    state: State<'_, Arc<AppState>>,
    request: OrganizeApplyRequest,
) -> AppResult<OrganizeApplyResult>
```

- **描述**：应用用户确认的整理建议（批量）。支持 `RenameTitle`（含路径同步）、`MoveToFolder`、`AddTag`。返回 `OrganizeApplyResult { applied, skipped, errors }`。

---

## 十七、章节/文档写作 (Document) — `commands::document_commands`

### chapter_writing_execute

```rust
pub async fn chapter_writing_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: ChapterWritingInput,
) -> AppResult<ChapterWritingResult>
```

- **描述**：执行章节级写作任务（续写章节、重构章节等）。检索证据，调用 LLM 生成章节内容替换建议。完成后发射 `ai:chapter_writing_complete` 事件。

### document_check_execute

```rust
pub async fn document_check_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: DocumentCheckInput,
) -> AppResult<DocumentCheckResult>
```

- **描述**：执行文档级检查（完整性、一致性、质量评估等）。启发式分析 + LLM 增强分析。完成后发射 `ai:document_check_complete` 事件。

### parse_document_chapters

```rust
pub fn parse_document_chapters(
    content: String,
) -> AppResult<Vec<ChapterInfo>>
```

- **描述**：从 Markdown 内容中解析章节结构。返回 `Vec<ChapterInfo>`（标题、级别、内容范围等）。

---

## 十八、统一助手 (Assistant) — `commands::assistant_commands`

### assistant_execute

```rust
pub async fn assistant_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse>
```

- **描述**：统一助手入口——根据 `intent` 字段路由到不同工作流（Chat / Writing / Citation / Organize / Research / Chapter / Document）。通过 harness task 层执行。返回标记联合 `AssistantExecuteBody` + harness 元数据。

- **TaskPlan 过渡字段**：前端 `buildAssistantTaskPlan` 与 Rust `TaskPlanSummary` 共享四个可选语义字段，用于让 Task Policy / Capability Gate 派生策略，避免 `intent` 过载：
  - `evidenceNeed`：`"none"` | `"fresh_web"` | `"multi_source_research"`。fresh web 短答查证不升格为 research。
  - `contextNeed`：`"none"` | `"current_reference"` | `"vault_search"` | `"long_document"`。选区/引用走 `current_reference`。
  - `operationKind`：`"answer"` | `"patch"` | `"create"` | `"organize"` | `"diagnose"`。rewrite_selection 为 `patch`。
  - `outputShape`：`"chat"` | `"confirmation"` | `"artifact"` | `"diagnostic"`。rewrite_selection 走 `confirmation`，research 走 `artifact`。
  - 字段在 TS `src/types/ai.ts` 与 Rust `src-tauri/src/ai_types/mod.rs` 同名（serde `snake_case` + `skip_serializing_if = "Option::is_none"`），缺省按 `none`/`answer`/`chat` 处理，旧前端可继续只发 `intent`。

---

## 十九、AI 运行时 (AI Runtime) — `commands::ai_commands`

### context_assemble

```rust
pub async fn context_assemble(
    state: State<'_, Arc<AppState>>,
    scene: String,
    note_path: Option<String>,
    _note_content_hash: Option<String>,
    query: String,
    session_id: Option<i64>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
) -> AppResult<AssembledContext>
```

- **描述**：组装 AI 上下文——意图检测 + 检索规划。返回 `AssembledContext { provisional, packets, tools, context_status, execution_plan }`。`web_search` 只授权 WebEvidenceBroker 收集联网证据，不属于 `LlmGenerateParams`。

### ai_send_message

```rust
pub async fn ai_send_message(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    scene: String,
    session_id: Option<i64>,
    message: String,
    selected_packet_ids: Option<Vec<String>>,
    note_path: Option<String>,
    context_scope: Option<ContextScopeDto>,
    web_search: Option<bool>,
    new_session: Option<bool>,
) -> AppResult<serde_json::Value>
```

- **描述**：发送 AI 消息（完整 LLM pipeline）。包含：guardrails 注入检测、session 管理、上下文组装、harness 多轮执行、工具调用、trace 记录。返回包含 `request_id`, `session_id`, `status`, `content`, `tool_calls`, `usage` 等的 JSON。harness LLM 重试前会发出 `ai:retry_status` 事件，字段为 `request_id`, `attempt`, `max_attempts`, `delay_ms`，不包含 prompt、上下文包或文档正文。`web_search` 只授权 WebEvidenceBroker，URL 深读仍通过 broker URLs 进入证据链。

### tool_confirm

```rust
pub async fn tool_confirm(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
    tool_call_id: String,
    decision: String,
    modified_args: Option<serde_json::Value>,
) -> AppResult<serde_json::Value>
```

- **描述**：处理用户对工具调用的确认（批准/拒绝/修改参数）。批准后执行工具并恢复 harness 流程。发射 `ai:tool_result` 事件。

### ai_list_tools

```rust
pub fn ai_list_tools(
    scene: String,
) -> AppResult<Vec<serde_json::Value>>
```

- **描述**：获取指定场景下可用的 AI 工具列表（含 name, description, requires_confirmation, access_level）。

### knowledge_reindex

```rust
pub async fn knowledge_reindex(
    state: State<'_, Arc<AppState>>,
) -> AppResult<serde_json::Value>
```

- **描述**：重建所有知识索引（法规条款、锚点等）。返回 `{ anchors, regulations }` 统计。

### search_hybrid

```rust
pub async fn search_hybrid(
    state: State<'_, Arc<AppState>>,
    query: String,
    scene: Option<String>,
    note_path: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<serde_json::Value>>
```

- **描述**：跨所有知识层的混合搜索（FTS + 向量 + 图谱 + 精确匹配）。返回 context packet JSON 数组。默认 limit=15。

### session_list

```rust
pub async fn session_list(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
    note_path: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<SessionSummary>>
```

- **描述**：列出聊天会话（支持按 scene/note_path 过滤、分页）。返回 `Vec<SessionSummary>`。默认 limit=50, offset=0。

### session_delete

```rust
pub async fn session_delete(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
) -> AppResult<bool>
```

- **描述**：删除指定会话。

### session_rename

```rust
pub async fn session_rename(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    title: String,
) -> AppResult<()>
```

- **描述**：重命名会话标题。

### session_retract

```rust
pub async fn session_retract(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    from_seq: i64,
) -> AppResult<u32>
```

- **描述**：撤回消息——删除指定 seq 及之后的所有消息。返回被删除的消息数量。

### session_load

```rust
pub async fn session_load(
    state: State<'_, Arc<AppState>>,
    session_id: i64,
    limit: Option<u32>,
) -> AppResult<Vec<SessionMessage>>
```

- **描述**：加载指定会话的最近消息。返回 `Vec<SessionMessage>`。默认 limit=50。

### session_clear_all

```rust
pub async fn session_clear_all(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
    note_path: Option<String>,
) -> AppResult<u32>
```

- **描述**：删除匹配 scene/note_path 过滤条件的所有会话。返回删除数量。

### ai_cache_clear

```rust
pub async fn ai_cache_clear(
    state: State<'_, Arc<AppState>>,
) -> AppResult<serde_json::Value>
```

- **描述**：清除所有 AI 运行时缓存（sessions、harness checkpoints、knowledge deposits、web page cache、search cache）。返回各类型清理数量统计。

### harness_resume

```rust
pub async fn harness_resume(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
) -> AppResult<serde_json::Value>
```

- **描述**：从 checkpoint 恢复被中断的 harness 运行。

### harness_abort

```rust
pub async fn harness_abort(
    state: State<'_, Arc<AppState>>,
    request_id: String,
) -> AppResult<()>
```

- **描述**：中止活跃的 harness/model 请求。更新 trace 状态为 Aborted。

### skills_list

```rust
pub async fn skills_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<SkillEntry>>
```

- **描述**：列出所有已安装的 Skills（全局 + vault 级别）。返回 `Vec<SkillEntry>`。

### skills_create_draft

```rust
pub async fn skills_create_draft(
    state: State<'_, Arc<AppState>>,
    request: SkillCreateDraftRequest,
) -> AppResult<SkillDraftDto>
```

- **描述**：生成 prompt-only `SKILL.md` 草稿、目标路径和内容哈希。不会写入文件。
- **副作用**：无；调用方必须展示草稿并等待用户确认。

### skills_confirm

```rust
pub async fn skills_confirm(
    state: State<'_, Arc<AppState>>,
    draft: SkillDraftDto,
) -> AppResult<()>
```

- **描述**：校验草稿 markdown 与 `content_hash` 一致后写入 vault 级 `.iris/skills/<slug>/SKILL.md`，记录确认哈希并启用。
- **副作用**：写入已确认的 prompt-only Skill 文件与确认元数据。

### prompt_profile_get

```rust
pub async fn prompt_profile_get(
    state: State<'_, Arc<AppState>>,
) -> AppResult<PromptProfile>
```

- **描述**：获取当前用户的 prompt profile（自定义系统提示词配置）。

### prompt_profile_set

```rust
pub async fn prompt_profile_set(
    state: State<'_, Arc<AppState>>,
    profile: PromptProfile,
) -> AppResult<()>
```

- **描述**：保存用户的 prompt profile。

### prompt_profile_presets

```rust
pub fn prompt_profile_presets() -> Vec<serde_json::Value>
```

- **描述**：列出内置的 prompt profile 预设模板。返回 `[{ label, profile }]`。

---

## 二十、研究工作流 (Research) — `commands::research_commands`

### research_execute

```rust
pub async fn research_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    topic: String,
    web_authorized: Option<bool>,
) -> AppResult<serde_json::Value>
```

- **描述**：执行完整研究工作流（半自主多轮研究）。支持中途取消，每轮发射 `ai:research_progress` 进度事件。返回包含 `request_id`, `topic`, `rounds`, `evidence_matrix`, `argument_chain`, `summary`, `total_tokens` 的 JSON。

### research_status

```rust
pub fn research_status(
    state: State<'_, Arc<AppState>>,
) -> AppResult<serde_json::Value>
```

- **描述**：获取研究工作流状态（最近 10 条研究 trace）。返回 `{ recent_research: [...] }`。

### research_abort

```rust
pub fn research_abort(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
) -> AppResult<()>
```

- **描述**：中止运行中的研究任务。发射 `ai:research_progress` 事件（Aborted 状态）。

### research_active_tasks

```rust
pub fn research_active_tasks(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<String>>
```

- **描述**：列出所有活跃研究任务的 request_id。

### research_generate_note

```rust
pub fn research_generate_note(
    _state: State<'_, Arc<AppState>>,
    request: ResearchNoteRequest,
) -> AppResult<ResearchNoteResult>
```

- **描述**：从研究结果生成结构化研究笔记（含 frontmatter、研究背景、摘要、证据概览、后续方向、参考文献等章节）。返回 `ResearchNoteResult { content, suggested_path, section_count }`。

---

## 二十一、个性化 (Profile) — `commands::profile_commands`

### profile_list

```rust
pub fn profile_list(
    state: State<'_, Arc<AppState>>,
    include_inactive: Option<bool>,
) -> AppResult<Vec<ProfileEntry>>
```

- **描述**：列出所有用户画像条目。默认仅返回活跃条目。`ProfileEntry { key, value, source, confidence, is_active, updated_at }`。

### profile_get

```rust
pub fn profile_get(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> AppResult<Option<ProfileEntry>>
```

- **描述**：根据 key 获取单个画像条目。

### profile_set

```rust
pub fn profile_set(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: serde_json::Value,
    source: String,
    confidence: Option<f64>,
) -> AppResult<()>
```

- **描述**：设置/更新画像条目（upsert）。**安全过滤**：拒绝包含 API Key、密码、私钥等敏感内容的值，拒绝超过 4096 字符的值。默认 confidence=1.0。

### profile_set_rule

```rust
pub fn profile_set_rule(
    state: State<'_, Arc<AppState>>,
    key: String,
    description: String,
    source: Option<String>,
) -> AppResult<()>
```

- **描述**：以纯文本描述保存规则（写入 `{"description": "..."}` 结构）。默认 source="user_manual"。

### profile_deactivate

```rust
pub fn profile_deactivate(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> AppResult<()>
```

- **描述**：停用画像条目（软删除，设 `is_active = 0`）。

### profile_delete

```rust
pub fn profile_delete(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> AppResult<()>
```

- **描述**：永久删除画像条目。

### inbox_list

```rust
pub fn inbox_list(
    state: State<'_, Arc<AppState>>,
    status: Option<String>,
) -> AppResult<Vec<KnowledgeDeposit>>
```

- **描述**：列出知识收件箱条目（按状态过滤）。默认 status="inbox"。`KnowledgeDeposit { id, session_id, source_note, deposit_type, content, status, target_path, created_at, updated_at }`。

### inbox_add

```rust
pub fn inbox_add(
    state: State<'_, Arc<AppState>>,
    deposit_type: String,
    content: String,
    source_note: Option<String>,
    session_id: Option<i64>,
) -> AppResult<i64>
```

- **描述**：添加知识收件箱条目。返回新条目的 ID。

### inbox_update_status

```rust
pub fn inbox_update_status(
    state: State<'_, Arc<AppState>>,
    deposit_id: i64,
    new_status: String,
    target_path: Option<String>,
) -> AppResult<()>
```

- **描述**：更新收件箱条目状态。状态流转：`"inbox"` -> `"archived"` -> `"written"`。

### inbox_delete

```rust
pub fn inbox_delete(
    state: State<'_, Arc<AppState>>,
    deposit_id: i64,
) -> AppResult<()>
```

- **描述**：删除收件箱条目。

### inbox_counts

```rust
pub fn inbox_counts(
    state: State<'_, Arc<AppState>>,
) -> AppResult<serde_json::Value>
```

- **描述**：获取收件箱各状态的数量统计。返回 `{ inbox, archived, written }`。

---

## 二十二、窗口外观 (Window Chrome) — `commands::window_chrome_cmd`

### get_desktop_chrome_metrics

```rust
pub fn get_desktop_chrome_metrics(
    window: WebviewWindow,
) -> DesktopChromeMetrics
```

- **描述**：返回当前平台的顶栏高度与窗口控件预留指标，供前端写入 CSS 变量。macOS 使用左侧系统原生红黄绿，Iris Rail 通过 `traffic_inset_logical: 88` 预留窗口态安全区；fullscreen 时前端通过 `data-iris-window-fullscreen` 将该 spacer 收为 `0px`。Windows / Linux 使用右侧自绘窗口控件且该值为 `0`。`DesktopChromeMetrics { titlebar_height_logical, traffic_inset_logical, scale_factor }`。

### reapply_window_chrome

```rust
pub fn reapply_window_chrome(
    window: WebviewWindow,
)
```

- **描述**：重新应用无边框窗口标题与平台圆角；当前前端不再为 macOS 交通灯重定位调用该接口。

### app_exit

```rust
pub fn app_exit(
    app: AppHandle,
)
```

- **描述**：在前端完成关闭守卫后退出 Tauri 应用。

---

## 附录：关键外部类型定义

| 类型                  | 定义位置                                | 字段                                                                                                                                                                 |
| --------------------- | --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FileEntry`           | `src-tauri/src/indexer/scan.rs:23`      | `id: i64, path: String, title: String, updated_at: String, word_count: i64`                                                                                          |
| `RecycleBinItem`      | `src-tauri/src/recycle/mod.rs:46`       | `id: String, original_path: String, title: String, deleted_at: String, expires_at: String, version_count: usize`                                                     |
| `SemanticHit`         | `src-tauri/src/embedding/engine.rs:201` | `chunk_id: i64, path: String, title: String, snippet: String, score: f32`                                                                                            |
| `LlmGenerateParams`   | `src-tauri/src/llm/mod.rs:44`           | `provider: String, model: Option<String>, messages: Vec<ChatMessage>, system: Option<String>, stream: Option<bool>, custom_base_url: Option<String>`                 |
| `VersionEntry`        | `src-tauri/src/version/mod.rs:20`       | `id: i64, file_id: i64, version_no: String, label: Option<String>, content_hash: String, word_count: i64, is_finalized: bool, kind: VersionKind, created_at: String` |
| `PatchProposal`       | `src-tauri/src/ai_types/mod.rs:325`     | `id, target_path, base_content_hash, range: SourceSpan, original_text, replacement_text, evidence_packet_ids, risk_level, warnings, created_at`                      |
| `PatchApplyResult`    | `src-tauri/src/ai_types/mod.rs:385`     | `success: bool, new_content_hash: Option<String>, error: Option<String>, warnings: Vec<String>`                                                                      |
| `CitationCheckInput`  | `src-tauri/src/ai_types/mod.rs:527`     | `paragraph_text, document_path, scope: Option<CitationCheckScope>, web_authorized`                                                                                   |
| `CitationCheckResult` | `src-tauri/src/ai_types/mod.rs:544`     | `request_id, claims: Vec<FactClaim>, coverage: CitationCoverage, suggestions, evidence_used, total_tokens`                                                           |
| `OrganizeTaskInput`   | `src-tauri/src/ai_types/mod.rs:633`     | `scope: Option<OrganizeTaskScope>, task_type: OrganizeTaskType`                                                                                                      |
| `OrganizeTaskResult`  | `src-tauri/src/ai_types/mod.rs:659`     | `request_id, batch: OrganizeBatch, total_tokens`                                                                                                                     |
| `ResearchNoteRequest` | `src-tauri/src/ai_types/mod.rs:700`     | `topic, summary, evidence_count, coverage_score, target_path`                                                                                                        |
| `ResearchNoteResult`  | `src-tauri/src/ai_types/mod.rs:710`     | `content, suggested_path, section_count`                                                                                                                             |
| `TokenUsage`          | `src-tauri/src/ai_types/mod.rs:720`     | `prompt_tokens, completion_tokens, total_tokens, prompt_cache_hit_tokens, prompt_cache_miss_tokens`                                                                  |

---

## 附录：注册顺序（lib.rs invoke_handler）

完整命令注册顺序以 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 为准。AI 能力收口后，Skills / Web Evidence 相关公开命令为：

```
skills_list, skills_paths, skills_create_draft, skills_confirm,
web_evidence_provider_upsert, web_evidence_providers_list,
web_evidence_provider_toggle, web_evidence_provider_delete,
web_evidence_provider_diagnostics
```

---

## Prompt-Only Skills and Web Evidence Provider IPC Contract

This section is the stable frontend contract after the Iris AI reign-in.

### Prompt-Only Skills

- Skills are prompt-only `SKILL.md` files created inside Iris and confirmed by the user.
- SKILL.md scope is the fact source; SQLite stores only enable/index state and confirmed hash metadata.
- Changed skill content or scope changes the hash and requires reconfirmation before activation.
- Frontend callers use `skillsCreateDraft` and `skillsConfirm` for the creation flow.

### Web Evidence Providers

- MCP is not exposed as arbitrary agent tooling. It is only a Web Evidence Provider when explicitly mapped to `web.search` or `web.fetch`. MCP provider input persists `transportConfigJson`, `credentialRefsJson`, `searchMapping`, and `fetchMapping`; credential JSON may contain OS credential service refs only, never raw secrets.
- Frontend callers use `webEvidenceProvidersList`, `webEvidenceProviderUpsert`, `webEvidenceProviderToggle`, `webEvidenceProviderDelete`, and `webEvidenceProviderDiagnostics`.
- Provider diagnostics belong in management center UI, not in ordinary AI evidence packets. `webEvidenceProviderDiagnostics(providerId?, liveCheck?)` returns checks, failure categories, credential/mapping readiness, and broker usability booleans; live network probes run only when `liveCheck` is true.

### Tool Confirmation Outcomes

- External web evidence and conflicts appear through the existing AI evidence packet UI and evidence detail temporary tab.
- Provider process details, failures, and cache diagnostics are management-center diagnostics.
