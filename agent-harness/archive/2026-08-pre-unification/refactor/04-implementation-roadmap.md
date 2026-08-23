# 04. 实施路线图

本路线图按风险和依赖排序，不绑定版本号或日期。版本安排仍由 `ROADMAP.md` 决定；本文阶段号只在 Harness 重构内部使用，不对应 `ROADMAP.md` 的 Agent Run 演进阶段编号。阶段 0–4 是已交付且必须持续回归的第一轮基线；阶段 5–7 独立收口当前用户可见缺陷；阶段 8 才扩展六类领域能力。状态只能由附录 A、B 和真实测试更新。

| 阶段 | 状态                | 状态依据                                            |
| ---- | ------------------- | --------------------------------------------------- |
| 0–4  | Baseline / Resolved | 附录 A 的 Resolved 事实与附录 B 的既有实证测试      |
| 5    | Resolved            | `UI-003` 目标测试已移入附录 B 实证表                |
| 6    | Partial             | 已有分类和预算骨架，但生产路由、地点传递和补充搜索持久化仍待收口 |
| 7    | Partial             | UI 隔离已完成；当前事实生产闭环和真实评测仍待收口             |
| 8    | Partial             | operation 级授权、冻结、DTO/evidence/终局生产矩阵已闭合；当前实例未配置真实结构化 Provider |

## 阶段 0–4：第一轮结构性收口（已交付回归基线）

### 阶段 0：基线复核与测试钉住

- 复核 Run、工具、证据、上下文和记忆的真实断点。
- 为 P0/P1 问题建立稳定 ID 与聚焦测试。
- 冻结 Agent capacity eval 场景 ID，不另造重复框架。

### 阶段 1：Run 与来源展示止血

- 收紧首次启动/retry 单航班和幂等。
- 使最终持久化早于 `AnswerComplete`。
- 补齐 Direct `SourceGroupFallback` 和 UI 来源组 fail-safe。
- 将拒绝确认收敛为 Cancelled。

### 阶段 2：工具事实与授权收敛

- 接通 `ToolSurfacePlan` 并使空 surface 真正禁止全部。
- 让 `capabilities_read`、目录和执行器共享当前 Run 工具事实。
- 将执行元数据收回工具目录，阻止伪造调用到达 dispatch。

### 阶段 3：证据、恢复与诊断安全

- Run、消息和事件成为 durable truth；sink 失败不改写终态。
- 当前 Run、未失效、HTTPS 可定位的证据才能成为引用候选。
- Direct/ToolLoop 均能诚实显示来源组。
- 工具事件和诊断不暴露原始参数、正文或凭证。

### 阶段 4：上下文压缩与最小记忆

- 摘要在读取时重新验证覆盖消息。
- 移除首条用户消息永久目标兜底。
- 沿用 071 完成 global/vault 记忆优先级及确认式删除/清理。

阶段 0–4 完成只代表结构性契约形成基线，不证明当前事实回答可靠，也不证明前端跨 Run 内容隔离。

## 阶段 5：跨 Run 回答投影隔离（已交付）

目标：立即消除新一轮处理期间显示上一轮正文的 P0 体验缺陷。

- 先以组合 hook 测试复现：旧 Run 已完成，新 Run accepted，但新答案尚未到达。
- reveal 返回 `runId`，投影层只消费与当前 presentation/Run 同身份的文本。
- 活动 presentation 的空 answer 保持为空，不回退到消息行内容。
- 新 Run 接受时取消上一 Run animation frame，清理 pending presentation events 和投影键。
- 增加迟到旧事件、终态恢复和 reduced-motion 回归测试。

退出条件：从新 Run 接受至首个新 answer delta 的每个 render 中，新助手行都不含上一 Run 字符；旧 Run 行保持原内容；恢复仍只读取同 Run 持久化事实。

施工计划：[`plans/01-turn-projection-isolation.md`](plans/01-turn-projection-isolation.md)。

## 阶段 6：时效分类与有界联网研究（已交付）

目标：先纠正通用路径，使没有专用领域服务商时也不会因为单次搜索和自由文本生成而猜测。

- 在 envelope 中冻结 `FreshFactDomain`、绝对时间窗和地域要求。
- 扩展可信 runtime 分类，覆盖“今天是几月几日”等等价表达。
- 将 query planner 与原始用户文本分离，加入绝对日期、语言和确认地点。
- 对推荐、新闻、比较和证据不足路径保留有界 Web ToolLoop；明确单一事实证据充分时提前停止。
- 用 `EvidenceGap` 约束后续搜索，固定搜索、抓取和结构化修复上限。
- 对当前事实启用结构化终局提交；协议不可用或证据不足时失败关闭。

