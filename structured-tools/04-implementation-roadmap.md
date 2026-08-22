# 04. 实施路线图

阶段编号只用于本专项，不对应产品版本。施工顺序固定，后阶段不能通过绕过前阶段来获得“可用”状态。

| 阶段 | 状态     | 目标                                             |
| ---- | -------- | ------------------------------------------------ |
| 0    | Baseline | 固定当前 0/11 实况、第三轮 intake 修复和旧库差异 |
| 1    | Planned  | 建立 operation-specific readiness                |
| 2    | Planned  | 对齐授权、工具表面、snapshot 和 dispatch         |
| 3    | Planned  | 增加真实 mapping 预览和管理中心矩阵              |
| 4    | Planned  | 接通健康排序、冻结备用和技术重试                 |
| 5    | Planned  | 为 11 个 operation 接入合规真实 Provider         |
| 6    | Planned  | 完成软件门禁、实例门禁和事实文档收口             |

## 施工前置协议（所有阶段必须遵守）

1. 每个阶段开始前，先阅读 [`07-provider-landing-and-decision-process.md`](07-provider-landing-and-decision-process.md) 和本文件中的决策门。
2. 任何标记为 **DECISION REQUIRED** 的项，施工 AI 必须先向设计者/用户提出具体选项并取得确认，**不得默认选择**。
3. 施工过程中发现新的歧义或方案缺口，立即停止，把问题记入 `02-gap-register.md` 或 `07` 的 Open Decisions，并询问设计者；不得绕过继续。
4. “讨论并完善方案”本身就是施工流程的一部分，不是额外步骤；未关闭的决策门不得进入下一阶段。
5. 每个决策门关闭时，在 `07` 的 Decision Log 中记录：选项、选择、理由、影响范围、确认人。

## 阶段 0：基线与停线条件

- 保留 `completed_conversation_accepts_a_third_current_movie_turn`，防止 0 候选再次阻断 Run。
- 导出当前 operation readiness，预期为 runtime 可用、外部 0/11 Configured。
- 固定开发库和安装库 schema 差异，不将旧实例误当新代码行为。
- 不修改现有 Run、evidence、Provider registry 和工具目录实体边界。

退出条件：当前事实可以由安全查询重复核实；文档不把 fixture 写成实例配置。

## 阶段 1：Readiness 真相

- 从现有 Provider、binding 和 health 派生当前支持矩阵内 operation 状态（目标 11 个）。
- 将“未配置”和“Provider 不健康”区分开。
- 让 resolver 实际消费健康事实，删除名为 healthy、实则只检查配置的并行判断。
- 暴露只读、安全的 readiness IPC。

**决策门 1（DECISION REQUIRED）**：operation 级 readiness/preview 如何持久化？必须从以下选项中确认：

- A. 允许一次最小 migration，在现有 binding 上增加 operation 级 preview/readiness 字段（推荐）；
- B. 允许一张小型派生表；
- C. 不新增 schema，但必须明确 `Ready` 的持久化依据是什么。

未关闭该决策门，不得开始实现 readiness IPC。

退出条件：同一函数为 resolver、intake、UI 提供一致状态；若选择 A/B，允许对应最小 schema 演进；若选择 C，必须能在文档中解释 Ready 的持久化来源。

## 阶段 2：Operation-specific 工具表面

- intake 根据分类和 operation 生成 `DomainToolGrant`。
- 只冻结匹配 operation 的 Provider snapshot。
- `ToolSurfacePlan` 和 `capabilities_read` 只显示当前 grants。
- dispatch 校验参数 operation、grant 和 snapshot 一致。

**决策门 2（DECISION REQUIRED）**：当某个 operation 未配置时，模型表面是“完全不显示该工具”，还是“显示工具但返回不可用”？本方案默认前者（不显示），但如果产品希望保留“可发现但不可用”的提示，必须由设计者确认，且不能影响 fail-closed 语义。

退出条件：天气 binding 永远不能使金融工具可调用；未配置娱乐能力不向模型广告；模型表面行为符合决策门 2 的确认结果。

## 阶段 3：真实预览和管理中心

- 保存 mapping 后，由用户显式触发一次受限真实调用。
- 原始输出只在内存中完成 mapping/validation。
- 预览成功才允许状态进入 Ready。
- 管理中心展示当前支持矩阵内全部 operation 的 readiness 矩阵和操作建议（目标 11 个）。

**决策门 3（DECISION REQUIRED）**：

1. 预览结果如何持久化？必须与决策门 1 一致。
2. 多 Provider 覆盖同一 operation 时，管理中心如何展示“部分覆盖”？是否引入 `PartialReady`/`CoverageLimited` 状态，还是只展示“Ready(覆盖范围见详情)”？必须由设计者确认。
3. 如果 mapping JSON 不足以表达覆盖范围，是否允许增加覆盖元数据字段？同样必须确认。

