# 01. 当前状态与证据

**审计时间：2026-08-19。代码基线：`branch-v1.3.0`，`bb600a7e`。**

## 1. 审计口径

本审计分别检查五层事实：

1. Catalog：工具和 operation 是否注册。
2. Contract：DTO、字段和时效 validator 是否存在。
3. Configuration：当前实例是否存在领域 binding。
4. Runtime：Provider 是否健康、能否被 Run 冻结和调用。
5. Acceptance：生产路径和用户实例是否完成验收。

工具显示为 `Dispatchable` 只证明 Catalog/dispatch 分支存在，不证明 Configuration、Runtime 或 Acceptance。

## 2. 代码层已经存在的能力

### 2.1 工具与 operation

| 工具                   | Operation                                                                        |
| ---------------------- | -------------------------------------------------------------------------------- |
| `system_time_now`      | 本机当前日期、时间、星期和时区                                                   |
| `weather_lookup`       | `weather.current`、`weather.forecast`                                            |
| `news_lookup`          | `news.search`                                                                    |
| `finance_lookup`       | `finance.quote`、`finance.metrics`、`finance.news`                               |
| `entertainment_lookup` | `entertainment.now_playing`、`entertainment.upcoming`、`entertainment.streaming` |
| `sports_lookup`        | `sports.schedule`、`sports.score`                                                |

五个外部工具都在 `src-tauri/src/ai_runtime/tool_catalog/fresh_domains.rs` 注册，并通过 `web.domain.read` 授权。

### 2.2 已有结构

- migration 072 为 binding/snapshot 增加 `domain_operation` 和 `output_mapping_json`。
- `DomainOperation` 定义 11 个稳定 operation。
- `FreshDomainRecord` 定义 Weather、News、Finance、Entertainment、Sports DTO。
- validator 检查 HTTPS 来源、必需字段、时间窗口、地域、单位和 operation/variant 一致性。
- `FreshDomainService` 可以消费 Run-frozen MCP snapshot，映射 Provider JSON 并登记 evidence。
- Host renderer 可以从验证后的领域记录生成受控最终内容。
- 缺城市、陈旧事实、字段不足和 Provider 不可用均有 fail-closed 边界。

这些事实说明框架已形成，不说明任何真实外部领域服务已经配置。

## 3. 当前开发实例配置

审计数据源为 `.iris-dev/app-data/iris.db`。查询只读取 Provider 名称、启用状态、operation 和脱敏健康摘要，不读取 transport 配置、credential refs 或凭证明文。

### 3.1 普通 Web Provider

| Provider  | Enabled | Web search | Web fetch | 健康摘要                             |
| --------- | ------- | ---------- | --------- | ------------------------------------ |
| AnySearch | 是      | 已映射     | 已映射    | 有成功记录；最近记录包含一次 timeout |
| Tavily    | 是      | 已映射     | 已映射    | 有成功记录；当前无连续失败           |

当前搜索路由包含 Tavily 和 AnySearch。该配置只支持普通 WebEvidenceBroker，不自动产生 `web.domain.read` binding。

### 3.2 领域 binding 实况

| Operation                   | Configured bindings | Enabled/trusted bindings | 当前行为                                    |
| --------------------------- | ------------------- | ------------------------ | ------------------------------------------- |
| `weather.current`           | 0                   | 0                        | 缺城市先询问；有城市后 Provider unavailable |
| `weather.forecast`          | 0                   | 0                        | 缺城市先询问；有城市后 Provider unavailable |
| `news.search`               | 0                   | 0                        | 可以使用普通 Web evidence                   |
| `finance.quote`             | 0                   | 0                        | Provider unavailable                        |
| `finance.metrics`           | 0                   | 0                        | Provider unavailable                        |
| `finance.news`              | 0                   | 0                        | Provider unavailable                        |
| `entertainment.now_playing` | 0                   | 0                        | 缺城市先询问；有城市后 Provider unavailable |
| `entertainment.upcoming`    | 0                   | 0                        | Provider unavailable                        |
| `entertainment.streaming`   | 0                   | 0                        | Provider unavailable                        |
| `sports.schedule`           | 0                   | 0                        | Provider unavailable                        |
| `sports.score`              | 0                   | 0                        | Provider unavailable                        |

结论：外部领域为 **0/11 Configured**。

### 3.3 安装版差异

macOS 安装版 `app-data/iris.db` 仍可观察到旧 binding schema：capability CHECK 只允许 `external.read`，没有 `domain_operation`。因此：

- 开发库 migration 072 已存在，不等于用户正在运行的数据库已经升级。
- 旧安装实例不能保存领域 binding。
- 重新构建后必须验证真实 059→072 升级和应用实际使用的数据目录。

## 4. 当前生产路由

`FreshDomainService` 的实际降级规则是：

```text
有 Run-frozen structured snapshot
  -> MCP 调用
  -> 白名单 output mapping
  -> DTO validator
  -> evidence ledger
  -> Host/model finalization

没有 structured snapshot
  -> news.search: WebEvidenceBroker
  -> weather/finance/entertainment/sports: structured_provider_unavailable
```

这比“任意领域都可以普通 Web fallback”更严格，也更安全。现阶段不得把目标设计中的 Web 降级描述成已实现事实。

## 5. 2026-08-19 第三轮提交故障

“你好 → 今天几号 → 最近上映电影”的第三轮曾在 Run intake 返回通用“请求未能提交”。根因不是会话只允许两轮，而是：

- 当前请求首次分类为 Entertainment；
- 当前实例有选中的普通 Web Provider，但没有任何领域 binding；
- intake 错误地把“0 个结构化候选”判断为“多个 Provider 歧义”。

`bb600a7e` 已修正为：0 个候选不阻断 Run 创建；真正多个候选且无确定顺序时仍失败关闭。这个修复只解决 intake 错判，不会凭空配置 `entertainment.now_playing`。

## 6. 现有测试能证明什么

能证明：

- 工具名、参数 schema 和 dispatch 已注册；
- DTO 的必需字段和时效规则会 fail-closed；
- output mapping 使用受限 JSON path；
- Provider 原始输出不会直接进入 UI 诊断；
- 无天气 Provider 时不会用普通 Web 冒充；
- Host renderer 能消费验证后的测试记录。

不能证明：

- 当前实例存在任何结构化领域 binding；
- AnySearch/Tavily 暴露天气、行情、排片或比分结构化工具；
- Provider 健康事实参与了实际选择；
- 11 个 operation 都通过了正式 intake 到最终消息的生产路径；
- 用户安装版 schema 和开发库一致。

## 7. 当前允许的声明

允许：

- “Iris 已具备结构化领域工具框架和确定性验证边界。”
- “当前实例已配置普通 Web 搜索。”
- “当前实例尚未配置天气、金融、娱乐和体育的结构化 Provider。”

禁止：

- “结构化工具全部落地可用。”
- “11 个 operation 单元测试通过，因此真实服务已配置。”
- “AnySearch/Tavily 的普通搜索等于领域结构化能力。”
- “`Dispatchable` 等于 Operational。”
