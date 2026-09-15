# doc_normalize_markdown

<!-- iris:object T25 kind=rules file=true -->

`doc_normalize_markdown` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T25 -->

<!-- iris:object T25 kind=tool name=doc_normalize_markdown owner=M08 -->

## 用途与语义归属

**文本转换辅助**：对调用方提供的 Markdown 文本做确定性规范化，把结果作为文本返回。语义归属为工具，责任模块 `M08`（验证与交付）；它在目标合同里的定位是**无文件写入的变换**，用于配合格式整理任务，**不能替代内容保持验证**——架构定义 §7.3 与 §6.3 明确：格式整理是否保持正文信息、顺序与链接，由 `C23` 的验证合同判定，不由本工具的存在或输出来证明。

源码事实（当前实现的规范化范围）：统一 `\r\n`／`\r` 为 `\n`；围栏代码块（``` 或 ~~~，缩进 ≤3 空格）内部原样保留；围栏之外的连续空行折叠为至多一个空行；每行去除行尾空白；删除开头空行；保证结尾恰好一个换行。**不做**列表编号重排、表格对齐、链接修复或引用抽取——`doc_fix_links`与`doc_extract_citations` 是各自独立的目标目录项。

## 参数与消费

目录 schema：`content`（string，**required**）。这是本工具唯一声明的参数，处理器唯一读取的字段也是它（`arg_str(args, "content")`），**当前不存在声明未消费的参数**。

返回：`{ "type": "doc_normalize_markdown", "markdown": <规范化文本>, "charCount": <字符数> }`。

参数校验分两层，反馈粒度不同：`C17` 执行门前用 [guardrails.rs](../../src-tauri/src/ai_runtime/guardrails.rs) 校验声明字段，缺 `content` 或类型不符时返回 `{"error":"tool_arguments_invalid"}`（**不是字段级说明**）；若校验通过而处理器取不到字符串，错误消息形如 `missing content`。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["document.transform"]`。该能力 ID 只在目录映射里出现，见本节末的暴露缺口。
- 权限原子（`C04`）：`Atom::DocNormalizeMarkdown`（`doc.normalize_markdown`），Risk = Medium，`supported = true`。
- 确认要求：当前目录项 `requires_confirmation = true`，且风险等级非 Low，因此决策为 `RequiresConfirmation`——调用必须走 `C05` 冻结变更与用户确认路径后才执行；授权范围内的已有有效授权不再反复索要（`K05`）。
- 不受联网开关约束：本工具不发起网络请求，`web.search` 与本工具无关。
- 不读 Vault：输入完全来自参数中的 `content`，处理器不调用 `ctx` 的检索范围与文档策略检查；因此它既不扩权也不受检索范围收窄。

## 副作用

**处理器自身无文件写入、无数据库写入。** 它只做纯字符串变换并返回文本（[boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs) 的 `doc_normalize_markdown_tool`）。

必须如实记录的当前事实：目录项把访问级别标为 `ToolAccessLevel::WriteMarkdown`，与「无文件写入的变换」不一致；架构定义 §6.3 与 `G01` 都把它登记为**权限分类错误**（原话：处理器返回转换文本，目录却标有写权限；目标应按真实效果修正分类）。因此当前实际行为是：不写文件，但因为是写等级且需确认，它仍然进入 `C05`→`C06` 的冻结变更路径，冻结目标被记为 `application://tool/doc_normalize_markdown`（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `frozen_relative_paths`），并计入「已确认变更」预算。读调用方文本、返回候选、再决定是否写入，属于调用方的责任。

## 预算

- `cost_class` 由目录的 `requires_confirmation = true` 归入 `ToolBudgetClass::ConfirmedChange`（[capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs) 的 `budget_class`）。
- 当前预设 `max_confirmed_change_calls = 6`（Standard／Delegated／DurableApply 三预设同值，见 [run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs)）。这是**主循环分类额度**，不是全局统一总数；数值权威在 `K06`，本卡不据此设新门槛。
- 单次派发包裹在 30 秒超时内（`DEFAULT_TOOL_DISPATCH_TIMEOUT`）；不在自动重试白名单（白名单只含 `web_search`／`web_fetch`）。
- 输入文本大小无独立声明上限；实际受模型请求与工具结果的整体信封约束，**具体上限待核对**。

## 幂等

**是纯函数**：同一输入文本得到同一输出（无随机、无时间、无外部读取）。重复派发不产生副作用与重复计费意义上的变更，也不会追加任何状态。

需要注意的是治理层语义而非本工具语义：确认路径按批次消耗，重复提议同一调用会被循环的重复调用判定拦截（`tool_call_repeated`，同回合内同指纹超过 `MAX_REPEAT_CALLS` 即拒绝），已成功执行的同指纹调用返回 `tool_call_already_succeeded`。

## 取消

取消信号在派发入口生效：`dispatch_tool_inner` 首先执行 `ctx.ensure_run_active()`，Run 已请求取消时返回 `Cancelled`，处理器不被调用。

