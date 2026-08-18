# 附录 A：现状核对清单

本表是施工前的事实清单，不是长期设计承诺。状态含义：

- **Confirmed**：当前代码可直接确认缺口存在。
- **Partial**：机制已存在，但覆盖或生产接入不完整。
- **Stale**：旧审查结论已被当前代码事实推翻。
- **Unverified**：尚无足够代码或测试证据，不进入主干施工。

| ID        | 优先级 | 状态       | 当前事实                                                                             | 最小行动                                        |
| --------- | ------ | ---------- | ------------------------------------------------------------------------------------ | ----------------------------------------------- |
| RUN-001   | P0     | Confirmed  | retry 仓储层能识别已接受请求，但上层执行入口未保留 `is_new`，存在重复 spawn 风险     | 传递 `is_new`，并发测试钉住唯一执行权           |
| RUN-002   | P0     | Confirmed  | `AnswerComplete` 可早于最终消息/终态持久化完成                                       | 调整最终化顺序并做失败注入测试                  |
| RUN-003   | P1     | Partial    | Run 状态与事件已有基础，但 sink 失败后的恢复语义缺少统一契约                         | 用快照/重放恢复，禁止重做副作用                 |
| ROUTE-001 | P1     | Partial    | `ToolSurfacePlan` 已开始收敛时效/Web 判断，但能力读取和执行仍未完全消费同一冻结结果  | 完成现有 planner 接入，Executor 只消费冻结结果  |
| ROUTE-002 | P2     | Unverified | 没有证据表明新增 LLM Router 能改善当前可靠性                                         | 不进入首阶段；仅保留评测后立项可能              |
| TOOL-001  | P1     | Partial    | `ToolImplementationStatus` 已排除 Planned，但 `capabilities_read` 仍读取完整目录     | 改为读取 `ToolSurfacePlan` 的已解析工具列表     |
| TOOL-002  | P0     | Confirmed  | `spawn_subagent`、`conclude_reasoning` 的遗留权限映射到不相关能力                    | 修正映射；无产品用途则删除/不暴露               |
| TOOL-003  | P1     | Partial    | 目录、权限和执行校验均存在，但尚未形成单一不可绕过门禁                               | 收敛门禁并覆盖旁路负例                          |
| TOOL-004  | P2     | Confirmed  | 部分工具参数没有生产消费方                                                           | 删除死参数或实现真实语义                        |
| EVID-001  | P0     | Partial    | `SourceGroupFallback` 已存在且 ToolLoop 会生成；Direct 严格 Web 路径仍可能无 binding | Direct 也生成 fallback，所有 Web 最终化显式绑定 |
| EVID-002  | P0     | Partial    | UI 只有识别到 fallback 才显示“未逐段核验”，binding 缺失时降级不够诚实                | 缺失/未知 binding 统一 fail-safe                |
| EVID-003  | P1     | Confirmed  | 严格结构化 VERIFIED 规则没有形成有效覆盖                                             | 逐工具增加确定性规则；其余保持 uncalibrated     |
| EVID-004  | P2     | Partial    | `session_evidence` 已具时间、原 Run、失效和安全摘录字段                              | 在绑定校验中完整消费现有字段，不新增证据表      |
| SEC-001   | P0     | Confirmed  | 错误工具权限映射可能使授权语义失真                                                   | 与 TOOL-002 一并修复并加入拒绝型测试            |
| SEC-002   | P0     | Partial    | 已有 Web 权限与内容隔离机制，但本地检索到 Web 查询的数据流需端到端负例               | 建立统一数据流门禁和隐私回归测试                |
| CTX-001   | P1     | Partial    | 运行时上下文构造逻辑存在，但生产调用链接入不足                                       | 接成只读 `RunSituation`，不新增状态表           |
| CTX-002   | P1     | Confirmed  | 会话记忆兜底可能把第一条用户消息长期提升为目标                                       | 移除兜底，目标只来自当前请求/明确任务           |
| CTX-003   | P1     | Partial    | `conversation_summaries` 已存在，可支持压缩，但需补覆盖范围和失效语义                | 复用现表并增加失效/重建测试                     |
| MEM-001   | P2     | Partial    | `ai_memories` 已存在，但 key 冲突可能跨 scope 覆盖                                   | 调整为 `(scope, key)` 并提供 scope 清理         |
| MEM-002   | P2     | Confirmed  | 缺少“仅用户确认偏好可长期写入”的主干约束                                             | 限制写入入口、来源和预算                        |
| UI-001    | P1     | Partial    | `capability_degraded` 组件与测试存在，但生产面板接入不完整                           | 接入既有事件投影，不新增组件体系                |
| UI-002    | P2     | Confirmed  | 原始/无用工具参数会增加噪音与隐私风险                                                | 仅显示脱敏摘要和稳定错误码                      |
| EVAL-001  | P1     | Stale      | “只有 24 个评测场景”的旧基线已过期；当前代码已有 48-case 契约                        | 复用现有套件并维护稳定场景 ID                   |
| MEM-003   | —      | Stale      | “完全没有记忆基础设施”不准确：会话摘要和 `ai_memories` 均已存在                      | 只补最小安全语义，不重建记忆中心                |

## 核对纪律

- 实施某项前再次搜索其定义、调用方和测试；若事实变化，先更新本表状态。
- `Stale` 项不转化为任务。
- `Unverified` 项必须先获得复现、调用链或评测证据，不能凭架构偏好升级优先级。
- 优先级描述风险，不代表版本承诺。
