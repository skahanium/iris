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

readiness 从现有实体派生，不新增表：

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

不预设商业供应商。Provider 通过现有 MCP discovery 和管理中心接入，每个 operation 独立满足：

1. 工具是明确只读，schema 闭合且参数预算可控。
2. 响应能用受限 JSON path 映射全部必需字段。
3. 来源是 HTTPS 可定位资源，时间、地域、单位和延迟完整。
4. 真实预览通过 DTO validator。
5. timeout、rate limit、schema drift 和空数据返回稳定安全码。
6. 原始参数、输出、transport 和凭证不进入诊断。

AnySearch/Tavily 只有在 MCP discovery 实际暴露独立领域工具并逐 operation 通过准入时，才能建立领域 binding。普通 `web_search/web_fetch` 映射不能直接升级。

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

管理中心按 11 个 operation 展示：

- 状态：未配置、待验证、可用、降级、不健康；
- 主 Provider 和备用数量；
- 最近安全探测时间；
- 安全 reason code 和修复入口；
- mapping 必需字段是否完整。

界面不显示 endpoint、credential refs、原始 Provider JSON 或用户查询参数。

## 9. 双重完成门禁

### 软件门禁

使用本地、确定性的 MCP contract fixture 覆盖 11 个 operation 的正式生产链。它证明 Iris 代码可以正确接入合规 Provider。

### 实例门禁

当前实例显示受支持 operation 均为 Ready/Operational，并用真实 Provider 完成天气、新闻、金融、娱乐和体育场景。它证明用户当前安装配置真的可用。

两者缺一时不得宣称完成。
