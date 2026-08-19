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

## 阶段 0：基线与停线条件

- 保留 `completed_conversation_accepts_a_third_current_movie_turn`，防止 0 候选再次阻断 Run。
- 导出当前 operation readiness，预期为 runtime 可用、外部 0/11 Configured。
- 固定开发库和安装库 schema 差异，不将旧实例误当新代码行为。
- 不修改现有 Run、evidence、Provider registry 和工具目录实体边界。

退出条件：当前事实可以由安全查询重复核实；文档不把 fixture 写成实例配置。

## 阶段 1：Readiness 真相

- 从现有 Provider、binding 和 health 派生 11-operation 状态。
- 将“未配置”和“Provider 不健康”区分开。
- 让 resolver 实际消费健康事实，删除名为 healthy、实则只检查配置的并行判断。
- 暴露只读、安全的 readiness IPC。

退出条件：同一函数为 resolver、intake、UI 提供一致状态；不新增表。

## 阶段 2：Operation-specific 工具表面

- intake 根据分类和 operation 生成 `DomainToolGrant`。
- 只冻结匹配 operation 的 Provider snapshot。
- `ToolSurfacePlan` 和 `capabilities_read` 只显示当前 grants。
- dispatch 校验参数 operation、grant 和 snapshot 一致。

退出条件：天气 binding 永远不能使金融工具可调用；未配置娱乐能力不向模型广告。

## 阶段 3：真实预览和管理中心

- 保存 mapping 后，由用户显式触发一次受限真实调用。
- 原始输出只在内存中完成 mapping/validation。
- 预览成功才允许状态进入 Ready。
- 管理中心展示完整 11-operation readiness 矩阵和操作建议。

退出条件：字段、时效、来源或地域不合格的 Provider 无法显示为可用。

## 阶段 4：健康、备用和预算

- 使用现有持久化健康表和 circuit breaker。
- 每个 operation 冻结最多三个有序候选。
- 主 Provider 瞬时故障后只切换到冻结备用。
- 技术重试与业务补搜分开计数。
- 全部候选失败时返回领域不可用，不调用未冻结 Provider。

退出条件：故障切换可恢复、可审计、不扩大业务搜索预算。

## 阶段 5：真实 Provider 接入

按以下顺序逐 operation 接入：

1. `weather.current`、`weather.forecast`；
2. `news.search`；
3. `finance.quote`、`finance.metrics`、`finance.news`；
4. `entertainment.now_playing`、`entertainment.upcoming`、`entertainment.streaming`；
5. `sports.schedule`、`sports.score`。

每个 operation 必须单独完成 discovery、只读审核、字段 mapping、真实预览、健康探测和 production Run 验收。同一 Provider 覆盖多个 operation 也不能合并验收。

若没有合规 Provider：

- 状态保持 Unconfigured；
- 模型 surface 不显示；
- 产品支持矩阵明确缩减；
- 不使用非官方网页解析或模型补字段凑数。

## 阶段 6：验收与收口

- 使用本地 MCP contract fixture 运行 11-operation production matrix。
- 在当前实例执行五领域真实场景清单。
- 验证安装版 059→072 升级和新装路径。
- 运行 Rust/前端质量门和 Agent eval。
- 按真实证据更新本文档状态。

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
- fixture 通过后文档直接把实例标记为 Operational。
