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

## Agent Run 契约

- normal-domain 请求只能使用 `assistant_run_start`。请求包含显式会话、显式引用、可选的一次性 `explicitAction` 和安全域；当前编辑器、活动 tab、scene、intent、旧任务 ID 和笔记正文都不是隐式输入。
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
