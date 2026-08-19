# 03. 目标架构

## 1. 核心原则

目标不是增加更多工具名，而是建立一条可证明的能力链：

```text
用户请求
  -> FreshFactDomain + DomainOperation
  -> OperationReadiness
  -> Operation-specific Tool Grant
  -> Run-frozen Provider Snapshots
  -> MCP Call / News Web Fallback
  -> Whitelist Output Mapping
  -> Domain DTO Validation
  -> Evidence Registration
  -> Host/Structured Finalization
  -> Durable Message + Recovery
```

任一环节失败都不能由模型补写事实。

## 2. Readiness 单一事实

readiness 从现有实体派生；是否允许最小 schema 演进（表/列/migration）由决策门 1 确认，未确认前不新增：

```rust
enum DomainReadinessState {
    Unconfigured,
    NeedsReview,
    Unhealthy,
    Ready,
    WebFallback,
}

struct DomainOperationReadiness {
    operation: DomainOperation,
    state: DomainReadinessState,
    eligible_provider_ids: Vec<String>,
    reason_code: Option<String>,
}
```

`Ready` 必须同时满足：

- Provider enabled；
- binding user_trusted；
- provider/binding hash 与审核时一致；
- input schema、argument mapping 和 output mapping 完整；
- 最近真实预览或调用成功；
- 未达到现有 circuit-breaker 连续失败阈值。

`news.search` 没有结构化 binding、但 Web 已授权和可用时，可以是 `WebFallback`。其他领域无 binding 时为 `Unconfigured`。

> 状态集合可能根据决策门 3 扩展（例如 `PartialReady`/`CoverageLimited`），扩展前必须由设计者确认。

### 2.1 Readiness 持久化：施工前必须定案的决策

当前代码只有 provider 级健康表（`web_evidence_provider_health`）和 binding 元数据，**没有 operation 级 preview/readiness 的持久化字段**。而 `Ready` 又要求“最近真实预览或调用成功”，这会形成矛盾：

- 如果严格“不新增表、不新增 migration”，`Ready` 只能退化为 provider 级健康猜测，无法证明某个 operation 本身可用；
- 如果允许一次最小 schema 演进，则可以在现有 binding 上增加 operation 级 preview/readiness 字段，或新增一张小型派生表。

**这是施工前必须由设计者拍板的 DECISION REQUIRED 项，AI 不得自行选择。** 默认建议是：允许一次最小 migration，在 `mcp_capability_bindings` 上增加 `last_preview_at`、`last_preview_status`、`last_preview_reason_code`、`preview_success_count` 等字段；若设计者选择“不新增表”，则必须明确 `Ready` 的持久化依据是什么，否则不能宣称 Ready。

## 3. Operation-specific 授权

当前粗粒度 `web.domain.read` 只保留为权限原子，不能再承担可用性证明。Run intake 产生：

```rust
struct DomainToolGrant {
    operation: DomainOperation,
    tool_name: &'static str,
    route: DomainReadinessState,
}
```

约束：

- `weather.current` grant 只能开放 `weather_lookup` 对应 operation。
- 天气 binding 不能授权金融、娱乐或体育。
- `capabilities_read` 只显示本 Run grants 与目录交集。
- dispatch 再次校验工具参数 operation 与 snapshot operation 一致。
- 伪造 operation 返回 `tool_not_in_run_surface`，不得到达 Provider。

## 4. Provider 准入

不预设商业供应商。Provider 通过现有 MCP discovery 和管理中心接入，每个 operation 独立满足；接入前必须完成 [`07-provider-landing-and-decision-process.md`](07-provider-landing-and-decision-process.md) 中的 Provider Decision Record 并经设计者确认：

1. 工具是明确只读，schema 闭合且参数预算可控。
2. 响应能用受限 JSON path 映射全部必需字段。
3. 来源是 HTTPS 可定位资源，时间、地域、单位和延迟完整。
4. 真实预览通过 DTO validator。
5. timeout、rate limit、schema drift 和空数据返回稳定安全码。
6. 原始参数、输出、transport 和凭证不进入诊断。

AnySearch/Tavily 只有在实际暴露独立领域工具并通过对应 transport 的准入（MCP discovery，或经 07 OD-002 确认的 REST adapter）并逐 operation 通过验收时，才能建立领域 binding。普通 `web_search/web_fetch` 映射不能直接升级。

## 5. 健康和候选冻结

每个 operation 最多冻结三个候选：

1. 用户明确优先 Provider；
2. 最近验证成功的 Ready Provider；
3. 未熔断的 Degraded 备用 Provider。

