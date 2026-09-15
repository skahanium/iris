# search_hybrid

<!-- iris:object T05 kind=rules file=true -->

`search_hybrid` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T05 -->

<!-- iris:object T05 kind=tool name=search_hybrid owner=M07 -->

## 用途与语义归属

**默认本地检索入口**：在知识库中做混合检索（FTS + 向量 + 分数融合），返回与查询相关的片段包。语义归属为工具（`M7` 检索与证据），责任模块 `M07`；它服务的是「已授权本地材料里查什么」这一问题，不负责网页检索（`T11` `web_search`）也不负责正文阅读（`T06` `read_note`）。

FTS／向量／图是内部策略，不是模型可选参数：`C19` 保持范围、版本与排名语义，模型只提供查询语义。

## 参数与消费

目录 schema：`query`（string，**required**）。**schema 未声明 `limit`**。

源码事实（必须如实记录）：处理器仍会读取 `args["limit"]`——`(args["limit"].as_u64().unwrap_or(10) as usize).clamp(1, 8)`，即默认 10、单次上限 8 条。也就是说 `limit` 是一个**可被模型传入但未在目录中声明**的字段：当前参数校验只校验已声明字段且允许额外字段透传，因此它实际生效。这属于「声明与真实消费不一致」，与 `C16` 的「所有暴露参数必须消费或明确拒绝」方向相反却同样不干净；本卡标为**待核对**：应把它补进 schema，还是明确拒绝。它**尚未**登记进 `G01`。

其余输入来自派发上下文：`ctx.note_path`、`ctx.file_id`、`ctx.retrieval_scope`、`ctx.runtime_documents`。检索层选择为——`search_hybrid`：FTS + 向量，且**仅当 `note_path` 存在时**才启用图这一层；`exact`、`template` 层恒为关。

返回 `{ "results": <packets>, "count": <n>, "retrievalStatus": {...} }`。`retrievalStatus` 是有界的层状态摘要：`degraded` 布尔与最多 8 条 `{layer, status}`。源码注释说明：只有层名与类型化状态进入模型，**不发布自由文本诊断、模型标识或路径**——使模型能区分「空 vault」与「索引未就绪」，从而改变检索策略。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.read"]`。该能力只在 Run 建立时按显式材料或隐式 Vault 依赖加入，不是每个 Run 都有。
- 权限原子（`C04`）：`Atom::VaultSearch`，Low，`supported = true`，`requires_confirmation = false`。
- 范围约束：`ctx.retrieval_scope` 随请求冻结，检索不得悄悄扩大到全库；结果还会先按 `DocumentCapability::Read` 过滤，被文档策略拒绝的路径不进入模型观察。相关错误码：范围违规 `agent_run_retrieval_scope_violation`，策略拒绝 `agent_run_document_policy_denied`。
- 与联网开关的关系：本地检索本身不需要联网。但 `ContextMode::ExplicitReferences` 且检索范围不受限时，本工具被 `constrain_for_run_context` 从工具面隐藏，避免在有 @ 材料时仍做全库搜索。
- 子任务继承：子运行按同一合同获得（或收窄）检索能力，不能借子任务放宽范围。

## 副作用

**无文件写入、无数据库写入。** 只有一次只读连接内的检索：`state.db.with_read_conn(...)` → `hybrid_retrieve_with_diagnostics`。不写索引、不改配置、不落证据正文；证据登记由 `C22` 依据返回的身份元数据完成，正文仍留在受控会话或瞬时上下文。输出策略 `bounded_results`，证据策略 `current_run_local`（`LOCAL_RESULTS`）。

## 预算

- `cost_class = "local"`，归入 `ToolBudgetClass::Local`（当前预设 `max_local_tool_calls = 12`，`max_tool_calls = 24`；主循环分类额度，不是全局统一总数；数值权威在 `K06`）。
- 单次结果上限：处理器 `limit` 钳制在 1–8；目录 `max_results = Some(20)` 是登记元数据上限，与处理器的 8 不是同一个数——**两者关系待核对**（本卡不合并解释）。
- **发现调用配额**：本工具在 `is_discovery()` 名单内，每个模型回合最多执行 2 个发现调用；超出的独立发现动作返回 `{"status":"deferred_for_feedback","reason":"discovery_batch_limit"}`，且**不消耗额度**（`deferred_result` 标 `success: true`）。
- 派发包裹在 30 秒超时中；本工具不在自动重试白名单（白名单只含 `web_search`／`web_fetch`）。

