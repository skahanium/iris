# 工具目录总览

<!-- iris:object README-TOOLS kind=rules file=true -->

本目录是 39 个内置业务工具的正式定义位置（工具卡）。工具名是**能力接口，不是权限**：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

## 通用能力目录（16 个）

| 工具                    | ID    | 语义归属     | 卡片                                                   |
| ----------------------- | ----- | ------------ | ------------------------------------------------------ |
| `system_time_now`       | `T01` | `M06`        | [system_time_now.md](./system_time_now.md)             |
| `app_context_read`      | `T02` | `M01`／`M03` | [app_context_read.md](./app_context_read.md)           |
| `capabilities_read`     | `T03` | `M06`        | [capabilities_read.md](./capabilities_read.md)         |
| `skills_list`           | `T04` | `M06`        | [skills_list.md](./skills_list.md)                     |
| `search_hybrid`         | `T05` | `M07`        | [search_hybrid.md](./search_hybrid.md)                 |
| `read_note`             | `T06` | `M06`        | [read_note.md](./read_note.md)                         |
| `list_vault`            | `T07` | `M06`        | [list_vault.md](./list_vault.md)                       |
| `get_outline`           | `T08` | `M06`        | [get_outline.md](./get_outline.md)                     |
| `get_backlinks`         | `T09` | `M06`        | [get_backlinks.md](./get_backlinks.md)                 |
| `get_context_packets`   | `T10` | `M03`        | [get_context_packets.md](./get_context_packets.md)     |
| `web_search`            | `T11` | `M07`        | [web_search.md](./web_search.md)                       |
| `web_fetch`             | `T12` | `M07`        | [web_fetch.md](./web_fetch.md)                         |
| `memory_read`           | `T13` | `M03`        | [memory_read.md](./memory_read.md)                     |
| `memory_write`          | `T14` | `M02`→`M03`  | [memory_write.md](./memory_write.md)                   |
| `insert_text_at_cursor` | `T15` | `M02`／`M08` | [insert_text_at_cursor.md](./insert_text_at_cursor.md) |
| `replace_selection`     | `T16` | `M02`／`M08` | [replace_selection.md](./replace_selection.md)         |

## 扩展能力目录（23 个）

`git_read_status`、`git_read_diff`、`git_read_log`、`secret_exists`、`fs_import_to_vault`、`fs_export`、`fs_read_authorized_folder`、`fs_write_authorized_export`、`doc_normalize_markdown`、`doc_extract_citations`、`git_write_commit`、`search_semantic`、`search_keyword`、`get_regulation`、`spawn_subagent`、`scheduled_task_create`、`scheduled_task_list`、`scheduled_task_delete`、`vault_create_note`、`vault_rename_move`、`vault_delete_to_trash`、`vault_asset_write`、`vault_version_list`（`T17`–`T39`，卡片同目录同名文件）。扩展工具只在任务需要、能力启用且授权满足时进入工具面。

## 不进入业务工具数的项目

| 项目                                 | 决定                                                                 |
| ------------------------------------ | -------------------------------------------------------------------- |
| `submit_final_answer`                | 保留 1 个条件式协议入口；普通聊天与 WebRequired 不强制使用           |
| `conclude_reasoning`                 | 从目标工具面退出，用自然答案结束；兼容期仍需识别旧记录，但不重放动作 |
| 原生 `web_search` 声明               | M4 内部协议能力，经 `C20` 管理；不作为第二个可任选的 Iris 搜索工具   |
| AnySearch／Tavily 的 search／extract | 作为 `web_search`／`web_fetch` 的后端适配，不重复暴露为旁路工具      |
| 外部 MCP 其他工具                    | 各自注册、验证与授权，计数 `N` 单独呈现，默认不接管受控写入          |

## Planned 占位（11 个，不暴露）

`fs_pick_file`、`fs_pick_folder`、`doc_convert`、`doc_ocr`、`doc_extract_pdf`、`doc_extract_table`、`doc_fix_links`、`clipboard_write`、`clipboard_read`、`secret_create_update`、`secret_read_plaintext`。

其中 `secret_read_plaintext` 是**明确禁止实现与暴露**的能力，不是等待开发的计划；其余占位也没有被此次定义升级为新增功能任务。

## 每轮暴露多少工具

采用「基础工具 + 获准领域工具 + 显式扩展」：普通助手对话 4 个基础工具；允许网页检索时 6 个；允许笔记库读取时 10 个（同时允许网页时 12 个）；编辑应用或管理任务只加所需写工具。**不存在「所有运行永久最多 5–7 个工具」的设计，也不存在「39 个每轮全部暴露」的设计。**

<!-- iris:end README-TOOLS -->

<!-- iris:object README-TOOLS kind=rules name=tools-index -->

### README-TOOLS 目录索引

本对象是目录索引容器：登记该目录各对象的身份与职责边界，并声明**它不承载任何对象的正式定义**——每个对象的定义在各自的文件里。维护规则见 [rules/governance.md](../rules/governance.md)；对象身份与标记规则见 [rules/objects.md](../rules/objects.md)。

<!-- iris:end README-TOOLS -->

<!-- iris:object P07 kind=rules name=planned-and-entry-points owner=P05 -->

### P07 协议入口与 Planned 占位登记

本对象登记**不进入 39 个业务工具数**的对象，避免它们被误当作可用能力。

**规则**：Planned 项不得出现在任何工具面；协议入口只在对应合同下暴露；第三方服务能力只能作为既有工具的后端适配，不得以别名形成第二条路径。

| 对象                                 | 身份                               | 处理                                                              |
| ------------------------------------ | ---------------------------------- | ----------------------------------------------------------------- |
| `submit_final_answer`                | 结构化提交入口，1 个条件式协议入口 | 只在需要结构化结果的合同中暴露；普通聊天与 WebRequired 不强制使用 |
| `conclude_reasoning`                 | 旧推理收束工具                     | 从目标工具面退出；兼容期识别旧记录，但不重放动作                  |
| 原生 `web_search` 声明               | 供应商协议能力                     | 由 `K12` 管理，不是第二个可任选的 Iris 搜索工具                   |
| AnySearch／Tavily 的 search／extract | 第三方服务能力                     | 作为 `web_search`／`web_fetch` 的后端适配，不作为旁路工具重复暴露 |
| 外部 MCP 工具                        | 动态数量 `N`                       | 各自注册、验证与授权；计数单独呈现，默认不接管受控写入            |

**Planned 占位（11 个，不暴露、不分配工具 ID）**：`fs_pick_file`、`fs_pick_folder`、`doc_convert`、`doc_ocr`、`doc_extract_pdf`、`doc_extract_table`、`doc_fix_links`、`clipboard_write`、`clipboard_read`、`secret_create_update`、`secret_read_plaintext`。

其中 `secret_read_plaintext` 是**明确禁止实现与暴露**的能力；其余占位也没有被此次定义升级为新增功能任务。Planned 项不因文档里出现名字而声明可用。

<!-- iris:end P07 -->
