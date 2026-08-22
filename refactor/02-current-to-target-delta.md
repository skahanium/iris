# 02. 从当前实现到最小目标形态

本次重构不推倒重建。目标是复用现有 Run、事件、工具目录、证据表、MCP 管理面和会话摘要，在真实断点处补齐契约。

本文区分三种语气：

- **已交付**：当前代码和测试可以直接证明；
- **当前缺口**：当前生产链存在且尚未收口；
- **目标调整**：尚未实现的目标；具体属于核心缺陷还是后续能力增强，以第 4 节和实施路线图标注为准，不能写入 `ARCHITECTURE.md` 作为既成事实。

## 1. 总体目标形态

运行时继续只使用一个代码层面的 `RunSituation` 投影，不新增持久化会话真相实体：

```text
ExecutionEnvelope（含 fresh fact 决策）
  + 已提交的会话消息
  + 当前 Run 事件与工具结果
  + 当前 Run 的 session_evidence
  + 冻结的 intake / tool surface / provider snapshot
  + 必要时的 conversation_summary
  = RunSituation（只读投影）
```

Executor 只消费这份冻结输入；UI 只消费带 Run 身份的持久化状态、可重放事件和当前 presentation。这样同时减少两类分叉：

1. 路由器认为可做、模型看到可做、执行器实际不能做；
2. 当前 Run 正在处理、UI 却把上一 Run 的正文当作当前内容。

## 2. Run 基线与前端内容所有权

### 已交付

- 内部 accept/retry 返回 `{ accepted, is_new }`，公共 IPC 签名保持不变；持久化 Run 的 `client_request_id` 具有唯一约束。
- 同 ID、同 intake 指纹返回原 Run 且 `is_new=false`；同 ID、不同指纹返回幂等冲突。只有 `is_new=true` 的首次接受者能启动执行器。
- `session_key` 只界定普通会话活动顶层 Run 的单航班范围；新的 ID 即使文本相同也属于新请求。
- 最终化顺序固定为：校验证据 → 持久化助手消息与绑定 → 持久化 Run 终态 → 发出 `AnswerComplete`。
- 事件 sink 失败只影响实时展示，窗口重新获得焦点或页面重新可见时从持久化快照/事件补齐。
- 用户拒绝统一映射为现有 `Cancelled(reason=user_rejected_change)`，拒绝后不执行变更、不生成最终消息。

### 当前缺口

- 前端 Run 身份已存在，但平滑回答 reveal 只返回字符串和 revealing 状态，没有把所属 `runId` 带给投影层。
- 新 Run 首次 render 发生在 reveal 清空 effect 之前，因而可能读到上一 Run 的 answer。
- `useAssistantConversationProjection` 在本轮 presentation 正常拥有内容但当前答案为空时，会回退到助手行已有正文，把短暂串入的旧答案固化到新 Run。
- `activateAccepted` 没有在切换身份时同步取消上一 Run 的待 flush animation frame 和 pending presentation events。

### 目标调整

- reveal、presentation、消息补丁和事件消费全部以相同 `runId` 为前提。
- 新 Run 被接受后，助手占位正文必须同步为空；effect 尚未运行也不能暴露旧 answer。
- 持久化内容回退只用于同一个 Run 的终态恢复，不用于活动 presentation 的空答案阶段。

## 3. Intake、时效分类与工具表面

### 已交付

- intake 做确定性分类、权限快照和输入规范化，结果随 Run 冻结。
- `ToolSurfacePlan` 已成为模型工具展示、`capabilities_read` 和执行器门禁的共同事实。
- Web 开关、模型能力、工具实现状态和确认策略共同裁剪表面，任何模型输出都不能扩大用户授权。

### 当前缺口

- `is_trusted_runtime_request` 使用有限的完整短语列表；“今天是几月几日”不匹配“今天几号/当前日期”，会被当成严格外部事实并执行 Web 搜索。
- `classify_time_sensitivity` 只有 `Current/None`，不能区分 runtime、天气、新闻、金融、影视、体育和通用 Web，也无法携带领域时间窗与地域要求。
- 现有测试只证明“近期电影”进入 ToolLoop/具备 Web capability，没有证明实际执行路径允许模型继续检索。
- `dispatch_required_web_verified_run` 对所有严格 Web 先执行一次原始查询预取，再隐藏 `web_search`；即使 envelope 原本为 ToolLoop，也会退化成单次检索后的自由文本生成。
- 查询未系统加入绝对当前日期、地域或需要补齐的证据字段，用户的“近期”会原样依赖服务商和模型理解。

### 目标调整

- `ExecutionEnvelope` 增加可向后兼容的 `FreshFactDomain` 和确定性时间窗/地域要求。
- runtime 日期时间直接走可信本机能力；五类外部领域进入相应稳定能力或有界 Web 研究。
- 明确单一事实可以在首批证据充分时一次完成；推荐、新闻、比较或首批证据不足时保留受预算约束的搜索/抓取能力。
- 工具表面继续失败关闭，但“已经预取”不能成为隐藏所有后续研究能力的充分理由。

