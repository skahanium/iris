# Iris IPC API 参考

## Retired contracts

The current frontend event registry contains no `version:save_complete` or `llm:reset` event. The only Run lifecycle event is `assistant:run_event`.

The following internal commands are retired and must not be registered in `generate_handler!`, declared in `src/types/ipc.ts`, or wrapped by `src/lib/ipc.ts`: `llm_providers`, `version_cleanup_cmd`, `document_title_audit_cmd`, `skills_paths`, and `classified_ai_retrieval_clear`.

Tauri 命令注册在 [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)，前端类型定义在 [`src/types/ai.ts`](../src/types/ai.ts) 与 [`src/types/ipc.ts`](../src/types/ipc.ts)，调用封装在 [`src/lib/ipc.ts`](../src/lib/ipc.ts)。这三处是命令名、参数和返回类型的权威来源；本文只记录稳定的边界规则。

## 调用规则

- React 组件只能调用 `src/lib/ipc.ts` 的类型安全封装，禁止直接 `invoke()`。
- 修改 Rust `#[tauri::command]` 签名时，必须同步修改 TypeScript 类型、封装、测试和本文。
- 路径、provider、凭据、写入和工具调用必须在 Rust command 边界重新验证；前端类型不是安全边界。

## 命令分组

| 分组              | 主要命令前缀/示例                                                                                                                       | 责任                                                                                        |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| 设置与凭据        | `settings_*`、`credential_*`                                                                                                            | 非敏感设置与本地加密凭据状态                                                                |
| Vault 与文件      | `vault_*`、`file_*`、`folder_*`、`media_*`                                                                                              | Markdown、资源、目录、锁与索引扫描；`file_write` 返回落盘回执                               |
| 版本与回收站      | `version_*`、`recycle_*`                                                                                                                | 快照、恢复、清理与回收站                                                                    |
| 搜索与知识        | `search_*`、`embedding_scheduler_*`、`knowledge_reindex`、`tag_list`、`graph_data`、`corpus_*`                                          | FTS、语义搜索、知识结构；后台嵌入调度器状态、启动与暂停控制                                 |
| LLM 配置          | `llm_*`、`connectivity_status`                                                                                                          | provider、模型、路由与连通性；不执行助手请求                                                |
| Agent Run         | `assistant_run_start`、`assistant_run_control`、`assistant_run_get`                                                                     | 按安全域路由的执行、控制与状态读取；normal-domain 持久化回放，classified 仅限进程内易失状态 |
| Agent 会话        | `assistant_session_list`、`assistant_session_load`、`assistant_session_rename`、`assistant_session_delete`、`assistant_session_retract` | 仅通过 `AssistantSessionRef` 访问、与当前文档解绑的域隔离历史                               |
| Skills 与联网证据 | `skills_*`、`web_evidence_provider_*`、`prompt_profile_*`                                                                               | prompt-only Skills、联网证据 provider 与个性化                                              |
| 涉密数据          | `classified_*`、`assistant_classified_run_take_result`                                                                                  | 加密分类空间、易失涉密 Run 与一次性结果读取；不共享 normal Run 回放                         |
| 窗口              | `app_exit`、`get_desktop_chrome_metrics`、`show_main_window_when_ready`                                                                 | 桌面窗口生命周期与 Chrome 指标                                                              |
| 订阅资料库        | `feed_*`                                                                                                                                | RSS/Atom/JSON Feed 发现、订阅、同步、阅读状态与本地 FTS 搜索                                |

## Agent Run 契约

- `explicitReferences` 表示用户主动授权的本轮材料。后端重新读取并校验路径、内容哈希和 UTF-8 范围后，将选区或 `@` 文档作为授权材料直接送入 Provider Prompt；不得根据 corpus 角色过滤或静默降级为无引用请求。文件夹和标签仍表示检索范围，不代表全文附件。此规则不改变 IPC 字段、Run wire 或历史消息格式。