## 幂等

读取操作，重复调用不产生副作用与重复计费。但**结果可漂移**：索引生成代、内容变更与范围版本都会改变命中；两次相同查询不保证相同结果。`K09` 关于「同一修订内成功的同一动作可以复用」的复用语义针对联网检索，本工具不据此声称可缓存——本地复用是否允许**待核对**。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不进入检索。检索在单次同步只读连接内完成，没有执行中途的取消检查点：取消只能阻止**新的**调用，不能中断已开始的查询。不留下部分结果。

## 失败反馈

- 参数无效：缺少 `query`（非字符串）→ `invalid_arguments_tool_result`，形式为 `{"error":"tool_arguments_invalid"}`——**不是字段级说明**，与 `G01` 要求的字段级反馈不一致。
- 检索失败：`dispatch_tool_with_retry` 对 `search_hybrid` 失败后会再派发一次 `search_keyword`（同参数），返回**关键词搜索的结果**。这是源码事实，须注意两点：模型看到的工具名是 `search_hybrid` 而实际执行的是 `search_keyword`；这与 `C17` 的观察身份、`K10` 的派发观察是否一致，**待核对**（未被 `G01` 覆盖）。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- 索引未就绪：不表现为工具失败，而是 `retrievalStatus.degraded = true` 加上对应层的 `status`；模型据此改变策略，而不是被误导为「库里没有」。
- 空结果如实返回（`count = 0`），属于有效观察，不得当作传输故障重复同一请求。
- 用户侧：单次失败不得表达为「模型能力降级」（`N17`）；恢复轨迹由 `C14`／`C26`／`C27` 记录。

## 暴露规则

- `default_enabled_without_skill = true`。
- 目标工具面：对话允许笔记库读取时「基础加六个通用本地工具」之一（该组合下 10 个；同时允许网页时 12 个）。
- 需要在 Run 冻结能力中含 `vault.read`；`effect = Answer` 之外的路径不自动获得本地检索。
- 例外：无 Range 的显式引用场景被隐藏（见「授权」）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（第 20 行起，`Dispatchable`、`ReadIndex`、`max_results = Some(20)`、`LOCAL_RESULTS`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"search_hybrid" | "search_semantic" | "search_keyword"` 分支；`dispatch_tool_with_retry` 的关键词回退）、[tool_dispatch/search.rs](../../src-tauri/src/ai_runtime/tool_dispatch/search.rs)（`hybrid_search`、`retrieval_status`）。
- 检索实现：[retrieval_broker.rs](../../src-tauri/src/ai_runtime/retrieval_broker.rs)（`hybrid_retrieve_with_diagnostics`、`RetrievalRequest`、`RetrievalLayers`）；范围：[retrieval_scope.rs](../../src-tauri/src/ai_runtime/retrieval_scope.rs)。
- 发现配额与延迟反馈：[agent_tool_loop.rs](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)（`is_discovery_call`、`deferred_result`、`MAX_DISCOVERY_CALLS_PER_MODEL_TURN`）。
- 权限与能力分类：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`VaultSearch`）、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`vault.read`、`is_discovery()`）。
- 现有测试：[tool_dispatch/search.rs](../../src-tauri/src/ai_runtime/tool_dispatch/search.rs) 内的 `retrieval_status` 测试（不泄露路径／模型标识、空层不算退化、层数有界）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器、诊断摘要与发现配额判定均在位）。`verification.state = none`。

已知不一致（见上，均**待核对**，尚未登记为 `G*`）：`limit` 未在 schema 声明却被消费；目录 `max_results = 20` 与处理器上限 8 的关系；`search_hybrid` 失败回退到 `search_keyword` 时的观察身份。未执行真实模型验收，不声明检索质量或合同正确。

## 相关合同

- `K09` 联网授权与查询外发：本地检索不涉外发，但同一 Run 内它与联网动作共享授权边界；本卡只引用。
- `K10` 工具面版本与派发观察：暴露与派发状态记录。
- `K06` 统一预算账本：Local 分类额度的唯一权威。
- 相邻工具：`T06` `read_note`（命中后的正文读取）、`T07` `list_vault`（同为发现调用）、`T28` `search_semantic`／`T29` `search_keyword`（显式扩展，不与默认混合搜索重复自动调用）。
- 需求依据：`N01`（基础对话可靠）、`N14`（长对话承接已授权材料）；默认本地入口的取舍见架构定义 §6.2。

<!-- iris:end T05 -->
