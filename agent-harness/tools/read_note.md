# read_note

<!-- iris:object T06 kind=rules file=true -->

`read_note` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T06 -->

<!-- iris:object T06 kind=tool name=read_note owner=M06 -->

## 用途与语义归属

按范围与版本读取本地正文：以 UTF-8 字节位置读取指定笔记的**有界** Markdown 内容，并用全文哈希安全续读。语义归属为工具（`M6` / 文档服务），责任模块 `M06`。

它是「命中之后读正文」的那一步：检索（`T05`）给片段，正文读取必须由本工具完成，且**搜索片段不得冒充已读正文**（`C21` 的同一条原则在本地读取上同样成立）。读取目标是 Run 冻结范围内的用户笔记，不是内部元数据。

## 参数与消费

| 参数           | 类型    | 声明边界                                   | 处理器行为                                                                           |
| -------------- | ------- | ------------------------------------------ | ------------------------------------------------------------------------------------ |
| `path`         | string  | **required**                               | 必填；缺失即 `missing path`                                                          |
| `start_byte`   | integer | `minimum: 0`，默认 0                       | 非 `u64` 或缺失时按 0；超出内容长度或**不是 UTF-8 字符边界**时明确拒绝               |
| `content_hash` | string  | 可选                                       | 与当前全文哈希不等时返回 `read_note content hash mismatch`（续读时拒绝已变化的笔记） |
| `max_chars`    | integer | `minimum: 1`，`maximum: 12000`，默认 12000 | `clamp(1, 12000)`，默认值同为 12000                                                  |

返回：`path`、`content`、`truncated`、`contentHash`（**整篇**内容哈希）、`sourceSpan {start, end}`、`nextStartByte`（仅在截断时给出）。

版本与续读语义（源码事实）：`contentHash` 与 `sourceSpan` 是**内部工具结果元数据**，用于让证据登记绑定「实际读到的来源」而不是可能被截断的模型载荷；`sourceSpan` 的单位是 UTF-8 字节，`max_chars` 的单位是字符——多字节字符不会被从中间切断，续读用 `nextStartByte`。参数边界与处理器钳制一致，未出现「声明与消费不一致」的参数。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.read"]`；`access_level = ReadNoteSpan`。
- 权限原子（`C04`）：`Atom::VaultRead`，Low，`supported = true`，`requires_confirmation = false`。
- 读取前的四道边界（按源码顺序）：① 文档策略对 `Discover`、`Read`、`SendToModel` 三项能力逐一判定（任一项 Deny → `agent_run_document_policy_denied`）；② `retrieval_scope` 必须允许该路径（否则 `agent_run_retrieval_scope_violation`）；③ 活跃技能作用域必须允许该路径；④ 路径必须是 vault 内合法用户笔记（拒绝绝对路径、`..` 穿越与 `.iris/`、`.classified` 等内部元数据，且对规范化后的路径再查一次别名）。
- 不受联网开关约束：本地读取不需要外发，联网关闭时仍可用。
- 子任务继承：授权随父级冻结能力快照收窄或继承；子运行不得用本工具读范围之外的笔记。

## 副作用

**无文件写入、无数据库写入。** 处理器只做一次 `std::fs::read_to_string` 与纯计算，不打开写句柄、不改索引、不落证据正文；返回的哈希用于证据身份，不改变文件。输出策略 `bounded_note_span`，证据策略 `current_run_local`（`LOCAL_NOTE_SPAN`）。

## 预算

- `cost_class = "local"`，归入 `ToolBudgetClass::Local`（当前预设 `max_local_tool_calls = 12`、`max_tool_calls = 24`；主循环分类额度，不是全局统一总数；数值权威在 `K06`）。
- 单次内容上限 12 000 字符（`DEFAULT_READ_NOTE_CHARS = MAX_READ_NOTE_CHARS = 12_000`）；`max_results` 未声明（`None`）。
- **不进发现调用配额**：`is_discovery()` 只含 `search_hybrid`、`search_semantic`、`search_keyword`、`list_vault`、`web_search`——本工具不受「每模型回合 2 个发现动作」限制，可连续续读。
- 派发包裹在 30 秒超时中；不在自动重试白名单。
- Durable Apply 场景另有目标限定核对口径：确认后最多以 `read_note` 做 2 次模型／4 次本地只读核对（见 `ARCHITECTURE.md`「Agent Run」节）；这是该场景的实现事实，不构成全局额度承诺。

## 幂等