## 4. 工具目录与执行

### 已交付

- 保留现有 `ToolCatalogEntry` 和 `ToolImplementationStatus`，没有第二套成熟度枚举。
- `cost_class`、`output_policy`、`evidence_policy` 已作为 `ToolCatalogEntry` 的可选执行元数据。
- 工具展示、参数校验、权限校验和 dispatch 共享目录事实；空 surface 确实禁止全部。
- `capabilities_read` 只返回当前 Run surface 与目录的交集。

### 当前缺口

- 模型目录只有 `system_time_now` 与通用 `web_search`，没有天气、新闻、金融、影视和体育的稳定 Iris 操作。
- 通用 MCP `external.read` 需要 Composer 逐 Run 显式选择，适合任意外部只读工具，不适合用户每次询问天气或行情时重复配置。
- 现有 provider mapping 只覆盖 `web.search/web.fetch`；领域服务商的输入、输出和稳定 operation 尚无冻结映射。

### 目标调整

- 本节属于核心缺陷收口后的能力增强，不阻塞第 3、5、7 节所述旧链路和前端投影修复。
- 增加五个稳定只读工具：`weather_lookup`、`news_lookup`、`finance_lookup`、`entertainment_lookup`、`sports_lookup`；本机时间继续复用 `system_time_now`。
- 工具返回附录 D 定义的规范化 DTO，不把任意服务商字段直接暴露给模型。
- 优先复用当前 `WebEvidenceBroker`；只有精确天气、行情等需要且存在已审核映射时才使用结构化 MCP provider。
- 复用现有 MCP binding/snapshot 增加可选领域 operation 与输出映射，不建立通用 REST 平台或第二套 provider registry。

## 5. Web 研究、证据与最终化

### 已交付

- 所有 Web 最终化路径会生成 `Exact`、`Normalized` 或 `SourceGroupFallback` 之一。
- Direct 无精确标记时会生成 fallback；UI 对缺失、未知或解析失败的绑定按来源组显示。
- 当前 Run、未失效、HTTPS 可定位的证据才能进入引用候选。
- 无结构化验证规则时不会晋升 VERIFIED。

### 当前缺口

- `SourceGroupFallback` 证明的是“检索过”，不能证明回答中的电影名、日期、价格或上映状态来自这些来源。
- `calibrated_structured_finalization_enabled` 当前没有任何真实模型/协议条目；严格 Web 生产路径因此普遍退回自由文本回答。
- `FinalAnswerIntegrity` 只检查结束原因和基本形态，不检查当前事实实体、数字、日期、地域和数据时点是否来自证据。
- 现有评测允许来源组满足引用要求，并大量使用固定模型响应，所以可以在结构流程全绿时仍然生成陈旧或虚构答案。

### 目标调整

- 除 `none/runtime` 外的外部当前事实结论必须通过结构化终局提交或 Harness 模板化事实渲染完成。
- 来源组可以继续展示检索范围，但不能满足严格当前事实的成功条件。
- 领域 DTO 做确定性的字段、单位、地域、时效和来源校验；通用 Web 候选做实体/数字/日期的可定位匹配。
- 首次证据不足时只允许在冻结预算内继续研究；一次修复后仍不足则返回稳定的证据不足错误，不生成猜测答案。

## 6. 地域、上下文与最小记忆

### 已交付

- 短会话使用已提交消息，超预算后使用读取时重新验证的 `conversation_summaries`。
- 当前目标不再永久取第一条用户消息。
- `ai_memories` 使用 global/vault 两级 scope，vault 优先于 global；写入、删除和清理都需要确认。

### 当前缺口

- 常用地点虽可用现有记忆保存，但时效分类和查询规划尚未定义稳定 key、地域粒度与缺失行为。
- 电影“近期能否在影院看到”、天气等问题若不带地域，现有 Web 查询不会确定地区，也不会先询问用户。

### 目标调整

- 常用地点只使用经确认的 global memory：`location.city`、`location.province`、`location.country`。
- 本轮明确地点优先于记忆；无城市时天气和附近影院必须询问。
- 允许放宽的领域按城市 → 省份 → 国家逐级查询，并在答案中说明最终使用的范围。
- 不通过 IP、Vault 内容、模型猜测或 provider 端点推断地点。

## 7. 前端投影

### 已交付

- 过程事件和状态组件由 `UnifiedAssistantPanel` 统一投影，没有独立 Harness 仪表盘。
- `capability_degraded`、等待确认、取消、失败和完成均可从持久化事实恢复。
- 工具诊断默认只展示脱敏摘要，不恢复原始工具参数。

### 当前缺口与目标

- 当前缺口不是会话数据串行错误，而是 React render/effect 边界上的跨 Run 临时状态泄漏。
- reveal 返回值必须显式携带 Run 身份；投影 key 也必须包含该身份，而不只包含 answer 长度。
- 活动 presentation 的空正文是合法状态，不能用旧内容填满。
- 需要组合 hook 的回归测试覆盖“旧 Run 完成 → 新 Run accepted → 新答案尚未到达”的真实顺序。