Run 接受后：

- 只允许在这组 snapshot 内重试或切换；
- 运行中发现的新 Provider 不进入本 Run；
- hash 漂移、禁用或撤销立即失败关闭；
- 一次业务调用最多尝试三个候选；
- 单 Provider 瞬时错误最多重试一次；
- 技术重试和备用切换不消耗业务补搜轮次。

### 5.1 多 Provider 覆盖同一 operation：必须显式建模覆盖范围

同一个 operation 可能由多个 Provider 各覆盖一部分，例如：

- `sports.schedule`：Provider A 只覆盖 NBA，Provider B 只覆盖英超；
- `entertainment.streaming`：Provider A 只覆盖美区，Provider B 只覆盖中国区；
- `finance.quote`：Provider A 覆盖美股，Provider B 覆盖 A 股/港股。

**禁止把“Provider 存在”直接当作“operation 完整可用”。** 施工前必须完成覆盖矩阵：

| Operation         | 覆盖维度  | Provider A 覆盖 | Provider B 覆盖 | 未覆盖范围              |
| ----------------- | --------- | --------------- | --------------- | ----------------------- |
| `sports.schedule` | 联赛/地区 | NBA             | 英超            | 其他联赛 -> Unavailable |

决策规则：

1. 若一个 Provider 覆盖该 operation 的全部目标范围，按单 Provider 处理。
2. 若需要多个 Provider 才能覆盖目标范围，必须设计“按请求参数路由到覆盖该范围的 Provider”。
3. 若当前 schema/mapping 无法表达覆盖范围，**这是 DECISION REQUIRED**：选择在 mapping JSON 中增加 `coverage` 元数据，或允许一次最小 migration 增加覆盖字段；AI 不得自行决定。
4. 未被任何 Provider 覆盖的子范围，不得宣称可用；具体状态为 `Unavailable`，或经决策门 3 确认后使用 `CoverageLimited`/`PartialReady`，不得由模型补写，也不得用普通 Web 冒充。
5. 若覆盖模型过于复杂，允许把该 operation 的支持范围收缩到单一 Provider 能覆盖的子集，并在产品支持矩阵中明确声明。

## 6. 输出和证据

Provider 不能提供 Iris evidence ID。执行顺序固定为：

1. 调用冻结 Provider；
2. 在内存中对白名单字段做 mapping；
3. 验证 DTO；
4. 将受限 DTO/摘录登记到现有 evidence ledger；
5. 使用数据库生成的 evidence ID；
6. Host 或结构化 finalization 只能引用这些 ID；
7. 持久化最终消息和 Run 终态；
8. sink 失败时从数据库恢复，不重新执行 Provider。

## 7. 降级政策

| 领域          | 无结构化 Provider                             | Provider 全部失败 | 允许模型自由补写 |
| ------------- | --------------------------------------------- | ----------------- | ---------------- |
| Runtime time  | 使用本机 runtime                              | 返回 runtime 错误 | 否               |
| News          | WebEvidenceBroker；仍需发布日期、来源和时间窗 | 证据不足          | 否               |
| Weather       | Unavailable                                   | Unavailable       | 否               |
| Finance       | Unavailable；公司新闻只有明确 Web 规则时例外  | Unavailable       | 否               |
| Entertainment | Unavailable                                   | Unavailable       | 否               |
| Sports        | Unavailable                                   | Unavailable       | 否               |

是否增加某个 operation 的 Web fallback 必须先定义确定性字段提取和 validator；不能仅依靠模型阅读搜索摘要。

## 8. 管理中心

管理中心按当前支持矩阵内全部 operation 展示（目标 11 个）：

- 状态：未配置、待验证、可用、降级、不健康、WebFallback（仅 News）；
- 主 Provider 和备用数量；
- 最近安全探测时间；
- 安全 reason code 和修复入口；
- mapping 必需字段是否完整。

界面不显示 endpoint、credential refs、原始 Provider JSON 或用户查询参数。

## 9. 双重完成门禁

### 软件门禁

使用本地、确定性的 contract fixture（MCP；若 07 OD-002 允许 REST adapter，则包含对应 transport fixture）覆盖当前支持矩阵内的全部 operation 的正式生产链（目标为 11 个；若决策门 6 选择缩减，则以支持矩阵为准）。它证明 Iris 代码可以正确接入合规 Provider。

### 实例门禁

当前实例显示受支持 operation 均为 Ready/Operational，并用真实 Provider 完成当前支持矩阵内的天气、新闻、金融、娱乐和体育场景。它证明用户当前安装配置真的可用。

两者缺一时不得宣称完成。
