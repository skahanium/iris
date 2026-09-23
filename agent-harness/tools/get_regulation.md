# get_regulation

<!-- iris:object T30 kind=rules file=true -->

`get_regulation` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T30 -->

<!-- iris:object T30 kind=tool name=get_regulation owner=M07 -->

## 用途与语义归属

**领域扩展：按法规名称与条号取条款原文**。语义归属为工具，责任模块 `M07`（检索与证据）。目标合同给出的取舍有两半：

1. 它是**领域扩展**（法规／条款这一领域），不是通用检索的替代；
2. 它**退出普通任务默认工具面**——即普通对话不应默认带着它。

当前实现只满足前半句：「直取精确条款」这一能力在位；「退出默认工具面」**没有找到对应机制**（见「暴露规则」，属目标合同与现状的差异）。

它与 `T05`／`T28`／`T29` 的关系是同一检索器上的另一条路线：本工具**只开 exact 层**（FTS、向量、图全部关闭），并且不解析自由查询语义——它把 `regulation_name` 与 `article` 拼成受控查询串后再交给精确匹配层。

## 参数与消费

目录 schema：`regulation_name`（string，**required**）、`article`（string，**required**）、`paragraph`（string，可选）。

- `regulation_name`、`article`：**被消费**。处理器拼接为 `《<regulation_name>》<article>`，交由 exact 层用正则 `《([^》]+)》\s*第?([一二三四五六七八九十百千万0-9]+)条` 解析（[retrieval_broker/exact.rs](../../src-tauri/src/ai_runtime/retrieval_broker/exact.rs)）。
- `paragraph`（款号）：**在当前实现中未被消费**。处理器只读前两个字段，`paragraph` 不参与查询构造；源码全文只在目录 schema 与 exact 层读取数据库列的语境中出现该名字，没有写入方读取调用参数。因此模型可以传 `paragraph` 而不会产生任何效果——与 `C16` 的「所有暴露参数必须消费或明确拒绝」方向相反，**属声明未消费**。这与 `G01` 记录的 `spawn_subagent` `context_hint`／`max_rounds` 是同类问题，但 `G01` 正文只点名了后者；本项**尚未登记为 `G*`**。
- 调用要求里的 `第X条` 形式（如「第六条」）在正则里可被 `第?` 前缀容忍；`article` 传「第六条」或「六」都能匹配，但传结构化条款号（如 `6.1`）无法生成有效查询。
- 查询串由工具构造，不经 `ctx` 的自由查询路径；`ctx.retrieval_scope` 仍然生效，结果再按 `DocumentCapability::Read` 过滤。

返回：`{ "regulation": <首个命中或 null>, "found": <bool> }`。注意处理器对 exact 层的请求上限是 3 条，但只返回**第一条**传给模型；`found` 反映的是过滤后是否非空。

## 授权

- 能力合同（`C16`）：`["vault.read"]`；`capability_affinity` 额外把本工具标记为 `ResearchSynthesis`（`get_regulation` 的定向分支），访问等级为 `ReadNoteSpan`。
- 权限原子（`C04`）：`Atom::VaultRead`，Risk = Low，`requires_confirmation = false` ⇒ `AutoAllowed`（与 `read_note`／`get_outline` 同原子）。
- 范围与策略：`ctx.retrieval_scope` 随请求冻结；exact 层自身还排除 `.classified` 路径（SQL 条件），再叠一层 `DocumentCapability::Read` 过滤。
- 联网：不需要，与联网开关无关。
- 子任务：在 `C15` 的 `child_tool_surface` 白名单内，可被继承（只能收窄）。
- 显式引用场景：`constrain_for_run_context` 的白名单不含本工具，因此在无范围的 `ExplicitReferences` 场景被隐藏。

## 副作用

**无文件写入、无数据库写入。** 单次只读连接内的 exact 层查询。

**必须如实记录的证据事实**：exact 层产出的 packet 把 `source_span` 设为 `None`、`content_hash` 设为空串。当 Run 尝试把成功结果登记为本地证据时，登记入口要求非空 `content_hash` 与合法的 `source_span`，否则丢弃该条（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `local_evidence_input_from_packet` 与 `register_local_tool_evidence` 的 `get_regulation` 分支）。因此按当前实现，**本工具的命中可能无法进入本次 Run 的本地证据账本**，模型仍能读到返回的条款文本，但它不能像 `read_note` 那样被登记为可引用证据。这属于**静态源码推论**，本轮未执行运行验证；是否另有补登路径**待核对**。

## 预算

- `execution_metadata = Some(LOCAL_NOTE_SPAN)`（`cost_class = "local"`、`output_policy = "bounded_note_span"`、`evidence_policy = "current_run_local"`）⇒ `ToolBudgetClass::Local`；当前预设 `max_local_tool_calls = 12`、`max_tool_calls = 24`。分类额度不是全局统一总数（`K06`）。
- **不占发现调用配额**：`is_discovery()` 判定明确把 `get_regulation` 排除在发现调用之外（[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)、[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs) 的 `discovery_calls_are_catalog_metadata_not_domain_routing`）。它与 `search_*` 的配额因此互不挤占。
- 目录 `max_results = Some(1)`，与处理器返回首条的写法一致；exact 层内部 SQL 上限 5、请求上限 3，**声明上限 1 与内部上限的关系待核对**（本卡不合并解释）。
- 派发包裹在 30 秒超时中；不在自动重试白名单内。

## 幂等

**读取操作，重复调用无副作用。** 同一 `regulation_name` + `article` 在索引不变时返回同一首条命中；索引或法规文件更新后结果可变，不声称可缓存。