变换本身是同步纯计算，**没有执行中途的取消检查点**，也无法中断已开始的字符串处理。取消不会留下部分结果（无写入、无中间文件）。

## 失败反馈

- **可恢复**：参数无效返回 `tool_arguments_invalid` 或 `missing content`，模型可补参重提。按架构 §3 M5，应给出**字段级**校验问题而不是笼统 error；当前实现只给稳定错误码，字段级说明属于 `G01` 的目标要求（与工具卡逐项一致性核对一并处理）。
- **需用户操作**：需要确认的调用在未获确认前不执行；确认绑定目标、内容、范围与版本（`K05`）。
- **不可恢复**：无网络、无外部依赖，因此没有服务类不可恢复失败；Run 已取消属于终局原因，不重试。
- **用户侧表达**：格式整理不通过校验时，应交付差异与不确定项、不自动写入、也不声称「内容完全未改」（架构 §7.3）；单次工具错误不得表达为「模型能力降级」（`N17`）。
- 内容保持的真实性由 `C23` 的验证合同负责：本工具的输出**不是**内容未变的证据，用规范化结果替代内容保持验证属于 `G01` 的目标缺口范围。

## 暴露规则

- 目录登记 `default_enabled_without_skill = false`；它不在「核心无技能只读工具」名单内（该名单是 `default_enabled_without_skill && !requires_confirmation` 的过滤结果）。
- 目标工具面：扩展目录项，「仅在任务需要、能力启用且授权满足时进入工具面」（架构 §6.3）；格式整理任务属架构 §7.3（`L04`）。
- 实际门禁是能力快照：只有 Run 冻结能力含 `document.transform` 时才可能暴露（`C16` 的 `is_authorized_by`）。
- **暴露缺口（本卡核对所得，尚未登记为 `G*`）**：在基线 `2670739f` 的实现里，`document.transform` 没有找到任何授予位置；`RunIntake` 只加入 `model.text`／`model.vision`／`runtime.read`／`note.propose_patch`／`note.apply_patch`／`context.read`／`vault.read`／`web.search`／`harness.child_run`／`external.read`（[run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs)）。因此按当前路径，本工具虽为 `Dispatchable`，却可能根本无法进入任何 Run 的工具面。**该结论限于静态核对，待核对**：是否另有未纳入本次核对的能力授予入口。
- 子任务能否继承**待核对**：本工具不在 `C15` 的 `child_tool_surface` 白名单内，因此子运行不会获得它；这与「目标是否允许子任务做格式整理」不是同一问题。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`doc_normalize_markdown`：`WriteMarkdown`、`requires_confirmation = true`、`Dispatchable`、`default_enabled_without_skill = false`、`max_results = None`、`execution_metadata = None`）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`document.transform`、`budget_class`）。
- 权限画像：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`permission_profile_for_tool` 的 `doc_normalize_markdown` 分支）。
- 处理器与规范化实现：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`doc_normalize_markdown_tool`、`normalize_markdown`、`markdown_fence_marker`）。
- 派发分支与暴露判定：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`DISPATCHABLE_TOOL_NAMES`、`dispatch_tool_inner`、`is_exposable_tool`）、[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)（`tools_for_authorized_capabilities`）。
- 现有测试：[src-tauri/tests/agent_permission_boundaries.rs](../../src-tauri/tests/agent_permission_boundaries.rs) 的 `doc_normalize_markdown_is_content_only`（断言 `"# Title\r\n\r\n\r\nBody   \r\n"` → `"# Title\n\nBody\n"`）；[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs) 内两个 `normalize_markdown` 单元测试（围栏内保留空行、围栏外折叠空行）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器与规范化函数在位并有单元测试）。`verification.state = none`。

已知不一致：目录 `WriteMarkdown`（写等级、需确认）与「无文件写入的变换」的目标分类不符，登记于 `G01`，**本卡不声明已修正**；「文本变换不能替代内容保持验证」是 `C23` 的目标合同，本轮未执行任何内容保持实验；能力授予缺口见「暴露规则」。未执行真实模型验收，不声明该工具在用户任务中可用。

## 相关合同

- `K16` 验证报告：格式整理的允许变化集合与逐项报告由该合同承载；本工具的输出只是候选。
- `K05` 授权范围与冻结确认：确认、目标绑定与版本复验；本工具因此进入变更路径。
- `K06` 统一预算账本：`ConfirmedChange` 分类额度的唯一权威。
- 相邻工具：`T15` `insert_text_at_cursor`／`T16` `replace_selection`（真正的受控写入动作）、`T26` `doc_extract_citations`（同为文本辅助）、`T06` `read_note`（取得待整理原文）。
- 需求依据：`N04`（Markdown 格式整理不修改实际内容，序号除外）；缺口依据 `G01`。
- 依据文档：[docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3、§7.3；[AGENT-REFORM-DISCUSSION-2026-09-14.md](../../AGENT-REFORM-DISCUSSION-2026-09-14.md) 第十八节（格式整理的内容保持要求）。

<!-- iris:end T25 -->
