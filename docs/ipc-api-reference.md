# Iris IPC API 参考

Tauri 命令注册在 [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)，前端类型定义在 [`src/types/ipc.ts`](../src/types/ipc.ts)，调用封装在 [`src/lib/ipc.ts`](../src/lib/ipc.ts)。这三处是命令名、参数和返回类型的权威来源；本文提供稳定的分类与契约规则，而不重复维护易过期的命令总数。

## 调用规则

- React 组件只能调用 `src/lib/ipc.ts` 导出的类型安全函数，禁止直接 `invoke()`。
- Rust `#[tauri::command]` 签名改变时，必须同步 TypeScript 类型、封装函数、相关测试和本文档。
- 路径、provider、凭据、写入和工具调用都必须在 Rust command 边界再次验证；前端类型不是安全边界。

## 命令分组

| 分组          | 主要命令前缀/示例                                                                | 责任                                   |
| ------------- | -------------------------------------------------------------------------------- | -------------------------------------- |
| 设置与凭据    | `settings_*`、`credential_*`                                                     | 非敏感设置与本地加密凭据状态           |
| Vault 与文件  | `vault_*`、`file_*`、`folder_*`、`media_*`                                       | Markdown、资源、目录、锁与索引扫描     |
| 版本与回收站  | `version_*`、`recycle_*`                                                         | 快照、恢复、清理和回收站               |
| 搜索与知识    | `search_*`、`knowledge_reindex`、`tag_list`、`graph_data`、`corpus_*`            | FTS、语义、检索证据与知识结构          |
| LLM 配置      | `llm_*`、`connectivity_status`                                                   | provider、模型、路由与连通性           |
| AI Runtime    | `assistant_*`、`ai_*`、`session_*`、`agent_task_*`、`harness_*`                  | 助手会话、上下文、任务、工具确认和诊断 |
| 工作流        | `writing_*`、`citation_*`、`organize_*`、`chapter_*`、`document_*`、`research_*` | 写作、引用、整理、文档与研究流程       |
| Skills 与证据 | `skills_*`、`web_evidence_provider_*`、`prompt_profile_*`                        | prompt-only Skills、联网证据和个性化   |
| 分类数据      | `classified_*`                                                                   | 加密分类空间及其独立内存检索           |
| 窗口          | `app_exit`、`get_desktop_chrome_metrics`、`show_main_window_when_ready`          | 桌面窗口生命周期与 Chrome 指标         |

## 关键契约

### 写入

涉及笔记内容的 API 必须使用当前内容 hash 或明确目标范围；AI 产生的改动通过提案与确认流程写入。Iris 不可在没有用户确认的情况下修改用户 `.md`。

### 凭据

`credential_set`、`credential_has`、`credential_status`、`credential_delete` 只处理服务名与状态。任何返回值、日志、诊断或错误都不得包含秘密值。

### 检索

`search_keyword`、`search_semantic`、`search_reindex` 服务于普通搜索；`search_hybrid` 返回 AI `ContextPacket`。v1.2.6 将新增嵌入索引状态与进度契约，必须同步本文件、Rust/TS 类型和测试后才可使用。

### 事件

需要长时间运行进度的功能使用 Tauri event；事件 payload 必须有稳定类型、可取消语义和脱敏错误文本。现有 research 进度封装在 `src/lib/ipc.ts`；后续嵌入重建沿用同一原则。