循环层机械规则同其他只读工具：同指纹重复调用以 `tool_call_repeated` / `tool_call_already_succeeded` 表达。

## 取消

取消在派发入口生效（`ctx.ensure_run_active()`），已取消的 Run 不进入查询。

exact 层是单次同步只读查询，**无中途取消检查点**；不留下部分结果。

## 失败反馈

- **可恢复（参数类）**：缺 `regulation_name` 或 `article` 时，执行门返回 `{"error":"tool_arguments_invalid"}`，处理器层报 `missing regulation_name` / `missing article`。**不是字段级说明**（`G01` 目标）。
- **可恢复（查询无法解析）**：`article` 形式不符合正则要求时，exact 层返回空列表；工具返回 `found = false`、`regulation = null`。这是**有效观察**：模型应改用可解析的条号形式或改用 `T05`／`T29` 检索法规原文，而不是重复同一请求。
- **未命中不是失败**：`found = false` 不标 `success = false`，属如实缺口。
- **超时**：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- **用户侧表达**：单次失败不得表达为「模型能力降级」（`N17`）；条款内容是否支持结论由 `C23` 判断，本工具只提供命中文本。
- **归因提示**：`article` 参数未被消费的 `paragraph` 字段不会产生错误，也不会产生效果——模型看不到「参数被忽略」的反馈，这与 §3 M5 要求的「参数无效即字段级反馈」不一致，属**待核对**项。

## 暴露规则

- 目录 `default_enabled_without_skill = true`、`requires_confirmation = false`，因此它**进入**核心无技能只读名单（[tool_catalog_impl.rs](../../src-tauri/src/ai_runtime/tool_catalog_impl.rs) 的 `catalog_default_readonly_names` 过滤器）；该名单的单元测试只列了部分必需项，`get_regulation` 不在必需清单里，这只是测试覆盖范围的事实，不改变它被登记为默认只读的结论。
- **目标合同的差异（本卡核对所得）**：架构 §6.3 要求本工具「退出普通任务默认工具面」；基线 `2670739f` 中**没有**找到把它移出默认面的机制——工具面的收窄只有三条实际路径：能力快照（`C16`）、`constrain_for_run_context` 的显式引用白名单、以及 Web 工具规划（[tool_surface.rs](../../src-tauri/src/ai_runtime/tool_surface.rs) 只规划 web 工具）。普通 ToolLoop 只要含 `vault.read` 且不是无范围显式引用，本工具就在面上。该差异与 `G01`（工具目录目标与现状的差异）同属「目标目录状态尚未落地」，但 `G01` 正文未点名本项，故**尚未登记为 `G*`**，本卡标注为待核对事实。
- 隐藏条件：无范围的显式引用场景（见「授权」）。
- 子任务：白名单内可继承。
- Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（`get_regulation`：`ReadNoteSpan`、`requires_confirmation = false`、`max_results = Some(1)`、`LOCAL_NOTE_SPAN`）。
- 处理器与查询构造：[tool_dispatch/search.rs](../../src-tauri/src/ai_runtime/tool_dispatch/search.rs)（`regulation_lookup`）。
- exact 层：[retrieval_broker/exact.rs](../../src-tauri/src/ai_runtime/retrieval_broker/exact.rs)（`search_exact_regulation`、`RE_REGULATION_ARTICLE`、`regulation_index` 联表查询）。
- 能力、发现判定与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`vault.read`、`is_discovery()` 排除本工具、`ResearchSynthesis` affinity）。
- 权限画像与证据登记：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`VaultRead`）、[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`register_local_tool_evidence` 的 `get_regulation` 分支、`local_evidence_input_from_packet`）。
- 现有测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（非发现调用的显式断言）；[src-tauri/tests/agent_vault_tools.rs](../../src-tauri/tests/agent_vault_tools.rs)（不泄露 `.classified` 元数据）；[retrieval_broker/exact.rs](../../src-tauri/src/ai_runtime/retrieval_broker/exact.rs) 内 `exact_regulation_regex_matches`。**本次核对未发现覆盖 `paragraph` 消费或证据登记行为的测试**（事实陈述，非缺陷定性）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器、exact 层与正则匹配在位）。`verification.state = none`。

已知不一致（**待核对**，未登记为 `G*`）：`paragraph` 声明未消费；「退出普通任务默认工具面」无对应机制；exact 层 packet 缺少 `content_hash`／`source_span` 导致本地证据登记可能被丢弃；目录 `max_results = 1` 与内部请求上限 3／SQL 上限 5 的关系。未执行真实模型验收，不声明条款检索的准确性与覆盖率。

## 相关合同

- `K09` 联网授权与查询外发：本工具不涉外发；目录登记本工具消费 `K09`，该对应关系属登记事实，本卡不据此声称法规检索需要联网授权。
- `K10` 工具面版本与派发观察：暴露与派发状态的记录依据（含「退出默认面」的目标要求应按此合同落地）。
- `K14` 证据与来源事实：命中条款能否成为可引用证据，按该合同的来源身份与支持关系判断。
- `K06` 统一预算账本：`Local` 分类额度的唯一权威。
- 相邻工具：`T05` `search_hybrid`／`T29` `search_keyword`／`T28` `search_semantic`（通用检索路线）、`T06` `read_note`（读取法规原文全篇）、`T07` `list_vault`（找到法规文件路径）。
- 需求依据：本工具由架构定义 §6.3 的扩展目录定位给出（领域扩展、退出普通任务默认工具面）。
- 依据文档：[docs/agent-architecture.md](../../docs/agent-architecture.md) §3 M5（参数纠偏）、§6.3、§6.5（工具面按任务与授权形成）。

<!-- iris:end T30 -->
