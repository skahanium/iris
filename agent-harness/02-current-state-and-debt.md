# 02. 当前状态与技术债

**审计日期：2026-08-22。代码起点：`f19364d6`。**

本审计区分代码存在、当前测试证据、实例配置和目标设计。工具名、DTO 或 fixture 存在，不等于真实 Provider 已配置或用户请求能够完成。

## 1. 状态总览

| 能力                                         | 实现状态     | 处置方式 | 当前事实                                              |
| -------------------------------------------- | ------------ | -------- | ----------------------------------------------------- |
| Run 幂等、单航班与 durable finalization      | 已实现待复验 | 保留     | 已有唯一键、请求指纹、终态顺序和恢复测试              |
| Run-local UI 投影与迟到事件隔离              | 已实现待复验 | 保留     | reveal、presentation 和持久化恢复已有 Run 身份合同    |
| 冻结工具表面、执行门禁与 `capabilities_read` | 已实现待复验 | 保留     | 三者共享当前 Run 的允许工具事实                       |
| Web 权限、classified 隔离、查询污染门禁      | 已实现待复验 | 保留     | Web 开关、local-only 与 taint witness 已存在          |
| `system_time_now`                            | 已实现待复验 | 保留     | 本机事实不依赖 Web                                    |
| Run-local evidence 与来源组展示              | 已实现待复验 | 保留     | 当前 Run、未 retired、HTTPS 可定位证据才能成为候选    |
| `FreshResearchPlan`、`EvidenceGap`、查询去重 | 部分实现     | 重构     | 搜索轮次已受控，但抓取和修复预算未完整接线            |
| 模型驱动多轮搜索与深抓取                     | 部分实现     | 重构     | 首次确定性预取存在；部分路径仍隐藏后续 Web 能力       |
| 结构化领域 DTO、mapping、validator、renderer | 已实现待复验 | 保留     | 五类工具表面、11 个 operation 和 migration 072 已存在 |
| 真实结构化 Provider 配置                     | 延期         | 保留     | 2026-08-19 快照为 0/11 configured；执行前需重新审计   |
| Windows deterministic eval runner            | 已验证       | 保留     | 脚本 8/8 与 smoke 24/24 已于 2026-08-22 通过          |

## 2. 已形成的有用基线

### Run、恢复和 UI

- `client_request_id` 与 intake 指纹共同约束幂等重放；`session_key` 只限定活动顶层 Run。
- 最终消息、证据绑定、Run 终态和 `AnswerComplete` 已有明确提交顺序。
- sink 失败后从持久化状态恢复，不应重新执行工具或副作用。
- 新 Run 的 reveal、动画 frame 和 presentation 不应消费上一 Run 内容。

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

## 3. 当前研究路径的真实缺口

1. 严格 Web 路径先执行一次确定性预取；当 effort 为 Direct 时可以在一次预取后直接综合，未由统一证据充分性判断控制。
2. `constrain_domain_tool_surface` 对 News/结构化分支隐藏通用 `web_search`，使模型无法针对新缺口继续研究。
3. `run_tool_loop` 为后续模型调用构造 Web 配额时仍设置 `max_fetches: 0`；`ResearchBudget.max_fetches` 和 `max_repairs` 没有形成完整生产控制。
4. `domain_operation_is_executable` 使非 News operation 在缺少 binding 时于模型调用前失败，阻断了可由通用 Web 证据完成的研究型问题。
5. 当前分类主要按领域冻结 operation，尚未完整表达“精确槽位事实”和“研究型叙述”的不同完成合同。
6. `fresh_domains` 存在模块级 dead-code 许可，需要在替代路径落地时执行可达性清理。

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
