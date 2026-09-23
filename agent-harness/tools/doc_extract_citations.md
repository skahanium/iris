# doc_extract_citations

<!-- iris:object T26 kind=rules file=true -->

`doc_extract_citations` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T26 -->

<!-- iris:object T26 kind=tool name=doc_extract_citations owner=M07 -->

## 用途与语义归属

**抽取候选引用**：从调用方提供的文档文本里找出 HTTP(S) URL，作为引用候选返回。语义归属为工具（M7／M8），责任模块 `M07`（检索与证据）。目标合同的边界很明确：它只抽取候选，**不自动认证事实**——「有 URL 不等于事实成立」（架构 §6.3 与 §3 M7 的 `C22` 责任）。

因此本工具的输出不是证据：候选进入证据链必须经 `C22` 的记录与 `C23` 的核实条件检查（`K14`）。相关方 `M08` 的身份来自架构定义的语义归属标注，卡内不据此重复定义验证合同。

源码事实（当前实现的抽取范围）：把输入按空白切分，去掉词首尾的 `< > ( ) [ ] , . ; " '` 字符，保留以 `https://` 或 `http://` 开头的记号，按出现顺序去重。**不识别** Markdown 链接语法与参考文献格式，不解析标题、作者、日期等元数据，也不判断 URL 是否可访问——与「引用元数据」这一目录描述相比，当前实现是更窄的 URL 抽取启发式，属**已知事实**而非缺陷结论。

## 参数与消费

目录 schema：`content`（string，**required**）。处理器只读取该字段，**当前不存在声明未消费的参数**。

返回：`{ "type": "doc_extract_citations", "citations": [{"url": <url>}, …], "count": <n> }`。返回元素只有 `url` 一个字段；重复 URL 只出现一次。

参数校验分两层：`C17` 执行门按声明 schema 校验，缺失或类型不符返回 `{"error":"tool_arguments_invalid"}`（**不是字段级说明**）；处理器层取不到字符串时错误消息形如 `missing content`。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["document.citations"]`。该能力 ID 只在目录映射里出现，见本节末的暴露缺口。
- 权限原子（`C04`）：`Atom::DocExtractCitations`（`doc.extract_citations`），Risk = Medium，`supported = true`。
- 确认要求：目录项 `requires_confirmation = true`，且风险等级非 Low，因此决策为 `RequiresConfirmation`，必须经 `C05` 冻结确认后执行（`K05`）。
- 不读 Vault、不发起网络请求：输入完全来自参数 `content`，处理器不检查检索范围、文档策略，也不受联网开关影响。
- 子任务：本工具在 `C15` 的 `child_tool_surface` 白名单内，即**允许子运行继承**（前提是父级工具面本来就有它，且能力快照允许）；子任务继承只能收窄，不能借此扩权。

## 副作用

**无文件写入、无数据库写入、无网络请求。** 处理器是纯函数式的字符串扫描与去重（[boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs) 的 `doc_extract_citations_tool` 与 `extract_urls`）。

必须如实记录的当前事实：目录访问级别标为 `ToolAccessLevel::WriteCache`，这是「写缓存类」的等级标注，但处理器不写缓存、不落盘、不建索引。按目标合同，它属于文本辅助；因此当前分类与真实效果的对应关系**待核对**（与 `G01` 对 `doc_normalize_markdown` 的同类问题同源，但 `G01` 的正文只点名了 `doc_normalize_markdown`，本项尚未登记）。它同样因为是「写等级 + 需确认」而进入 `C05`→`C06` 的冻结变更路径，冻结目标记为 `application://tool/doc_extract_citations`。

抽取结果是否构成可引用的事实，不由本工具决定：候选仍需 `C22` 的来源身份与 `C23` 的核实条件。

## 预算

- `requires_confirmation = true` ⇒ 归入 `ToolBudgetClass::ConfirmedChange`（[capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs) 的 `budget_class`）。
- 当前预设 `max_confirmed_change_calls = 6`（三预设同值，[run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs)）；这是主循环分类额度，不是全局统一总数，数值权威在 `K06`。
- 目录 `max_results = None`：抽取条数**没有声明的结果上限**，实际只受输入长度与工具结果信封（`MAX_TOOL_RESULT_CHARS`）约束；具体上限**待核对**。
- 不在 30 秒派发超时的重试白名单内；`is_discovery()` 为 false，因此不占用「每回合 2 个发现调用」的配额。

## 幂等

**是确定性纯函数**：同一输入得到同一列表与同一顺序（去重按首次出现保留）。重复派发不追加任何状态，也不产生重复副作用。

