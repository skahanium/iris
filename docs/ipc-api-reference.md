# Iris IPC API 参考

Tauri 命令注册在 [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)，前端类型定义在 [`src/types/ai.ts`](../src/types/ai.ts) 与 [`src/types/ipc.ts`](../src/types/ipc.ts)，调用封装在 [`src/lib/ipc.ts`](../src/lib/ipc.ts)。这三处是命令名、参数和返回类型的权威来源；本文只记录稳定的边界规则。

## 调用规则

- React 组件只能调用 `src/lib/ipc.ts` 的类型安全封装，禁止直接 `invoke()`。
- 修改 Rust `#[tauri::command]` 签名时，必须同步修改 TypeScript 类型、封装、测试和本文。
- 路径、provider、凭据、写入和工具调用必须在 Rust command 边界重新验证；前端类型不是安全边界。

## 命令分组

| 分组              | 主要命令前缀/示例                                                                                                                       | 责任                                                                 |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| 设置与凭据        | `settings_*`、`credential_*`                                                                                                            | 非敏感设置与本地加密凭据状态                                         |
| Vault 与文件      | `vault_*`、`file_*`、`folder_*`、`media_*`                                                                                              | Markdown、资源、目录、锁与索引扫描                                   |
| 版本与回收站      | `version_*`、`recycle_*`                                                                                                                | 快照、恢复、清理与回收站                                             |
| 搜索与知识        | `search_*`（含 `search_embedding_status`）、`knowledge_reindex`、`tag_list`、`graph_data`、`corpus_*`                                   | FTS、语义搜索、知识结构；`EmbeddingIndexStatus` 描述嵌入索引状态     |
| LLM 配置          | `llm_*`、`connectivity_status`                                                                                                          | provider、模型、路由与连通性；不执行助手请求                         |
| Agent Run         | `assistant_run_start`、`assistant_run_control`、`assistant_run_get`                                                                     | 唯一的执行、取消、确认、恢复与断流回放入口                           |
| Agent 会话        | `assistant_session_list`、`assistant_session_load`、`assistant_session_rename`、`assistant_session_delete`、`assistant_session_retract` | 仅通过 `AssistantSessionRef` 访问、与当前文档解绑的域隔离历史        |
| Skills 与联网证据 | `skills_*`、`web_evidence_provider_*`、`prompt_profile_*`                                                                               | prompt-only Skills、联网证据 provider 与个性化                       |
| 涉密数据          | `classified_*`                                                                                                                          | 加密分类空间与内存索引清理；涉密 Run 仍只通过 `assistant_run_*` 访问 |
| 窗口              | `app_exit`、`get_desktop_chrome_metrics`、`show_main_window_when_ready`                                                                 | 桌面窗口生命周期与 Chrome 指标                                       |

## Agent Run 契约

- 发起请求只能使用 `assistant_run_start`。请求包含显式会话、显式引用、可选的一次性 `explicitAction` 和安全域；当前编辑器、活动 tab、scene、intent、旧任务 ID 和笔记正文都不是隐式输入。
- 生命周期事件只有 `assistant:run_event`。事件先持久化再发送；前端断流后使用 `assistant_run_get` 回放，不订阅 `llm:*`、`ai:*`、Harness 或工具确认事件。
- `assistant_run_control` 以预期 state version 进行幂等控制；取消、确认和恢复不使用平行的 task/harness API。
- 会话 ID 对前端是不透明的 `AssistantSessionRef`，不能用数据库主键、文档路径或涉密文件路径寻址。
- 已移除 `assistant_execute`、`ai_send_message`、`context_assemble`、`tool_confirm`、`session_*`、`agent_task_*`、`harness_*` 以及独立 writing/citation/organize/chapter/document/research 执行入口；不得恢复兼容封装。

## 写入与安全

涉及笔记正文的变更必须先生成可审计的变更计划与预览，并在应用前校验目标、计划 hash 与最新内容 hash。未经用户确认，Iris 不得修改用户 `.md` 文件。

`credential_set`、`credential_has`、`credential_status`、`credential_delete` 只处理服务名和状态；任何返回值、日志、诊断或错误均不得含有秘密值。

## Skills 与联网证据

Skills are prompt-only；`SKILL.md` scope is the fact source。`skills_*` 不安装依赖、不执行脚本、不暴露外部运行时。

联网证据由 `webEvidenceProvidersList`、`webEvidenceProviderDiagnostics` 与相关 provider IPC 管理。普通 LLM provider 不作为联网证据后端；只有被显式映射并通过诊断的 Web provider 才能进入 `WebEvidenceBroker`。
