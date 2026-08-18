# 附录 A：现状核对清单

本表是施工前的事实清单，不是长期设计承诺。状态含义：

- **Confirmed**：当前代码可直接确认缺口存在。
- **Partial**：机制已存在，但覆盖或生产接入不完整。
- **Resolved**：基线测试复现后已完成最小修复，并有回归测试通过。
- **Deferred**：保留安全门，但能力本身明确不进入本次完成标准。
- **Stale**：旧审查结论已被当前代码事实推翻。
- **Unverified**：尚无足够代码或测试证据，不进入主干施工。

| ID        | 优先级 | 状态       | 当前事实                                                                                                                                       | 最小行动                                                |
| --------- | ------ | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| RUN-001   | P0     | Resolved   | `client_request_id` 由数据库唯一约束；同 ID、同 intake 指纹返回原 Run 和 `is_new=false`，不同指纹冲突；`session_key` 只界定活动顶层 Run 单航班 | 保留 replay/conflict 与 concurrent start/retry 回归测试 |
| RUN-002   | P0     | Resolved   | 基线探针曾复现 `AnswerComplete` 早于持久化；现已改为 Completed 后再发终端展示                                                                  | 保留顺序回归测试并覆盖 sink 失败                        |
| RUN-003   | P1     | Resolved   | Run、消息和事件为唯一事实源；sink 失败不改写终态，前端在 focus/visible 时通过 `assistant_run_get` 恢复且不重执行                               | 保留终态 sink 故障与前端恢复测试                        |
| ROUTE-001 | P1     | Resolved   | `ToolSurfacePlan.tool_names` 已由生产编排器填充，`NormalRunToolExecutor` 只消费该冻结列表，`capabilities_read` 同步报告该列表                  | 保留 current-surface-only 与 executor surface 测试      |
| ROUTE-002 | P2     | Unverified | 没有证据表明新增 LLM Router 能改善当前可靠性                                                                                                   | 不进入首阶段；仅保留评测后立项可能                      |
| ROUTE-003 | P0     | Confirmed  | `is_trusted_runtime_request` 使用有限完整短语；“今天是几月几日”未命中可信 runtime，会进入严格 Web 路径                                         | 扩展确定性 runtime/fresh domain 分类并先写失败测试      |
| WEB-001   | P0     | Confirmed  | 严格 Web 生产路径先按原问题预取一次，再隐藏 `web_search`；ToolLoop 的近期电影问题实际退化为单次检索后的自由文本生成                            | 建立有界研究预算和 `EvidenceGap` 驱动的后续搜索         |
| TOOL-001  | P1     | Resolved   | `capabilities_read` 现通过 `ToolDispatchContext.available_tool_names` 只报告当前 Run 已冻结 surface                                            | 保留 current-surface-only 测试                          |
| TOOL-002  | P0     | Resolved   | 两个 harness 工具曾错误映射到 `vault.search`；现已使用独立 `harness.*` 权限原子                                                                | 保留权限映射回归测试                                    |
| TOOL-003  | P1     | Resolved   | 模型工具调用同时受 `AgentToolLoop.allowed_tools` 与 `NormalRunToolExecutor.allowed_tool_names` 表面门禁约束                                    | 保留 unexposed tool 负例与 executor surface 负例        |
| TOOL-004  | P2     | Resolved   | 遗留 `conclude_reasoning` 保持内部不可见且不进入模型 surface；未消费参数不暴露给模型                                                           | 保留 internal-only 不暴露测试与静态元数据负例           |
| EVID-001  | P0     | Resolved   | `SourceGroupFallback` 已存在且 ToolLoop 会生成；Direct 严格 Web 现已补齐 binding                                                               | 保留 Direct/ToolLoop 双路径回归测试                     |
| EVID-002  | P0     | Resolved   | UI 对 fallback 及缺失/未知 binding 明确显示“本次检索来源组/不表示已逐段核验”；精确 binding 仍走精确样式                                        | 保留来源组 fail-safe 测试                               |
| EVID-003  | P1     | Deferred   | 当前没有已注册的结构化业务校验规则；无规则时 fail-closed，通用自由文本语义校验不进入本轮                                                       | 引入真实结构化工具时再注册字段/单位/来源/时效规则       |
| EVID-004  | P2     | Resolved   | `list_current_run_web_citation_links` 只返回当前 Run、未 retired、HTTPS 可定位证据；foreign/retired 均被排除                                   | 保留 foreign/retired 负例测试                           |
| EVID-005  | P0     | Confirmed  | `SourceGroupFallback` 只能证明检索过来源；当前空结构化校准白名单使严格 Web 普遍自由文本收口，无法约束实体、数字、日期与地域                    | 当前事实强制结构化终局或模板化事实，证据不足失败关闭    |
| CAP-001   | P1     | Confirmed  | 目录只有 `system_time_now` 与通用 `web_search`，尚无天气、新闻、金融、影视和体育稳定 operation                                                 | 核心缺陷收口后按附录 D 独立建设与验收                   |
| SEC-001   | P0     | Resolved   | harness 工具已使用独立 `harness.*` 权限原子，不再继承 `vault.search`                                                                           | 保留权限映射拒绝型测试                                  |
| SEC-002   | P0     | Resolved   | 本地检索内容通过 `record_web_query_taint_witness` 与 Web 查询门禁阻止进入查询/URL/日志；已有端到端隐私负例                                     | 保留 taint/隐私负例回归测试                             |
| CTX-001   | P1     | Resolved   | `RunSituation = RunContext` 只读投影已在生产调用链使用，不新增第二状态表                                                                       | 保留 committed projection 测试                          |
| CTX-002   | P1     | Resolved   | 会话记忆兜底不再把第一条用户消息写入长期目标；仅使用最近用户消息或明确标记                                                                     | `first_user_message_is_not_permanent_goal` 已 GREEN     |
| CTX-003   | P1     | Resolved   | 摘要读取时复核覆盖消息的顺序、内容哈希和数量；范围内变化会刷新/清除，范围后新消息不使摘要失效                                                  | 保留读取时校验与近期窗口测试                            |
| MEM-001   | P2     | Resolved   | 沿用 071 `(scope,key)`；运行时只产生 global/vault scope，读取时 vault 优先，清理严格局限于指定 scope                                           | 保留优先级与 scope-local clear 测试                     |
| MEM-002   | P2     | Resolved   | `memory_write` 支持 upsert/delete_key/clear_scope，三者均经确认门并在 dispatch 时重新校验参数                                                  | 保留未确认不落库和各 operation 参数负例                 |
| UI-001    | P1     | Resolved   | `capability_degraded` 已接入 `UnifiedAssistantPanel` 生产事件投影，组件复用且不新增第二套体系                                                  | 保留事件 reducer 与生产面板 contract 测试               |
| UI-002    | P2     | Resolved   | 工具事件与审计只保存工具名、稳定码和受限摘要；哨兵测试证明原始参数、正文标记与凭证不进入事件诊断                                               | 保留原始参数哨兵负例                                    |
| UI-003    | P0     | Resolved   | reveal 返回 `runId` 并按身份门在 render 阶段隐藏异 Run answer；投影层只消费同 Run reveal，移除活动空答案回退；`activateAccepted` 切换前清理旧 frame/待 flush 事件 | 保留跨 Run 组合回归、终态恢复负例与迟到 frame 回归测试 |
| EVAL-001  | P1     | Stale      | “只有 24 个评测场景”的旧基线已过期；当前代码已有 48-case 契约                                                                                  | 复用现有套件并维护稳定场景 ID                           |
| EVAL-002  | P1     | Confirmed  | 现有 capacity eval 主要证明结构流程，允许来源组满足引用要求且使用固定响应，不能证明时效、地域和事实受证据支持                                  | 增加固定当前事实与跨 Run 多轮场景，不新建 runner        |
| MEM-003   | —      | Stale      | “完全没有记忆基础设施”不准确：会话摘要和 `ai_memories` 均已存在                                                                                | 只补最小安全语义，不重建记忆中心                        |