循环层的通用去重规则仍然适用：同回合内同指纹调用超过 `MAX_REPEAT_CALLS` 返回 `tool_call_repeated`，已成功执行的同指纹调用返回 `tool_call_already_succeeded`。若输入文本变化（例如新增链接），它是**新的输入**，属于新的调用事实，不是重试。

## 取消

取消在派发入口生效：`dispatch_tool_inner` 首先执行 `ctx.ensure_run_active()`，已取消的 Run 不会进入处理器。

扫描是同步纯计算，**无执行中途取消检查点**；取消不留下部分结果（无写入、无中间产物）。

## 失败反馈

- **可恢复**：参数无效返回 `tool_arguments_invalid` 或 `missing content`，模型可修正后重提。字段级说明是 `G01` 的目标要求，当前只给稳定错误码。
- **空结果不是失败**：输入中没有 URL 时返回 `count = 0` 的有效观察，不得当作传输故障重复同一请求（架构 §3 M5 的可恢复错误表：空结果如实保留、由模型改用其他办法）。
- **需用户操作**：未获确认前不执行；确认身份绑定调用目标与内容（`K05`）。
- **不可恢复**：无外部依赖，除 Run 取消外没有服务类不可恢复失败。
- **用户侧表达**：单次工具错误不得表达为「模型能力降级」（`N17`）；候选引用是否成立由 `C25` 依据 `C23` 的核实结论呈现，不能在正文里把 URL 候选当成已验证来源。
- 需要区分「未抽取到候选」与「候选无法核实」：前者是本工具的空观察，后者是 `C22`／`C23` 的判断。

## 暴露规则

- 目录登记 `default_enabled_without_skill = false`；不在核心无技能只读名单内。
- 目标工具面：扩展目录项，任务需要、能力启用且授权满足时进入（架构 §6.3）。
- 实际门禁是 Run 冻结能力：需要 `document.citations`（`C16` 的 `is_authorized_by`）。
- **暴露缺口（本卡核对所得，尚未登记为 `G*`）**：基线 `2670739f` 中 `document.citations` 没有找到任何授予位置；`RunIntake` 加入的能力清单不含它（[run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs)）。因此按当前路径，本工具虽为 `Dispatchable`，**可能无法进入任何 Run 的工具面**。该结论限于静态核对，**待核对**是否存在其他授予入口。这与 `T25` 的同类缺口是不同能力 ID 上的两个独立事实，不合并成一条结论。
- 子工具面白名单包含本工具（见「授权」），但子运行能否真正看到它仍取决于同一能力快照，**待核对**。
- Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`doc_extract_citations`：`WriteCache`、`requires_confirmation = true`、`Dispatchable`、`max_results = None`）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`document.citations`、`budget_class`）。
- 权限画像：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`doc_extract_citations` 分支）。
- 处理器与抽取实现：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`doc_extract_citations_tool`、`extract_urls`）。
- 子工具面：[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs)（`child_tool_surface` 的 `CHILD_SAFE_TOOLS`）。
- 派发分支：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`DISPATCHABLE_TOOL_NAMES`、`dispatch_tool_inner`）。
- 现有测试：[src-tauri/tests/agent_permission_boundaries.rs](../../src-tauri/tests/agent_permission_boundaries.rs) 的 `catalog_declares_phase5_remaining_permission_boundaries` 只断言目录项存在且有权限原子；**本次核对未发现针对抽取行为本身的测试**（属事实陈述，不是缺陷定性）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器与 `extract_urls` 在位）。`verification.state = none`。

已知不一致：目录描述为「抽取引用元数据」而实现只抽 URL；访问级别 `WriteCache` 与「不写任何东西」的真实效果不对应（**待核对**，未登记为 `G*`）；能力授予缺口见「暴露规则」。未执行真实模型验收，不声明引用抽取质量或「候选可直接引用」。

## 相关合同

- `K14` 证据与来源事实：候选来源身份、内容范围与支持关系由该合同承载；本工具只产出候选。
- `K05` 授权范围与冻结确认：确认与冻结路径。
- `K06` 统一预算账本：`ConfirmedChange` 分类额度的唯一权威。
- 相邻工具：`T25` `doc_normalize_markdown`（同为文本辅助）、`T06` `read_note`（取得待抽取正文）、`T11` `web_search`／`T12` `web_fetch`（网页来源线索的真实获取路径）、`T10` `get_context_packets`（当前已获准材料索引）。
- 需求依据：`N06`（联网查询与后续核实）与 `N08`（两边发现的 URL 均可读取）间接要求引用可追踪；本工具的服务方向由架构 §6.3 的扩展目录定位给出。
- 依据文档：[docs/agent-architecture.md](../../docs/agent-architecture.md) §3 M7（`C22`）、§6.3、§6.4。

<!-- iris:end T26 -->
