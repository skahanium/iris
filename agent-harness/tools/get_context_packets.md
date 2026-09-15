# get_context_packets

<!-- iris:object T10 kind=rules file=true -->

`get_context_packets` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T10 -->

<!-- iris:object T10 kind=tool name=get_context_packets owner=M03 -->

## 用途与语义归属

返回当前已获准材料索引，不再另读私有内容。语义归属「工具（`M03`／`M07`）」，责任模块 `M03`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.2。

它读取**本轮上下文装配的产物**，不是检索工具：返回的是 `C07` 已经装入的材料索引视图（当前实现直接返回本轮冻结的材料包切片），因此它既不改写材料、也不扩大范围，更不代替 `search_hybrid` 之类的发现动作。

返回项的形状为 `ContextPacket`（含来源类型、路径、标题、标题路径、来源区间、内容哈希、摘录、检索理由、引用标签等字段）。当前切片里的 `source_type` 是本地笔记来源；网页证据有独立的对象形态，本卡片不宣称网页条目出现在本返回值中。

## 参数与消费

目录定义的 `input_schema` 为无参对象（`{"type": "object", "properties": {}}`），没有必填字段，也没有可省略的可选字段。任何「按范围、按类型、按条数过滤」的参数属于目标合同之外的扩展，本卡片不声明其存在。

返回体的稳定字段为 `packets`（材料包数组）与 `count`（条数），以 [tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs) 的 `get_context_packets` 分支为准。

## 授权

- `C04` 是唯一授权决定者。
- 目录声明的 `access_level` 为 `ReadIndex`（展示元数据），运行期准入能力是 `context.read`；访问级别不得扩大 Run 授权。
- `context.read` 只在请求已建立显式材料或范围时进入冻结授权（`Effect` 与材料状态共同决定），因此本工具不是「默认全开」的读取入口。
- 权限原子映射为 `VaultRead`，风险级别 `Low`；不需要确认（`requires_confirmation = false`）。

## 副作用

只读：返回已装配材料的索引视图，不读磁盘、不查索引、不发起检索、不写任何内容、不外发。审计记录限于受限摘要与条数，不把材料正文复制进审计或诊断（`K17`）。

## 预算

按工具的单一预算分类 `Local` 计账（目录未声明执行元数据，分类回落到访问级别对应的本地类），受 `max_local_tool_calls` 约束（Standard／Delegated／DurableApply 当前均为 12）；不占用网络与外部读取额度（`K06`）。

## 幂等

同一 Run 内重复调用返回同一份冻结材料视图：材料包在本轮装配时确定，工具本身不重新装配。它不改变后续调用的结果，也不改变已发布内容。

## 取消

派发入口在返回任何内容前检查 Run 取消标志，取消后返回 `Cancelled`。本工具无外部调用，不存在取消后的迟到结果。

## 失败反馈

本工具的正常路径不产生业务失败：无材料时返回空数组与 `count = 0`。需要与「没有材料」区分的情形：

| 情形                               | 要求                                                                                                           |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 本轮没有任何已装配材料             | 返回空清单，不得伪造条目                                                                                       |
| 工具不在本轮工具面                 | 派发前拒绝（`tool_not_in_run_surface`），反馈让模型重新提议                                                    |
| Run 已取消                         | 返回 `Cancelled`，不返回过期材料视图                                                                           |
| 材料引用的来源已被删除或授权已撤回 | **待核对**：当前实现按装配时冻结的切片返回内容哈希，是否及如何在返回值中表达「不可再核验」未在本卡片证据中确认 |

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `read` 组。是否进入某一轮工具面由 `C16` 按授权与上下文决定并记录工具面版本。

在受限子任务中，本工具属于 `CHILD_SAFE_TOOLS`（只读白名单），父级已获准时可按父级授权交集进入子 Run 工具面。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)。
- 派发实现：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs) 的 `get_context_packets` 分支（直接返回 `ctx.cold_start_packets`）。
- 数据来源：`NormalRunToolExecutor.cold_start_packets`（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)），由 [run_context.rs](../../src-tauri/src/ai_runtime/run_context.rs) 的 `local_retrieval_packets` 提供；字段结构见 [ai_types/mod.rs](../../src-tauri/src/ai_types/mod.rs) 的 `ContextPacket`。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，派发器有真实处理器。这只描述静态源码事实，不代表合同正确或用户任务可用。
- 验证结果：本卡片不声明任何验证结论；本轮为基线 `2670739f` 的静态核对，未执行新实验、未运行付费实网评测。
- 与目标合同的**表达范围差异**：当前返回的是本轮本地的装配材料切片；目标合同还覆盖 `M07` 侧经由 `C22` 保存的线索身份。网页线索是否进入本工具返回、以何字段表达，本卡片标为**待核对**（登记依据尚未落实到 `G*`／`Q*` 的具体条目，不虚构缺口编号）。

## 相关合同

本工具消费 `K07`（上下文装配与裁剪边界）。相关对象：`M03`（上下文与记忆）、`C07`（当前上下文装配）、`C22`（证据与出处管理）、`C16`（工具目录、工具面与技能接入）、`C17`（调用派发与观察）、`K14`（证据身份、来源与支持关系）、`K17`（审计事件与诊断查询）。

<!-- iris:end T10 -->
