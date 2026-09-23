# memory_read

<!-- iris:object T13 kind=rules file=true -->

`memory_read` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T13 -->

<!-- iris:object T13 kind=tool name=memory_read owner=M03 -->

## 用途与语义归属

读取获准作用域长期记忆。语义归属「工具」，责任模块 `M03`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.2。

长期记忆与聊天摘要分离：这里返回的是用户确认保存的长期条目（跨会话的偏好与事实），不是本轮会话的摘要或工作状态。已保存偏好可在获准作用域自动取用，不要求模型先调用本工具；`C08` 拥有的压缩覆盖与工作状态不由本工具返回。

长期事实不自动当作当前网页证据：本工具的输出不构成时效性证据（`K08`、`C22`）。

## 参数与消费

| 参数    | 类型    | 必填 | 默认 | 消费事实                                              |
| ------- | ------- | ---- | ---- | ----------------------------------------------------- |
| `key`   | string  | 否   | 无   | 精确读取某条记忆；非空时按 key 命中，不再走关键词过滤 |
| `query` | string  | 否   | 无   | 按关键词过滤（key 或内容的包含匹配）                  |
| `limit` | integer | 否   | 20   | 返回条数上限；实现把它夹在 1–50 之间                  |

`key` 非空时走精确路径并优先返回 vault 作用域条目；否则按关键词过滤、按作用域优先级与更新时间排序，并按 key 去重。作用域由入口决定而不是由参数选择：可读范围是「`global` 加上当前 Vault 的 `vault:<vault_id>`」，因此本工具读取的是**当前 Vault 已获准作用域**，不是任意作用域。

返回体字段为 `items`（每项含 `key`、`content`、`scope`、`source`、`updated_at`，其中 `scope` 只回报为 `global` 或 `vault`）与 `count`。

## 授权

- `C04` 是唯一授权决定者；运行期准入门槛是能力 `memory.read`。
- 目录声明的 `access_level = ReadProfile` 只是展示元数据，不得扩大 Run 授权。权限原子映射为 `AppStateRead`，风险级别 `Low`；不需要确认（`requires_confirmation = false`）。
- 即使读取本身不需要确认，目标合同仍要求长期条目只在获准作用域内可读（`N12`、`N14`）。

## 副作用

只读：一次数据库读取，不写条目、不写 `.md`、不修改作用域、不外发内容。审计只登记受限摘要（条数与分类计数），不把记忆正文复制进审计或诊断（`K17`、`AGENTS` §1.4）。

## 预算

按工具的单一预算分类 `Runtime` 计账（目录未声明执行元数据，分类回落到 `ReadProfile` 对应的运行时类），受 `max_runtime_tool_calls` 约束（三类预设当前均为 4）；不占用网络与外部读取额度（`K06`）。

## 幂等

同一 Run 内、条目未变时重复调用返回同一结果，不改变存储状态、不影响后续派发。它在 Run 期间读到的条目可能被同 Run 的 `memory_write` 改变，此时以新的存储状态为准，不保证与上一次读取相同。

## 取消

派发边界在查询前检查 Run 取消标志，取消后返回 `Cancelled`，不返回过期条目作为当前事实。

## 失败反馈

| 情形                             | 反馈                                                     |
| -------------------------------- | -------------------------------------------------------- |
| Run 已取消                       | 返回 `Cancelled`                                         |
| 参数类型不符（`limit` 非数字等） | 参数无效，不派发                                         |
| 无匹配条目                       | 返回空 `items` 与 `count = 0`，不伪造条目                |
| 作用域不可确定                   | 明确错误（存储侧的错误码），不静默退化成「返回全局条目」 |
| 读取失败                         | 按可恢复错误反馈，让模型改查或收窄范围（`C14`）          |

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `root` 组。它在目录中声明 `default_enabled_without_skill = false`，因此不属于「无 Skill 即可用的只读基础面」；是否进入某一轮工具面由 `C16` 按授权与上下文决定并记录工具面版本（`K10`）。

目标工具面规则是：获准使用长期记忆时按需要加 `memory_read`，自动注入已获准偏好不要求模型先调用本工具。在受限子任务中，本工具**当前不在**子 Run 的只读白名单内。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)。
- 派发实现：[tool_dispatch/memory.rs](../../src-tauri/src/ai_runtime/tool_dispatch/memory.rs) 的 `memory_read_tool`（`MAX_MEMORY_READ_LIMIT = 50`）；经 [tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs) 路由。
- 目录契约测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（`memory_read` 不需要确认的断言）。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，派发器有真实处理器。这只描述静态源码事实，不代表合同正确或用户任务可用。
- 产品行为尚未确定：长期记忆的**写入时机、有效期、冲突处理与用户管理**未确定（`Q15`）；本工具当前能读到的是既有独立存储中的条目，条目的来源、失效与作用域表达仍需按 `C09`／`K08` 补齐。
- 验证结果：本卡片不声明任何验证结论；本轮为基线 `2670739f` 的静态核对，未执行新实验、未运行付费实网评测。

## 相关合同

本工具消费 `K05`（授权范围与冻结确认）、`K08`（压缩覆盖与长期记忆条目）。相关对象：`M03`（上下文与记忆）、`C09`（长期记忆）、`C04`（授权与外发策略）、`C08`（会话压缩与工作状态）、`C16`（工具目录、工具面与技能接入）、`C17`（调用派发与观察）、`K10`（工具面版本与派发观察）、`N12`、`N14`、`Q15`。

<!-- iris:end T13 -->
