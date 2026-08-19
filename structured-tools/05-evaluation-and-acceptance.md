# 05. 评测与验收

## 1. 验收分层

| 层级            | 证明内容                                               | 不能替代           |
| --------------- | ------------------------------------------------------ | ------------------ |
| Contract        | schema、mapping、DTO validator                         | 真实 Provider 可达 |
| Component       | resolver、surface、dispatch、health                    | 正式 Run 数据流    |
| Production path | intake → snapshot → evidence → finalization → recovery | 当前实例已配置     |
| Instance        | 用户配置的真实 Provider 和真实查询                     | 可重复自动化回归   |

完成声明必须同时列出 Production path 和 Instance 证据。

## 2. 问题—测试追踪

下表中“目标测试”在实际加入仓库并运行通过前，状态保持 Planned/Confirmed，不得计入完成数量。

| 问题 ID          | 当前状态  | 目标测试                                                              | 证明边界                                |
| ---------------- | --------- | --------------------------------------------------------------------- | --------------------------------------- |
| DOM-AVAIL-001    | Confirmed | `domain_readiness_requires_an_operation_specific_binding`             | 无 binding 时 operation 是 Unconfigured |
| DOM-HEALTH-001   | Confirmed | `domain_readiness_uses_persisted_health_instead_of_the_healthy_label` | Provider 选择消费持久化健康事实         |
| DOM-SURFACE-001  | Confirmed | `unconfigured_entertainment_is_not_advertised_as_callable`            | 模型和 UI 不夸大能力                    |
| DOM-ROUTE-001    | Confirmed | `weather_binding_never_authorizes_finance_tool`                       | 授权精确到 operation                    |
| DOM-PREVIEW-001  | Confirmed | `domain_binding_preview_must_validate_a_real_record_before_ready`     | 保存 mapping 不等于 Ready               |
| DOM-FALLBACK-001 | Confirmed | `non_news_domain_never_uses_generic_web_as_structured_success`        | 普通 Web 不冒充结构化结果               |
| DOM-RETRY-001    | Partial   | `provider_retry_does_not_consume_business_search_round`               | 技术尝试与业务轮次分离                  |
| DOM-UPGRADE-001  | Confirmed | `migration_072_upgrades_059_without_fake_domain_bindings`             | 旧库升级真实且不伪造配置                |
| DOM-INTAKE-001   | Resolved  | `completed_conversation_accepts_a_third_current_movie_turn`           | 0 候选不阻断第三轮 Run                  |

## 3. 11-operation 软件门禁

每个测试必须通过正式 intake 创建 Run，冻结本地 MCP contract fixture snapshot，登记真实数据库 evidence ID，由 Host/结构化 finalization 生成最终消息，并从数据库恢复：

```text
production_weather_current_uses_frozen_provider
production_weather_forecast_uses_frozen_provider
production_news_search_uses_structured_provider
production_finance_quote_uses_frozen_provider
production_finance_metrics_uses_frozen_provider
production_finance_news_uses_frozen_provider
production_entertainment_now_playing_uses_frozen_provider
production_entertainment_upcoming_uses_frozen_provider
production_entertainment_streaming_uses_frozen_provider
production_sports_schedule_uses_frozen_provider
production_sports_score_uses_frozen_provider
```

每项必须断言：

- operation、工具名和 snapshot 完全一致；
- Provider/binding/config hash 属于 Run 接受时冻结事实；
- Provider 伪造 evidence ID 被忽略；
- DTO 字段、时效、地域和 HTTPS 来源通过 validator；
- evidence ID 由 Iris 数据库生成；
- 最终正文不引入 DTO 外实体、数字、日期或状态；
- sink 失败或页面恢复不重新执行 Provider。

## 4. 负例矩阵

- 无 binding：Unconfigured，工具不进入 surface。
- binding disabled、untrusted 或 hash drift：NeedsReview/Unavailable。
- output mapping 缺字段或路径非法：预览失败。
- observation/asOf/checkedAt 陈旧：不生成当前结论。
- HTTP 或缺失 source URL：拒绝记录。
- 缺城市：天气和附近影院进入同 Run 输入恢复。
- 多 Provider 无明确顺序：不静默按名称挑选。
- 主 Provider timeout：只尝试冻结备用。
- 所有候选失败：非 News 不走普通 Web 冒充。
- 原始输出包含 `SECRET_SENTINEL`、`NOTE_SENTINEL`、`ARGUMENT_SENTINEL`：事件、审计、UI 和 eval 均不得出现。

## 5. 实例门禁

当前实例必须导出 11-operation readiness，并同步到 [`06-instance-readiness-record.md`](06-instance-readiness-record.md)。每个受支持 operation 至少有一个 Ready Provider；随后人工执行：

1. 指定城市的当前天气与七日内预报；
2. 最近 72 小时新闻；
3. 明确标的的行情、指标和相关新闻；
4. 指定城市正在上映、全国即将上映和指定地区流媒体可用性；
5. 明确赛事的赛程和比分。

记录只包含：operation、Provider 显示名称、请求地域/标的、检查时间、成功/安全错误、最终来源。禁止记录 endpoint、token、credential ref 或原始输出。

实例门禁必须同时验证应用实际使用的数据库已具备 migration 072 schema。

## 6. 完整质量门

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run lint
npm run format:check
npm run typecheck
npm run test
npm run docs:check
npm run agent:eval:smoke
npm run agent:eval
```

不运行 live API eval。真实服务验收由实例门禁人工执行。

## 7. 完成声明模板

只有全部门禁通过后才允许使用：

> 结构化领域软件门禁 11/11 通过；当前实例受支持 operation readiness 全部为 Operational；天气、新闻、金融、娱乐和体育真实场景均完成，未发生普通 Web 冒充、证据身份混淆或敏感诊断泄漏。

如果软件门禁通过但实例门禁未通过，只能使用：

> Iris 代码具备接入合规结构化 Provider 的生产能力；当前实例仍有未配置或未验证的 operation，尚不能宣称垂直工具全部可用。
