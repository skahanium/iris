# list_vault

<!-- iris:object T07 kind=rules file=true -->

`list_vault` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T07 -->

<!-- iris:object T07 kind=tool name=list_vault owner=M06 -->

## 用途与语义归属

路径与标题导航，遵守检索范围：列出知识库中的笔记路径与标题，供模型先定位再定点读取。语义归属为工具（`M6` / 文档服务），责任模块 `M06`。

它是**导航**工具，不是检索工具：不返回正文、片段或相关性排名。目标选择后的正文读取由 `T06` `read_note` 完成，结构查看由 `T08` `get_outline` 完成。

## 参数与消费

| 参数     | 类型    | 声明边界                      | 处理器行为                                                  |
| -------- | ------- | ----------------------------- | ----------------------------------------------------------- |
| `prefix` | string  | 可选，默认 `""`（无前缀过滤） | 作为 `path LIKE '<prefix>%'` 的过滤条件；空串时不做路径过滤 |
| `limit`  | integer | 可选，默认 50                 | `clamp(1, 100)`                                             |

返回 `{ "files": [ { "path", "title" } ], "count": <n> }`。**不返回正文、大小、修改时间或标签。**

源码事实（过滤顺序，决定了「遵守检索范围」的真实含义）：

1. 数据库查询先取每个 `path` 的最新记录（`id IN (SELECT MAX(id) FROM files GROUP BY path)`），使同一路径的版本漂移不重复列出；
2. 固定排除 `.iris/%`、`.classified`、`.classified/%`；
3. 应用 `prefix` 过滤并按 `path` 升序；
4. 逐条经 `DocumentCapability::Discover` 判定与 `retrieval_scope.allows_path` 判定，两者都通过才计入，达到 `limit` 即停止。

`limit` 的截断发生在过滤**之后**，因此返回条数可能少于库中命中数，且不附带「还有更多」的标志——模型只能通过提高 `limit`（上限 100）或改用前缀来继续。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.read"]`；`access_level = ReadIndex`。
- 权限原子（`C04`）：`Atom::VaultSearch`，Low，`supported = true`，`requires_confirmation = false`。
- 范围约束：`retrieval_scope` 在本工具上是**逐条过滤**而非整体拒绝——列表可以只返回范围内的一部分路径，也可以为空。因此「列表为空」既可能是真的没有笔记，也可能是范围把全部结果滤掉；这一差别在模型侧不可见（**待核对**是否应给出区别）。
- 内部元数据与涉密路径由 SQL 固定排除，且过滤发生在读取标题之前，不依赖调用方自觉。
- 不受联网开关约束。子任务继承同一范围与能力，不能借列举放宽范围。

## 副作用

**无文件写入、无数据库写入。** 处理器在单次只读连接内执行一次查询与内存过滤，不写索引、不改配置、不落证据。输出策略 `bounded_results`，证据策略 `current_run_local`（`LOCAL_RESULTS`）。

（对照记录：MCP provider 列举的 list 写库（`F04`／`Q05`）已从源码消除，关闭仍等 `D04`；不在本工具路径上。）

## 预算

- `cost_class = "local"`，归入 `ToolBudgetClass::Local`（当前预设 `max_local_tool_calls = 12`、`max_tool_calls = 24`；主循环分类额度，不是全局统一总数；数值权威在 `K06`）。
- 单次上限：处理器 `limit` 钳制 1–100；目录 `max_results = Some(100)` 与之一致。
- **发现调用配额**：本工具在 `is_discovery()` 名单内，每模型回合最多 2 个发现调用；超出的独立发现动作返回 `{"status":"deferred_for_feedback","reason":"discovery_batch_limit"}` 且不消耗额度。
- 派发包裹在 30 秒超时中；不在自动重试白名单。

## 幂等

纯读取，重复调用不产生副作用。结果随文件集、索引状态与范围变化：新增／删除／重命名笔记后同参数结果不同。消费者不得把一次列表当作 vault 的稳定快照，也不得把空列表当作「范围不可用」。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不执行查询。查询与过滤在单次同步只读连接内完成，无执行中途取消检查点。不留下部分结果。

## 失败反馈

- 参数无效：`prefix`／`limit` 类型不符时由参数校验拦住，返回 `{"error":"tool_arguments_invalid"}`——**不是字段级说明**，与 `G01` 要求的字段级反馈不一致；缺失两个参数都属正常路径（有默认值）。
- vault 未就绪或数据库不可读：`AppResult` 错误经派发层成为失败的工具结果；源码注释明确不把「无法检查」解释为「没有结果」，但**当前实现通过 `?` 传播错误而非返回空列表**，这一点与注释一致。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- 空结果是有效观察，不得当作传输故障重复同一请求。
- 用户侧：单次失败不得表达为「模型能力降级」（`N17`）。

## 暴露规则

- `default_enabled_without_skill = true`。
- 目标工具面：对话允许笔记库读取时「基础加六个通用本地工具」之一。
- 需要在 Run 冻结能力中含 `vault.read`。
- 例外：无 Range 的显式引用场景中被隐藏（`constrain_for_run_context` 的允许清单不含本工具）——该场景下模型应留在 @ 材料上，而不是列举全库。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（第 126 行起，`Dispatchable`、`ReadIndex`、`max_results = Some(100)`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/note.rs](../../src-tauri/src/ai_runtime/tool_dispatch/note.rs)（`list_vault` 及其 SQL 过滤与逐条范围判定）。
- 范围实现：[retrieval_scope.rs](../../src-tauri/src/ai_runtime/retrieval_scope.rs)（`allows_path`）；策略：[policy_decision_engine.rs](../../src-tauri/src/ai_runtime/policy_decision_engine.rs)（`DocumentCapability::Discover`）。
- 发现配额：[agent_tool_loop.rs](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`is_discovery()`）。
- 现有测试：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs)（`list_vault_returns_only_paths_inside_the_immutable_run_scope`）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器、SQL 过滤与范围判定在位）。`verification.state = none`。

不确定处（**待核对**）：截断到 `limit` 时不报告「还有更多」，范围过滤造成的空结果与真实空库不可区分；两者是否需要显式表达，尚未登记为 `G*`。未执行真实模型验收，不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：范围来自冻结授权的逐条判定；本卡只引用。
- `K10` 工具面版本与派发观察。
- `K06` 统一预算账本：Local 分类额度的唯一权威。
- 相邻工具：`T05` `search_hybrid`（按内容找）、`T06` `read_note`（读正文）、`T08` `get_outline`（看结构）。
- 需求依据：`N01`（基础对话可靠）；「路径与标题导航、遵守检索范围」的取舍见架构定义 §6.2。

<!-- iris:end T07 -->
