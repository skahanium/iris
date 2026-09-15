# get_backlinks

<!-- iris:object T09 kind=rules file=true -->

`get_backlinks` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T09 -->

<!-- iris:object T09 kind=tool name=get_backlinks owner=M06 -->

## 用途与语义归属

读取指定笔记的反向链接。语义归属「工具／文档服务」，责任模块 `M06`；目录身份（通用／扩展、ID、责任模块）由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.2。

它是一次受限的**索引查询**，不是正文读取：回答「哪些笔记链接到目标笔记」并给出链接上下文，不返回被链接笔记的正文，也不重新解析 `.md` 原文。目标笔记路径必须落在本轮获准范围内，命中落在范围外的来源笔记时该项被丢弃，而不是把整库反链返回给模型。

本工具**不**判断链接语义是否成立，也不把反链当作证据；索引与 `.md` 原文不一致时的表达另有归属，本卡片声明为**待核对**（见「当前状态」）。

## 参数与消费

| 参数   | 类型   | 必填 | 消费事实                                                                    |
| ------ | ------ | ---- | --------------------------------------------------------------------------- |
| `path` | string | 是   | 作为目标笔记的 Vault 相对路径，先经路径允许性检查，再以精确匹配查询链接索引 |

字段名、必填性与 `additionalProperties` 约束以 [tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs) 的定义为准。目录未声明 `additionalProperties`，是否拒绝未知字段属于目标合同对参数的要求，本卡片不代替实现核对。

已知未消费／未落实项：`ToolCatalogEntry.max_results` 声明 `Some(50)`，但索引查询没有对应 `LIMIT`，查询结果数量在源头未受该值约束；按 `G01` 的口径，这属于「目录声明与真实执行不一致」的待核对范围（见「当前状态」）。

## 授权

- `C04` 是唯一授权决定者：本工具是能力接口，被提议不等于被允许。
- 目录声明的 `access_level` 为 `ReadIndex`（展示元数据），运行期固定的准入是能力 `vault.read`；二者不是同一事实，访问级别不得扩大 Run 授权。
- 权限原子映射为 `VaultSearch`，风险级别 `Low`。
- 不需要确认（`requires_confirmation = false`）。

## 副作用

只读：一次数据库查询，不写 `.md`、不写索引、不向外发送内容、不产生持久变更。审计侧只记录受限摘要，不把笔记正文复制进审计或诊断（`K17`）。

## 预算

按工具的单一预算分类 `Local` 计账，在 Standard／Delegated／DurableApply 三类预设下都受 `max_local_tool_calls` 约束（当前三者为 12）；不占用网络或外部读取额度。数值属于受控配置，调整需兼容旧 Run 存档（`K06`）。

## 幂等

同一 Run 内、同一 `path`、同一索引状态下重复调用返回同一结果，不累积状态、不改变后续调用结果。索引在 Run 期间被重建属于外部变化，本卡片不宣称此刻的结果与上一次相同。

## 取消

派发入口在做任何查询前检查 Run 取消标志（`ToolDispatchContext::ensure_run_active`），取消后返回 `Cancelled`，不开始副作用。查询为短事务，没有「迟到结果被当作新答案」的路径（`C03`）。

## 失败反馈

失败必须按可恢复错误反馈给模型，而不是一次即终止任务（`C14`、`N16`）：

| 情形                       | 反馈                                                                 |
| -------------------------- | -------------------------------------------------------------------- |
| 缺少 `path` 或字段类型不符 | 派发前判定参数无效，不派发（`tool_arguments_invalid`）               |
| 目标路径不在本轮获准范围   | 明确拒绝，不把范围外反链返回给模型                                   |
| Vault 不可用               | 明确错误，不返回空列表冒充「没有反链」                               |
| 索引缺失或陈旧             | **待核对**：宿主读路径不重建索引，是否零结果及如何表达未在证据中确认 |

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `read` 组。可被工具面按授权与上下文暴露；是否进入某一轮工具面由 `C16` 决定并记录工具面版本。Planned 不暴露与本项无关——本项是已实现的目录项。

在受限子任务中，本工具属于 `CHILD_SAFE_TOOLS`（只读白名单），父级已获准时可按父级授权交集进入子 Run 工具面。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（组装入口 [tool_catalog/groups.rs](../../src-tauri/src/ai_runtime/tool_catalog/groups.rs)）。
- 派发实现：[tool_dispatch/note.rs](../../src-tauri/src/ai_runtime/tool_dispatch/note.rs) 的 `get_backlinks`，经 [tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs) 的 `dispatch_tool_inner` 路由；工具本身无独立测试模块，目录契约由 [tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs) 覆盖。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八，正文变化需显式接受并分类）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，派发器有真实处理器。这只描述静态源码事实，既不表示合同正确，也不表示用户任务可用。
- 验证结果：本卡片不声明任何验证结论。`verification.state` 以登记表为准，本轮基线 `2670739f` 的核对未运行新实验。
- 待核对并已在 `G01` 登记的口径差异：`max_results = 50` 无对应执行约束。
- 索引陈旧时的表达能力上述「失败反馈」已标为待核对，未在证据中确认。

## 相关合同

本工具消费 `K05`（授权范围与冻结确认）。相关对象：`M06`（工具与连接器）、`C16`（工具目录、工具面与技能接入）、`C17`（调用派发与观察）、`C04`（授权与外发策略）、`K10`（工具面版本与派发观察）、`K17`（审计事件与诊断查询）、`N12`（授权覆盖范围）。

<!-- iris:end T09 -->
