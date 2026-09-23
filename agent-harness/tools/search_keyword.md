# search_keyword

<!-- iris:object T29 kind=rules file=true -->

`search_keyword` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T29 -->

<!-- iris:object T29 kind=tool name=search_keyword owner=M07 -->

## 用途与语义归属

**显式精确检索扩展**：只启用 FTS 层，用于术语、专名、代码符号等需要精确匹配的场景。语义归属为工具，责任模块 `M07`（检索与证据）。

它同时承担两个角色，这是目标合同之外必须如实记录的当前事实：

1. **模型显式选择的精确检索**（目标合同定位）；
2. **失败回退落点**：`search_hybrid` 派发失败时，重试路径会以同一参数再派发 `search_keyword`，并把结果作为原调用的返回（[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs) 的 `dispatch_tool_with_retry`）。此时模型看到的工具名是 `search_hybrid`、真实执行的却是本工具——**与 `C17` 的观察身份一致性待核对**（该问题在 `T05` 卡片中已记录，本卡只从本工具一侧确认该回退存在）。

## 参数与消费

目录 schema：`query`（string，**required**）、`limit`（integer，default 10）。**两个参数都已声明且都被处理器消费**。

- `query`：缺失时执行门返回 `{"error":"tool_arguments_invalid"}`；处理器层缺失报 `missing query`。
- `limit`：`(args["limit"].as_u64().unwrap_or(10) as usize).clamp(1, 8)`——默认 10、**单次钳制 1–8 条**。
- 上下文输入（非模型参数）：`ctx.note_path`、`ctx.file_id`、`ctx.retrieval_scope`、`ctx.runtime_documents`。注意**图这一层恒为关**（图层只在 `search_hybrid` 且有 `note_path` 时启用），因此本工具是纯 FTS 路线。
- 结果按 `DocumentCapability::Read` 过滤后才进入模型观察。
- 目录 `max_results = Some(20)` 与处理器上限 8 的关系**待核对**。

返回结构与 `T05`／`T28` 相同：`{ "results": <packets>, "count": <n>, "retrievalStatus": { "degraded": <bool>, "layers": [...] } }`；`retrievalStatus` 只发布层名与类型化状态。

## 授权

- 能力合同（`C16`）：`["vault.read"]`。
- 权限原子（`C04`）：`Atom::VaultSearch`，Risk = Low，`requires_confirmation = false` ⇒ `AutoAllowed`。
- 范围与策略：`ctx.retrieval_scope` 冻结；范围违规 `agent_run_retrieval_scope_violation`，文档策略拒绝 `agent_run_document_policy_denied`。
- 联网：不需要，与联网开关无关。
- 显式引用场景：与 `T05`／`T28` 一样，在无范围的 `ExplicitReferences` 场景被隐藏（[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs) 的 `constrain_for_run_context`）。
- 子任务：在 `C15` 的 `child_tool_surface` 白名单内，可被继承（只能收窄）。

## 副作用

**无文件写入、无数据库写入、无索引与配置变更。** 单次只读连接内的 FTS 检索。

输出与证据策略沿用 `LOCAL_RESULTS`：命中来源元数据由 `C22` 登记为当次 Run 的本地证据，正文不落账本与审计。

## 预算

- `cost_class = "local"` ⇒ `ToolBudgetClass::Local`（当前预设 `max_local_tool_calls = 12`、`max_tool_calls = 24`；分类额度不是全局统一总数，权威在 `K06`）。
- **发现调用配额**：在 `is_discovery()` 名单内，每模型回合最多 2 个发现调用；超出返回 `{"status":"deferred_for_feedback","reason":"discovery_batch_limit"}`，不消耗额度。由回退路径触发的本工具调用**不属于模型新提议**，是否占用该配额属**待核对**的实现细节（回退发生在 `C17` 侧，不经 `C16` 的提议计划）。
- 单次结果上限 1–8 条。
- 派发包裹在 30 秒超时中；不在自动重试白名单内。

## 幂等

**读取操作**：重复调用无副作用。结果可随索引与内容变化而漂移，不声称可缓存。

回退语义需要注意：`search_hybrid` 失败后自动重试本工具，**可能让「一次失败 + 一次回退」在预算与审计上表现为两次派发**；回退是否计入同一调用身份属**待核对**（`C17` 只申领并回报分类计数）。