- normal-domain 请求只能使用 `assistant_run_start`。请求包含显式会话、显式引用、可选的一次性 `explicitAction` 和安全域；当前编辑器、活动 tab、scene、intent、旧任务 ID 和笔记正文都不是隐式输入。
- 编辑器选区候选是 renderer 内存中的临时 UI 状态，不是 IPC 输入；只有用户明确发送且候选仍通过磁盘内容哈希与 UTF-8 范围校验时，才转换为本次 `assistant_run_start` 的显式 `ContextReference`。选区取消、文档切换或 Agent 隐藏后不得提交或保留该引用。
- 选区预览文字不得进入 IPC、持久化事件、日志或会话；后端按显式路径、内容哈希和范围重新读取权威 Markdown。锁定普通文档只读引用不改变权限，未保存/无法映射选区必须在前端阻止发送；classified 文档不走 normal-domain 选区引用。
- normal-domain 生命周期事件只有 `assistant:run_event`。事件先持久化再发送；前端断流后使用 `assistant_run_get` 回放，不订阅 `llm:*`、`ai:*`、Harness 或工具确认事件。回放日志不包含工具参数或原始输出，只返回安全快照和受限过程展示。
- `assistant_run_control` 以预期 state version 进行幂等控制；取消、确认和恢复不使用平行的 task/harness API。
- `assistant_run_get` 的断流回放不是进程级执行恢复：Direct 与 ToolLoop 不支持进程级续跑，进程中断后不能由事件重新发起模型或工具。Durable Run 的暂停与检查点仅在其冻结计划、用户确认和内容 hash 复核均满足时才可进入恢复路径。
- 涉密 Run 仅在当前进程内易失执行。`assistant_run_get` 只接受显式 `runId`，按该 ID 读取无正文的易失快照与安全事件；不支持省略 `runId` 的“最近活动 Run”查询、持久化断流回放或进程级恢复。它不持久化事件、prompt 或模型输出；完成内容只能经 `assistant_classified_run_take_result` 在有效文档上下文中一次性读取，进程退出或易失状态清理后即失效。
- 会话 ID 对前端是不透明的 `AssistantSessionRef`，不能用数据库主键、文档路径或涉密文件路径寻址。
- 已移除 `assistant_execute`、`ai_send_message`、`context_assemble`、`tool_confirm`、`session_*`、`agent_task_*`、`harness_*` 以及独立 writing/citation/organize/chapter/document/research 执行入口；不得恢复兼容封装。

## 写入与安全

涉及笔记正文的变更必须先生成可审计的变更计划与预览，并在应用前校验目标、计划 hash 与最新内容 hash。未经用户确认，Iris 不得修改用户 `.md` 文件。

`file_write` 的成功语义仅是“指定 Markdown 已耐久原子落盘”。其 `FileWriteResult` 包含 `entry`、`contentHash` 与 `indexStatus`：`synced` 表示派生索引同步完成，`degraded` 表示 Markdown 已保存、索引修复已排队。调用方不得因 `degraded` 回滚、删除或拒绝该 Markdown，也不得把它显示为保存失败。所有正文写入入口（普通保存、创建、AI 应用、版本恢复、模板和链接级联）必须复用同一后端写入服务。

嵌入重建没有同步阻塞接口。`embedding_scheduler_status` 返回 `EmbeddingIndexStatus`，`embedding_scheduler_start` 立即返回 `started`、`already_running` 或调试运行时的 `disabled`，`embedding_scheduler_set_paused` 在批次边界暂停/恢复，`embedding_scheduler_set_foreground_busy` 只报告前台活动。前端持续消费完整状态事件；运行、暂停、失败、`disabled`、进度和自动尝试标记均以服务端状态为准，不能由组件自行推断。旧 `search_embedding_status` 与同步 `search_reindex` 已移除，不得恢复兼容入口。

`credential_set`、`credential_has`、`credential_status`、`credential_delete` 只处理服务名和状态；任何返回值、日志、诊断或错误均不得含有秘密值。

## Skills 与联网证据

Skills are prompt-only；`SKILL.md` scope is the fact source。`skills_*` 不安装依赖、不执行脚本、不暴露外部运行时。

联网证据由 `webEvidenceProvidersList`、`webEvidenceProviderDiagnostics` 与相关 provider IPC 管理。普通 LLM provider 不作为联网证据后端；只有被显式映射并通过诊断的 Web provider 才能进入 `WebEvidenceBroker`。

