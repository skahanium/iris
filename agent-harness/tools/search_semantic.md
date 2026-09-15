# search_semantic

<!-- iris:object T28 kind=rules file=true -->

`search_semantic` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T28 -->

<!-- iris:object T28 kind=tool name=search_semantic owner=M07 -->

## 用途与语义归属

**显式语义检索扩展**：只启用向量这一层，用于「知道意思但不确定用词」的本地检索。语义归属为工具，责任模块 `M07`（检索与证据）。

目标合同给出了明确取舍：**不与默认混合搜索重复自动调用**——`T05` `search_hybrid` 是默认本地检索入口，本工具是**模型显式选择**的窄化路线；Host 不按关键词替模型决定何时改用它（架构 §6.3、§3 M5 的可恢复错误表）。

三者共用同一处理器：`search_semantic` 只开向量层，`T29` `search_keyword` 只开 FTS 层，`T05` 同时开 FTS + 向量（并在有 `note_path` 时附加图这一层）。因此「换工具」在当前实现里等价于「换检索层组合」，不是三条独立检索管线。

## 参数与消费

目录 schema：`query`（string，**required**）、`limit`（integer，default 10）。**两个参数都在 schema 中声明，且都被处理器消费**——这是本工具与 `T05` 的一个差别（`T05` 的 `limit` 未在 schema 声明）。

- `query`：缺失时执行门返回 `{"error":"tool_arguments_invalid"}`；处理器层缺失报 `missing query`。
- `limit`：处理器按 `(args["limit"].as_u64().unwrap_or(10) as usize).clamp(1, 8)` 处理——默认 10、**单次钳制在 1–8 条**，声明与消费一致。
- 其余输入来自派发上下文，不是模型参数：`ctx.note_path`、`ctx.file_id`、`ctx.retrieval_scope`、`ctx.runtime_documents`。检索范围随 Run 冻结，不得悄悄扩大到全库。
- 结果先按 `DocumentCapability::Read` 过滤，被文档策略拒绝的路径不进入模型观察。
- 目录 `max_results = Some(20)` 是登记元数据上限，与处理器的 8 不是同一个数——**两者关系待核对**（本卡不合并解释，与 `T05` 的同一问题保持一致的如实记录）。

返回：`{ "results": <packets>, "count": <n>, "retrievalStatus": { "degraded": <bool>, "layers": [{"layer","status"}] } }`。`retrievalStatus` 是有界层状态摘要（最多 8 条），只发布层名与类型化状态，不发布自由文本诊断、模型标识或路径——使模型能区分「空 vault」与「索引未就绪」。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.read"]`。该能力只在 Run 建立时按显式材料或隐式 Vault 依赖加入。
- 权限原子（`C04`）：`Atom::VaultSearch`，Risk = Low，`supported = true`，`requires_confirmation = false` ⇒ `AutoAllowed`。
- 范围约束：`ctx.retrieval_scope` 冻结随请求；范围违规记录安全错误码 `agent_run_retrieval_scope_violation`，文档策略拒绝记录 `agent_run_document_policy_denied`。
- 联网开关：本工具不需要联网，与 `web.search` 无关。
- 显式引用场景：`ContextMode::ExplicitReferences` 且检索范围不受限时，本工具被 `constrain_for_run_context` 从工具面隐藏（避免有 @ 材料时仍做全库检索）；有文件夹／标签范围时保留。
- 子任务继承：本工具在 `C15` 的 `child_tool_surface` 白名单内，子运行可按同一合同获得或收窄，不能借子任务放宽范围。

## 副作用

**无文件写入、无数据库写入、无索引变更。** 只有一次只读连接内的检索（`state.db.with_read_conn` → `hybrid_retrieve_with_diagnostics`）。

输出策略沿用 `LOCAL_RESULTS`（`bounded_results` + `current_run_local` 证据策略）：成功命中的来源元数据由 `C22` 依据返回身份登记进当次 Run 的证据账本，**正文仍留在受控会话或瞬时上下文中**，不复制进账本、审计或日志。

## 预算

- `cost_class = "local"` ⇒ `ToolBudgetClass::Local`；当前预设 `max_local_tool_calls = 12`（`max_tool_calls = 24`）。这是主循环分类额度，不是全局统一总数；数值权威在 `K06`。
- **发现调用配额**：本工具在 `is_discovery()` 名单内，**每个模型回合最多 2 个发现调用**；超出的独立发现动作返回 `{"status":"deferred_for_feedback","reason":"discovery_batch_limit"}`，且**不消耗额度**（延迟结果标 `success: true`）。因此同一回合内同时提议 `search_semantic` 与 `T29`／`T05` 会互相挤占该配额。
- 单次结果上限由处理器钳制为 1–8 条。
- 派发包裹在 30 秒超时中；不在自动重试白名单内（白名单只含 `web_search`／`web_fetch`）。
- 语义层不可用时不计为额度耗尽：`retrievalStatus.degraded = true` 与对应层状态是如实观察（例如 `index_not_ready`），模型据此改变策略。

## 幂等