按 `path` + `start_byte` + `max_chars` 重复读取是安全的，不产生副作用。**版本语义是显式的**：传入 `content_hash` 时，笔记在两次读取之间被修改会得到明确拒绝，而不是静默返回新内容——这是「重试不得造成重复变更」的读取侧对应物。注意 `contentHash` 覆盖整篇而非返回片段，故截断续读仍能检出漂移。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不打开文件。读取与解析是同步调用，无执行中途取消检查点：取消阻止后续调用，不中断已开始的读取。不留下部分结果——截断是**声明的**结果（`truncated` 与 `nextStartByte`），不是失败留下的残缺。

## 失败反馈

- 参数无效：缺少 `path` → `missing path`；`start_byte` 非法 → `read_note start_byte must be a valid UTF-8 byte boundary`；这些是**字段级**说明（与 `G01` 对字段级反馈的要求方向一致），但经派发层包装后统一表现为 `success: false` 加 `error` 文本，模型看到的粒度取决于该包装。
- 版本冲突：`read_note content hash mismatch` —— 明确、可恢复，模型应重新读取当前版本而不是假定内容未变。
- 边界拒绝：范围违规、技能作用域违规、文档策略拒绝、路径越界（绝对路径／穿越／内部元数据／别名）各自返回不同错误文本，均**不可通过重试同一参数恢复**，应改目标或告知用户。
- IO 失败与超时：`AppError` 文本与 `{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- 用户侧：不得因单次读取失败提示「模型能力降级」（`N17`）；恢复轨迹归 `C14`／`C26`／`C27`。

## 暴露规则

- `default_enabled_without_skill = true`。
- 目标工具面：对话允许笔记库读取时「基础加六个通用本地工具」之一；在无 Range 的显式引用场景中，本工具**是**允许清单成员（`read_note`、`get_outline`、`get_context_packets`、`get_backlinks`、`web_search`、`web_fetch`、`spawn_subagent`），即该场景下本地读取仍可用，vault 级搜索／列举被隐藏。
- 需要 Run 冻结能力含 `vault.read`。
- 子任务网页核验受限时（`Q07`：子工具清单缺 `web_fetch`），本工具仍是子运行可用的本地读取途径——**不能**据此断言子任务能读任何正文。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（第 106 行起，`Dispatchable`、`ReadNoteSpan`、`max_results = None`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/note.rs](../../src-tauri/src/ai_runtime/tool_dispatch/note.rs)（`read_note`、`ensure_note_model_read_allowed`）。
- 路径安全：[storage/paths.rs](../../src-tauri/src/storage/paths.rs)（`validate_user_note_relative_path`，含穿越、绝对路径、内部元数据与别名拒绝）。
- 范围与策略：[retrieval_scope.rs](../../src-tauri/src/ai_runtime/retrieval_scope.rs)、[policy_decision_engine.rs](../../src-tauri/src/ai_runtime/policy_decision_engine.rs)；作用域判定：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)。
- 哈希：[cas/hash.rs](../../src-tauri/src/cas/hash.rs)（`content_hash_str`）。
- 现有测试：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs)（穿越拒绝、`.iris` 元数据拒绝、合法路径、截断保留全文哈希与 span、UTF-8 边界续读与哈希校验、非字符边界拒绝与 `max_chars` 钳制、策略先于打开文件、作用域外拒绝、绝对路径拒绝）。
- 消费方：`insert_text_at_cursor`／`replace_selection`、`vault_delete_to_trash` 的 `base_content_hash` 说明引用「`read_note` 返回的整篇内容 hash」。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器、路径校验、版本校验与续读元数据均在位）。`verification.state = none`。

不确定处：**截断载荷与整篇哈希的配合**已在源码注释与测试中体现，但多轮续读拼接是否在所有调用方都被正确使用，本轮未逐调用方核对（**待核对**）。未执行真实模型验收，不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：读取范围来自冻结授权，不在运行中扩权；本卡只引用。
- `K14` 证据身份、来源与支持关系：`contentHash` 与 `sourceSpan` 服务该合同，正文仍不进入日志。
- `K10` 工具面版本与派发观察。
- 相邻工具：`T05` `search_hybrid`（命中）、`T08` `get_outline`（先看结构再定点读取）、`T15`／`T16`（写入侧绑定同一 `base_content_hash`）。
- 需求依据：`N01`、`N14`（承接已授权材料）；子任务正文核验缺口见 `Q07`。

<!-- iris:end T06 -->