通用 MCP 只读工具使用独立的管理面 IPC：

- `mcp_read_only_tools_discover(providerId)`：实时 discovery 后仅返回服务端声明只读、名称和受支持输入 Schema 均通过审查的候选；不返回服务端 description。`readOnlyHint` 只是候选前提，不是对第三方实现的证明。
- `mcp_capability_bindings_list(providerId?)`、`mcp_capability_binding_upsert(input)`、`mcp_capability_binding_delete(bindingId)`：管理稳定 binding。Upsert 会再次 discovery、复核声明与 provider 配置 hash，并要求 `userTrusted: true` 表示用户已对精确 provider/tool/schema 完成二次确认；renderer 不能仅凭服务端声明自行证明只读。
- `AssistantRunStartRequest.externalToolGrants`：只接受 `{ bindingId, bindingConfigHash }`；仅 normal-domain、非 local-only Run 可在 Accept 事务中冻结并获得 `external.read`。

启用 MCP provider 或保存 binding 不会自动授权任何 Run。运行时不重新 discovery，并拒绝 provider disable/config hash 漂移、snapshot 或用户信任位篡改、Schema 不匹配和超限/不支持输出。Iris 会拒绝声明或 Schema 暴露副作用的工具，但不能独立证明用户已信任的第三方服务端忠实实现其只读声明。

## 订阅资料库（feed\_\*）

命令名、DTO 与 `src/types/ipc.ts` / `src/lib/ipc.ts` 一一对应（camelCase）；
Rust 侧契约见 `feed::model`。仅登记以下命令，全部通过仓储/service 访问
应用级 SQLite，不内嵌 SQL：

| 命令                         | 参数（camelCase）                                     | 返回                                                          |
| ---------------------------- | ----------------------------------------------------- | ------------------------------------------------------------- |
| `feed_discover`              | `url`                                                 | `FeedCandidate[]`（≤10，不含 HTML）                           |
| `feed_source_add`            | `input: FeedSourceAddInput`                           | `FeedSourceSummary`                                           |
| `feed_source_list`           | —                                                     | `FeedSourceSummary[]`                                         |
| `feed_source_update`         | `sourceId`、`patch: FeedSourceUpdateInput`            | —                                                             |
| `feed_source_trash`          | `sourceId`                                            | 移入 RSS 回收站的文章数                                       |
| `feed_source_trash_restore`  | `sourceId`                                            | 恢复来源及本次退订文章；来源保持暂停                          |
| `feed_source_trash_purge`    | `sourceId`                                            | 永久删除来源及文章数                                          |
| `feed_source_item_count`     | `sourceId`                                            | 来源下文章总数                                                |
| `feed_source_trash_preview`  | `sourceId`                                            | 退订确认所需文章数、收藏数和清理时间                          |
| `feed_source_trash_match`    | `url`                                                 | 相同规范 URL 的可恢复来源，或 `null`                          |
| `feed_library_summary`       | —                                                     | 订阅维护页的来源、文章、未读、失败及最近成功同步汇总          |
| `feed_trash_list`            | —                                                     | `FeedTrashSnapshot`；来源退订按来源分组，普通到期文章单列     |
| `feed_trash_restore`         | `itemId`                                              | —                                                             |
| `feed_trash_clear`           | —                                                     | 物理删除的回收站条目数                                        |
| `feed_library_optimize`      | —                                                     | 显式 SQLite 空间优化                                          |
| `feed_item_list`             | `query: FeedItemQuery`                                | `FeedItemSummary[]`（limit 1..=200）                          |
| `feed_item_get`              | `itemId`                                              | `FeedItemDetail`                                              |
| `feed_item_set_state`        | `itemId`、`patch: FeedItemStatePatch`（至少一个字段） | —                                                             |
| `feed_fulltext_enqueue_item` | `itemId`                                              | `queued`、`already_queued`、`already_ready` 或 `not_eligible` |
| `feed_document_prepare`      | `itemId`                                              | opaque PDF lease；仅用户点击时下载                            |
| `feed_document_cancel`       | `itemId`                                              | 取消排队或下载中的 PDF                                        |
| `feed_document_release`      | `handle`                                              | 释放 PDF lease                                                |
| `feed_images_authorize`      | `itemId`                                              | 记录单篇授权并返回可渐进请求的图片清单；不下载图片            |
| `feed_image_prepare`         | `itemId`、`index`、`forceRetry?`                      | 为清单中的单张图片签发本地 opaque lease                       |
| `feed_images_release`        | `handles[]`（最多 256）                               | 释放当前阅读器持有的图片 lease                                |
| `feed_items_mark_read`       | `query: FeedItemQuery`（冻结筛选）                    | 影响行数                                                      |
| `feed_sync_source`           | `sourceId`、`markHistoryRead?`（仅首次同步生效）      | `FeedSyncOutcome`（等待完成）                                 |
| `feed_sync_all`              | —                                                     | `FeedSyncBatchOutcome`（全部启用源，并发最多 2）              |
| `feed_sync_batch`            | `sourceIds`、`markHistoryRead?`                       | `FeedSyncBatchOutcome`（有界批量，并发最多 2）                |
| `feed_opml_import`           | `xml`（≤ 5 MiB 有界 UTF-8）、`dryRun?`                | `OpmlImportResult`（added/updated/skipped/addedIds）          |
| `feed_opml_export`           | —                                                     | OPML 2.0 文档字符串（不含内部状态）                           |