退出条件：本机日期不联网；近期电影复现场景第一次即使用当前时间和地域，或明确证据不足；模型不能在来源组存在但事实未绑定时完成严格回答。

施工计划：[`plans/02-freshness-routing-and-grounding.md`](plans/02-freshness-routing-and-grounding.md)。

## 阶段 7：核心评测、文档与缺陷收口

目标：使当前用户可见问题能够独立结案，不以尚未建设的领域工具掩盖旧链路缺陷。

- 增加“日期 → 近期电影 → 质疑上映情况”的固定多轮评测。
- 增加能力诚实、地域缺失、陈旧数据、协议不支持和来源组不足负例。
- 更新附录 A 的状态；只有目标测试存在并实际通过后更新附录 B 的实证表。
- 更新 `ARCHITECTURE.md` 为阶段 5–6 最终已实现事实；实现前不得提前修改。
- 同步受影响的 IPC 参考、ROADMAP 和用户可见文案。
- 先运行受影响模块检查，PR 收口时再运行项目规定的完整门禁。

退出条件：`UI-003`、`ROUTE-003`、`WEB-001`、`EVID-005`、`EVAL-002` 全部有真实测试证据；固定复现场景通过或按契约失败关闭；文档不再把第一轮结构性完成、核心缺陷收口和领域能力增强混为一谈。

阶段 7 完成后可以宣称本次用户暴露的核心缺陷已经收口；不得同时宣称 `CAP-001` 或六类领域能力已经完成。

## 阶段 8：六类稳定能力与低配置服务商（Partial）

目标：补齐常见当前事实能力，而不建设万能数据平台。

- 保留 `system_time_now`，新增天气、新闻、金融、影视和体育五个稳定工具。
- 建立附录 D 的统一 DTO、字段/单位/时效/地域/来源验证。
- 默认使用现有 `WebEvidenceBroker`；存在经审核结构化 MCP 映射时优先使用。
- 在现有 MCP binding/snapshot 上增加 domain operation 与输出映射，保持配置漂移和撤销检查。
- 复用确认式 memory 保存常用地点；缺城市时天气和附近影院必须询问。
- 金融限定为事实、新闻、趋势和比较分析，不提供个性化买卖建议。

当前生产矩阵已证明 11 个 operation 均经过 intake → 单一 operation snapshot → 受限工具表面 → MCP DTO mapping/validator → Iris evidence ID → 结构化终局与终态恢复；无 binding 的非 News operation 在模型调用前失败关闭，News 保留 Web fallback。此证据只说明软件框架可信，**不**表示当前实例已经配置真实结构化 Provider，也不表示 Provider health 排序、自动 failover、REST adapter 或覆盖运营矩阵已经完成。

施工计划：[`plans/03-common-domain-capabilities.md`](plans/03-common-domain-capabilities.md)。

## 阶段兼容边界

- **公共 IPC**：优先扩展现有 DTO 的可选字段，不改 Tauri command 名称；任何签名变化同步 Rust、`src/types/ipc.ts`、`src/lib/ipc.ts`、`src/types/ai.ts` 和 IPC 文档。
- **数据库 migration**：阶段 5–7 不新增 migration；阶段 8 只允许为现有 MCP binding/snapshot 增加领域映射字段的 072 up/down migration，不新增 provider 或 evidence 平行表。
- **UI**：阶段 5 复用现有消息行、presentation 和过程组件；不新增第二消息列表或 Harness 仪表盘。
- **评测**：继续复用现有 Agent capacity eval 框架；新增稳定场景，不建立第二套 runner。Live API 只在用户显式授权后人工执行。
- **依赖**：不新增第三方依赖；如实现中发现现有依赖无法完成，必须先回到文档说明许可、替代方案和必要性。

## 阶段停线规则

任一阶段出现以下情况时暂停后续扩展，先修复或回退：

- 同一 `client_request_id`、同一请求指纹的重放再次取得执行权或产生重复副作用；
- 新 Run 任一 render 显示上一 Run 正文；
- UI 成功状态无法由数据库事实恢复；
- Web 关闭后仍发生外部请求；
- runtime 本机事实被不必要地外发查询；
- 来源组被当作严格事实支持或逐段精确引用；
- 当前事实缺少时点、地域或来源仍被输出为确定性结论；
- 研究循环超过冻结预算或重复无证据缺口的查询；
- 新增状态表、目录、provider registry 或缓存成为第二真相源；
- 为通过测试而放宽隐私、确认或证据约束。

未来能力提案不与上述阶段并行进入主干，详见附录 C。