## 核对纪律

- 实施某项前再次搜索其定义、调用方和测试；若事实变化，先更新本表状态。
- `Stale` 项不转化为任务。
- `Unverified` 项必须先获得复现、调用链或评测证据，不能凭架构偏好升级优先级。
- 优先级描述风险，不代表版本承诺。

## 第一轮结构性收口结果（当前代码基线）

### Run 与恢复

- 首次启动与 retry 均在 SQLite immediate transaction 内执行幂等和会话单航班检查；`client_request_id` 是幂等键，同 ID、同指纹重放，不同指纹冲突；`session_key` 是活动 Run 单航班范围，不同 ID 遇活动 Run 返回 `agent_run_active_run_exists`。
- 涉密临时 Run 在进程内保存请求指纹与 `is_new`，不新增持久化历史。
- 拒绝确认以单事务写入 `rejected`、`Cancelled(user_rejected_change)` 与唯一 Cancelled 事件；重复拒绝 Noop，历史不一致状态恢复为 Cancelled。
- durable event 投递为 best-effort；sink 故障不覆盖已提交状态，前端只在 focus/visible 时快照恢复，不轮询、不重执行。

### 工具、证据与诊断

- 默认空工具表面真实表示禁止全部；主循环、子代理和严格 Web 预取显式注入表面，预取只允许 `web_search`。
- `capabilities_read` 返回目录与当前允许集合的交集；执行元数据由 `ToolCatalogEntry` 唯一持有。
- Direct 严格 Web 缺少精确标记时生成 `SourceGroupFallback`；UI 标为“本次检索来源组”，并声明不表示逐段核验。
- 诊断哨兵测试覆盖模型参数、笔记正文标记和凭证标记，事件与审计只保留安全摘要。
- EVID-003 为 Deferred；当前仅保留无注册规则不得进入 VERIFIED 的 fail-closed 门。

### 摘要与最小记忆

- `RunContext` 只读取验证后的摘要；覆盖范围内消息变化会刷新或清除，范围后的消息继续作为近期窗口。
- 记忆只使用 global 与当前 vault 两级稳定作用域；vault 覆盖 global，同名列表去重，对外不暴露 hash 或路径。
- `memory_write` 支持 upsert、delete_key、clear_scope；所有变更继续经过确认门，vault 不可解析时不降级到 global。

完整问题—测试对应关系见附录 B。上述状态只有在对应定向测试通过后才标记为 Resolved；最终全量门禁结果不在文档中预先声明。

## 当前核心缺陷待收口边界

- `ROUTE-003`、`WEB-001`、`EVID-005`、`EVAL-002` 仍是当前代码可复核的核心缺口。
- 在这些问题的目标测试尚不存在或尚未运行通过前，只能宣称第一轮结构性收口完成。
- `CAP-001` 是核心缺陷收口后的领域能力增强，不阻塞上述问题独立结案。
- 附录 D 和施工计划描述的是目标契约，不是 `ARCHITECTURE.md` 已实现事实。