**读取操作**：重复调用不产生副作用与重复变更。**但结果可漂移**：嵌入索引生成代、内容变更与范围版本都会改变命中，两次相同查询不保证相同结果，因此不据此声称可缓存。

循环层的机械规则：同回合内同指纹调用超过 `MAX_REPEAT_CALLS` 返回 `tool_call_repeated`；已成功执行的同指纹调用返回 `tool_call_already_succeeded`（除非该观察已被压缩，允许按 `K10` 的复用语义重放）。

## 取消

取消在派发入口生效：`ctx.ensure_run_active()` 在处理器之前执行，已取消的 Run 不进入检索。

检索在单次同步只读连接内完成，**没有执行中途的取消检查点**：取消只能阻止新的调用，不能中断已开始的查询；也不留下部分结果。

## 失败反馈

- **可恢复（参数类）**：缺少 `query` 返回 `tool_arguments_invalid`（执行门）或 `missing query`（处理器）。当前**不是字段级说明**，字段级反馈属 `G01` 目标。
- **可恢复（层不可用）**：向量层未就绪不是工具失败，而是 `retrievalStatus` 中该层的类型化状态与 `degraded = true`；模型应改用 `T29` 精确检索或先确认索引状态，而不是把「没有结果」当成「库里没有」。
- **空结果**：`count = 0` 是有效观察，不得当作传输故障重复同一请求（架构 §3 M5）。
- **超时**：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- **注意**：`search_hybrid` 失败会被重试路径回退到 `search_keyword`（观察身份与真实执行工具不一致的问题见 `T05`）；本工具与 `T29` **不在该回退分支内**——本工具失败不会被悄悄换成另一条检索路线。
- **用户侧表达**：单次失败不得表达为「模型能力降级」（`N17`）；恢复轨迹由 `C14`／`C26`／`C27` 记录。

## 暴露规则

- 目录 `default_enabled_without_skill = true`，且 `requires_confirmation = false`，因此进入「核心无技能只读工具」名单（[tool_catalog_impl.rs](../../src-tauri/src/ai_runtime/tool_catalog_impl.rs) 的 `catalog_default_readonly_names`，其单元测试把 `search_semantic` 列为必需项）。
- 目标工具面：对话允许笔记库读取时属「基础加六个通用本地工具」组合的一部分（该组合 10 个；同时允许网页时 12 个）。注意架构 §6.3 把本工具列为**扩展目录**项，因此「默认只读登记」不等于「每轮默认全开」——实际门禁仍是 Run 冻结能力含 `vault.read`（`C16` 的 `is_authorized_by`）。
- 隐藏条件：无范围的显式引用场景（见「授权」）。
- 子任务：白名单内可继承（收窄后）。
- Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（`search_semantic`：`ReadIndex`、`max_results = Some(20)`、`LOCAL_RESULTS`、`default_enabled_without_skill = true`）。
- 检索层选择与返回结构：[tool_dispatch/search.rs](../../src-tauri/src/ai_runtime/tool_dispatch/search.rs)（`hybrid_search` 的 `search_semantic` 分支、`retrieval_status`）。
- 派发分支：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"search_hybrid" | "search_semantic" | "search_keyword"`）。
- 检索实现与范围：[retrieval_broker.rs](../../src-tauri/src/ai_runtime/retrieval_broker.rs)、[retrieval_scope.rs](../../src-tauri/src/ai_runtime/retrieval_scope.rs)。
- 能力、发现配额与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`vault.read`、`is_discovery()`、`budget_class`）、[agent_tool_loop.rs](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)（`is_discovery_call`、`MAX_DISCOVERY_CALLS_PER_MODEL_TURN`、`deferred_result`）。
- 工具面收窄：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)（`constrain_for_run_context`）。
- 现有测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（默认只读名单、发现调用元数据、预算分类）；[src-tauri/tests/agent_vault_tools.rs](../../src-tauri/tests/agent_vault_tools.rs) 断言本工具不泄露涉密元数据。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，层选择、检索、状态摘要与证据登记路径在位）。`verification.state = none`。

已知不一致（**待核对**，未登记为 `G*`）：目录 `max_results = 20` 与处理器上限 8 的关系；语义层不可用时的降级表达只体现在 `retrievalStatus`，没有对应的模型可读建议。未执行真实模型验收，不声明语义检索质量或「显式使用不重复」。

## 相关合同

- `K09` 联网授权与查询外发：本工具不涉外发，但同一 Run 内与联网动作共享授权边界（目录登记本工具消费 `K09`，**该对应关系属登记事实，本卡不据此声称本地检索需要联网授权**）。
- `K10` 工具面版本与派发观察：暴露、延迟（`deferred_for_feedback`）与派发状态的记录依据。
- `K06` 统一预算账本：`Local` 分类额度与发现配额的唯一权威。
- 相邻工具：`T05` `search_hybrid`（默认混合入口）、`T29` `search_keyword`（精确层）、`T30` `get_regulation`（条号精确查原文）、`T06` `read_note`（命中后读正文）、`T07` `list_vault`（同为发现调用）。
- 需求依据：`N01`（基础对话可靠）、`N05`（连续对话承接前文）；扩展目录定位见架构 §6.3。

<!-- iris:end T28 -->