边界规则：

- **原始源载荷永不出 IPC**：`FeedItemDetail` 只有规范化 Markdown 与安全
  元数据；`source_payload` 原始载荷与用于本地检索的纯文本 `content_text` 永不进入任何参数或
  返回值，前端类型也不得声明该字段。
- **OPML 不接收文件路径**：`feed_opml_import` 只收有界 UTF-8 字符串；
  文件选择/保存由前端 dialog + fs 完成。导入只更新 folder/override、
  不重置同步与阅读状态；导出不含 ETag、错误、阅读状态或本地 ID，按
  `folder_path` 稳定排序为嵌套大纲。
- **同步事件** `feed:changed` 只投影 `sourceId`、变更类型、`newItems` 与
  稳定 `errorCode`，不含 URL、正文或请求头；事件只提示 UI 重新查询，
  不建立 job 恢复协议，应用重启后按 `next_fetch_at` 恢复。
- **输入有界**：ID ≤ 200、URL ≤ 2048（且必须通过 SSRF 校验）、string
  ≤ 4096；`FeedItemQuery.search` 必须非空且有界；游标时间/行号与批量来源数量均校验；`feed_item_set_state` 空 patch 拒绝。`feed_sync_batch` 接受最多 10,000 个有界来源 ID，执行端仍固定并发最多 2。
- **退订语义**：暂停同步通过 `feed_source_update` 设置 `isEnabled=false`；移入 RSS 回收站使用 `feed_source_trash`。来源和本次退订文章保留 30 天，恢复后保持暂停，不能复活此前按保留期限删除的文章。
- **网页正文补全**：`feed_source_update.fulltextEnabled` 是来源级开关；新建和升级后的来源默认开启。后续摘要型文章自动进入有界队列；升级前摘要或旧提取版本的网页正文仅在用户打开该文章时通过 `feed_fulltext_enqueue_item` 单篇重取。提取器读取通用 scholarly/OG/DC 元数据与语义容器并评分，不使用域名规则；失败时清除旧混乱正文并恢复 Feed 摘要。
- **PDF 主文档**：只接受通用论文元数据或明确 `application/pdf` 链接识别出的 HTTPS PDF。下载沿用唯一系统代理与固定 IP 传输；不同 URL 单任务排队、同一规范 URL 共享进行中的下载，等待与下载合计最多 180 秒，取消可中断两种状态。单文件最多 100 MiB、64 KiB 写入块；前端只获得短期 opaque lease，持有期间缓存清理不会删除对应文件。`feed:document-progress` 仅含 `itemId/status/bytes`，不含 URL、代理或本地路径。
- 发现的多候选必须由用户选择，不自动订阅全部；候选 URL 与请求同等
  校验（跨协议/私网拒绝）。