退出条件：字段、时效、来源或地域不合格的 Provider 无法显示为可用；覆盖范围展示符合决策门 3 的确认结果。

## 阶段 4：健康、备用和预算

- 使用现有持久化健康表和 circuit breaker。
- 每个 operation 冻结最多三个有序候选。
- 主 Provider 瞬时故障后只切换到冻结备用。
- 技术重试与业务补搜分开计数。
- 全部候选失败时返回领域不可用，不调用未冻结 Provider。

**决策门 4（DECISION REQUIRED）**：

1. 健康事实是 provider 级还是 operation 级？若 operation 级，必须与决策门 1 的持久化方案一致。
2. 三个候选的排序规则：用户优先 Provider、最近 Ready Provider、未熔断备用，这三者的优先级是否固定？若用户未设置优先 Provider，是否允许按健康分排序？必须确认。
3. 技术重试/备用切换的预算上限是否就是“一次业务调用最多三个候选 + 单 Provider 瞬时错误最多一次”？如不是，必须给出新上限。

退出条件：故障切换可恢复、可审计、不扩大业务搜索预算；排序和预算符合决策门 4 的确认结果。

## 阶段 5：真实 Provider 接入

按以下顺序逐 operation 接入：

1. `weather.current`、`weather.forecast`；
2. `news.search`；
3. `finance.quote`、`finance.metrics`、`finance.news`；
4. `entertainment.now_playing`、`entertainment.upcoming`、`entertainment.streaming`；
5. `sports.schedule`、`sports.score`。

**决策门 5（每个 operation 强制，DECISION REQUIRED）**：每个 operation 开始前，必须先完成 [`07-provider-landing-and-decision-process.md`](07-provider-landing-and-decision-process.md) 中的 Provider Decision Record（PDR），并由设计者确认。PDR 未确认，禁止进入 mapping、preview 或 production run。

PDR 至少回答：

- 候选 Provider 是 MCP 还是 REST？
- 如果只有 REST，是否允许新增 REST adapter？这属于架构决策，AI 不得自行开工。
- 该 Provider 覆盖该 operation 的哪些子范围？未覆盖范围怎么办？
- 是否存在多个 Provider 覆盖同一 operation？覆盖矩阵是什么？
- 字段 mapping 是否能完整映射到 DTO？不能时是否允许 schema/mapping 扩展？
- 真实预览用什么安全公开参数？预期返回什么字段？
- `news.search` 是否接受 WebFallback 作为完成形态，还是必须接入结构化 Provider？（见 07 OD-006）
- 没有合规 Provider 时，该 operation 是否从支持矩阵移除？

每个 operation 必须单独完成 discovery、只读审核、字段 mapping、真实预览、健康探测和 production Run 验收。同一 Provider 覆盖多个 operation 也不能合并验收。

若没有合规 Provider：

- 状态保持 Unconfigured；
- 模型 surface 不显示；
- 产品支持矩阵明确缩减；
- 不使用非官方网页解析或模型补字段凑数；
- 该缩减决定必须记录在 PDR 中，不能由 AI 悄悄跳过。

## 阶段 6：验收与收口

- 使用本地 contract fixture（MCP 或已确认的 REST adapter）运行当前支持矩阵内全部 operation 的 production matrix（目标 11 个）。
- 在当前实例执行当前支持矩阵内领域的真实场景清单。
- 验证安装版 059→072 升级和新装路径。
- 运行 Rust/前端质量门和 Agent eval。
- 按真实证据更新本文档状态。

**决策门 6（DECISION REQUIRED）**：最终支持矩阵必须由设计者确认。允许的收口方式有两种：

- A. 11 个 operation 全部 Ready/Operational；
- B. 部分 operation 因无合规 Provider 而明确从支持矩阵移除，其余 operation 完成真实验收。

不得出现“文档宣称 11/11，实际只有部分可用”的混合状态。若选择 B，必须同步更新 `README.md`、`ROADMAP.md`、管理中心和模型工具表面。

## 停线规则

任一情况出现时停止扩展并先修复：

- 工具表面报告不存在的 operation；
- 普通 Web mapping 被当作结构化 Provider；
- Provider 原始输出、参数或凭证进入事件/日志/UI；
- 运行中切换到未冻结 Provider；
- Provider 返回的 ID 被当作 Iris evidence ID；
- 非 News 领域在字段不足时用自由文本完成；
- 技术重试消耗或突破业务搜索预算；
- 新增第二套 Provider、健康或 evidence 真相源；
- fixture 通过后文档直接把实例标记为 Operational；
- 未关闭 DECISION REQUIRED 项继续施工。