循环层机械规则：同指纹调用超过 `MAX_REPEAT_CALLS` ⇒ `tool_call_repeated`；已成功同指纹调用 ⇒ `tool_call_already_succeeded`。

## 取消

取消在派发入口生效（`ctx.ensure_run_active()`）；已取消的 Run 不进入检索。

FTS 检索在单次同步只读连接内完成，**无中途取消检查点**，也不留下部分结果。回退路径中的第二次派发同样先经过 `ensure_run_active`，因此取消后不会继续回退执行。

## 失败反馈

- **可恢复（参数类）**：缺 `query` ⇒ `tool_arguments_invalid` 或 `missing query`（**非字段级**，属 `G01` 目标）。
- **空结果**：`count = 0` 是有效观察，应反馈缺口并让模型修改关键词或改查来源，不得当作传输故障重复同一请求。
- **FTS 层异常**：以 `retrievalStatus` 中该层的类型化状态与 `degraded = true` 表达，而不是笼统失败——这是「空 vault」与「索引未就绪」可区分的关键。
- **超时**：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- **用户侧表达**：单次失败不得表达为「模型能力降级」（`N17`）；真实失败仍须在诊断中保留（不得因有回退成功就抹掉原始失败，`G04`）。

## 暴露规则

- 目录 `default_enabled_without_skill = true`、`requires_confirmation = false`，进入核心无技能只读名单（[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs) 的必需要求包含 `search_keyword`）。
- 目标工具面：属「基础加六个通用本地工具」组合；但架构 §6.3 把本工具列为扩展目录项，因此实际门禁是 Run 冻结能力含 `vault.read`，而不是「登记为默认」本身。
- 隐藏条件：无范围的显式引用场景。
- 子任务：白名单内可继承。
- 回退暴露：即使本工具不在模型工具面上（例如被收窄隐藏），`search_hybrid` 失败仍会调用它——**这是宿主内部路径**，不因此让它出现在工具面；该行为与「Planned 不暴露」规则不冲突，但需要按 `C17` 的观察身份合同核对（**待核对**）。
- Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（`search_keyword`）。
- 层选择与返回结构：[tool_dispatch/search.rs](../../src-tauri/src/ai_runtime/tool_dispatch/search.rs)（`search_keyword` ⇒ `RetrievalLayers { fts: true, … }`）。
- 回退路径：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`dispatch_tool_with_retry`：`!result.success && tool_name == "search_hybrid"` ⇒ `dispatch_tool(…, "search_keyword", args)`）。
- 检索实现：[retrieval_broker.rs](../../src-tauri/src/ai_runtime/retrieval_broker.rs)；范围：[retrieval_scope.rs](../../src-tauri/src/ai_runtime/retrieval_scope.rs)。
- 能力、发现配额、预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)、[agent_tool_loop.rs](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)。
- 现有测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（预算分类显式断言 `search_keyword ⇒ Local`、发现元数据、默认只读名单）；[agent_tool_loop_tests.rs](../../src-tauri/src/ai_runtime/agent_tool_loop_tests.rs) 与 [run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的循环测试把本工具当作样例只读工具使用。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，FTS 层选择、检索实现、回退路径与证据登记在位）。`verification.state = none`。

已知不一致（**待核对**，未登记为 `G*`）：`search_hybrid` 失败回退到本工具时的观察身份与计数归属；目录 `max_results = 20` 与处理器 8 的关系；回退调用是否占用发现配额。未执行真实模型验收，不声明精确检索质量。

## 相关合同

- `K09` 联网授权与查询外发：本工具不涉外发；目录登记本工具消费 `K09`，该对应关系属登记事实，本卡不据此声称本地检索需要联网授权。
- `K10` 工具面版本与派发观察：派发状态、延迟与观察身份的记录依据；回退路径的身份一致性按此合同核对。
- `K06` 统一预算账本：`Local` 分类额度的唯一权威。
- 相邻工具：`T05` `search_hybrid`（默认混合入口与本工具的回退来源）、`T28` `search_semantic`（向量层）、`T30` `get_regulation`（条号精确查原文）、`T06` `read_note`、`T07` `list_vault`。
- 需求依据：`N01`、`N05`；扩展目录定位见架构 §6.3。

<!-- iris:end T29 -->
