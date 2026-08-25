# 02. 当前状态与技术债

**初始审计：2026-08-23，代码起点：`116b3663`；当前复验：2026-08-26。**

本审计区分代码存在、当前测试证据、实例配置和目标设计。工具名、DTO 或 fixture 存在，不等于真实 Provider 已配置或用户请求能够完成。

## 1. 状态总览

| 能力                                         | 实现状态 | 处置方式 | 当前事实                                                |
| -------------------------------------------- | -------- | -------- | ------------------------------------------------------- |
| Run 幂等、单航班与 durable finalization      | 已验证   | 保留     | 唯一键、请求指纹、终态顺序和恢复测试通过                |
| Run-local UI 投影与迟到事件隔离              | 已验证   | 保留     | 用户单行历史可补建 assistant；无归属迟到事件保持隔离    |
| 补充输入与同 Run 恢复                        | 已验证   | 保留     | 对话内卡片；结构化字段直读；恢复重调度具有明确终态      |
| 冻结工具表面、执行门禁与 `capabilities_read` | 已验证   | 保留     | 三者共享当前 Run 的允许工具事实                         |
| Web 权限、classified 隔离、查询污染门禁      | 已验证   | 保留     | Web 开关、local-only 与 taint witness 已覆盖            |
| `system_time_now`                            | 已验证   | 保留     | 本机事实不依赖 Web                                      |
| Run-local evidence、来源协议与来源组展示     | 已验证   | 保留     | `ProvenancePolicy` 统一 W/E/L/M；展示标签不参与终态校验 |
| `FreshResearchPlan`、`EvidenceGap`、查询去重 | 已验证   | 保留     | 三档 profile、抓取/修复/续接/证据/deadline 均为生产控制 |
| 模型驱动多轮搜索与深抓取                     | 已验证   | 重构完成 | 单一 `web_search` 合同按 current-Run provenance 深抓取  |
| 结构化领域 DTO、mapping、validator、renderer | 已验证   | 保留     | 五类工具表面、11 个 operation 和 migration 072 已存在   |
| 真实结构化 Provider 配置                     | 延期     | 保留     | 2026-08-19 快照为 0/11 configured；执行前需重新审计     |
| Windows deterministic eval runner            | 已验证   | 保留     | 脚本 8/8、smoke 24/24、full 48/48 于 2026-08-24 通过    |

## 2. 已形成的有用基线

### Run、恢复和 UI

- `client_request_id` 与 intake 指纹共同约束幂等重放；`session_key` 只限定活动顶层 Run。
- 最终消息、证据绑定、Run 终态和 `AnswerComplete` 已有明确提交顺序。
- sink 失败后从持久化状态恢复，不应重新执行工具或副作用。
- 新 Run 的 reveal、动画 frame 和 presentation 不应消费上一 Run 内容。
- 会话历史只有同 Run 用户消息时，正文、过程和终态事件幂等补建唯一 assistant 投影；无同 Run 用户消息时仍忽略迟到事件。
- 补充信息只承担硬执行前置条件，并绑定原 Run 在对话内展示；提交后由 `preparing` 重调度，已校验字段直接进入 RunContext，不经自然语言二次识别。

### 工具、权限和证据

- 工具目录、模型表面和执行器均有冻结表面合同。
- `web_search` 是当前唯一模型可见网络工具；遗留 `web.fetch` 名称只作为内部兼容归一化存在。
- Web evidence 复用现有 ledger，并具有当前 Run 归属、退休状态和 HTTPS 定位约束。
- Provider 原始参数、输出和自带 evidence ID 不直接成为用户可见事实。

### 结构化当前事实框架

- `system_time_now` 以及 weather、news、finance、entertainment、sports 五类工具表面已注册。
- `DomainOperation` 定义 11 个 operation；`FreshDomainRecord`、output mapping、validator 和 Host renderer 已存在。
- migration 072 增加 operation 与 output mapping 兼容字段，不能回滚删除或要求用户重建数据库。
- 这些资产仍有价值，应作为精确事实快路径保留，而不是继续驱动 11/11 近期路线。

## 3. 本轮关闭的研究路径缺口与剩余边界

1. `ResearchBudget` 已同时控制搜索、实际成功抓取、一次修复、模型续接、evidence 与 deadline；schema 3 的恢复状态校验其冻结上限，不记录正文或 URL。
2. 模型仅见 `web_search`；URL 必须来自当前用户消息或 current-Run evidence ledger，不能借用历史、foreign 或 retired evidence。broker 最多并发 3 个允许的深抓取，并回报实际成功数。
3. 研究型天气、市场、新闻、体育和影视请求均可走统一 Web research；有 binding 的精确事实仍保留结构化快路径。缺 binding 不再是模型前的一刀切失败，News 也不再拥有架构特例。
4. 两次研究回合未新增有效 evidence 时终止；profile deadline 覆盖 provider 调用和工具执行，Web 关闭仍不会外发。
5. `fresh_domains` 的模块级 dead-code 许可和已不可达分支已删除。基于真实 provider/model 的性能 p50/p95 与 token 基线明确排除在本轮范围外；它必须保持延期，且不能由 fixture 代替。
6. 2026-08-25 生产复现发现旧当前事实门把 Run-local `W1` 同全局 evidence ID、会话 `[C1]` 混合比较；空数据库测试因 ID 偶合产生假通过。现已删除重复引用解释器，由 `ProvenancePolicy` 单独校验来源语法、Run 所有权与逐块覆盖；高位 ID 的 Web fallback、11-operation 表驱动链路和 Host `E{id}` 渲染均有命名回归。

## 4. 评测基线修复事实

本次先复现了两个 Windows 专属阻塞：

- Node 预检把 Windows `stat.mode` 当作 POSIX 权限位，导致 7 个脚本测试中 4 个失败。
- PowerShell 5 无法解析 UTF-8 无 BOM fixture 中的非 ASCII 字符，且 Windows fixture 与 POSIX fixture 的 dated snippet 合同发生漂移，导致 12 个 Web/Hybrid smoke 案例失败。

修复后：

- POSIX 继续检查 owner 与 `0o022`；Windows 保留 absolute path、realpath、非文件系统根、类型和 source DB/data root 绑定。
- PowerShell fixture 改为 ASCII-safe 合同数据，并与 POSIX fixture 一致提供固定 freshness label。
- `node --test scripts/agent-eval.test.mjs` 为 8/8；`npm run agent:eval:smoke` 为 24/24。

这只恢复了评测可信度，不证明未来的自适应研究目标已经实现。

## 5. 技术债处置原则

- 替代 `domain_operation_is_executable` 等旧分支时，同一阶段删除对应断言和文档，不保留长期兼容层。
- 接通已有预算字段；若某字段仍不参与控制，则删除字段而非继续标注“未来使用”。
- 复用 `web_search` 的 `query`、`gap`、`urls`，不增加第二网络工具。
- Provider 接入只在真实需求和已确认 PDR 下进行；不为未选择的 Provider 建 readiness、REST 或 failover 平台。
- 每个阶段同时报告新增、删除、延迟、token、证据量和质量变化；只增加代码而没有净简化的阶段不得结案。
