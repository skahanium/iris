# 06. 当前实例 Readiness 记录

**记录时间：2026-08-19。环境：`.iris-dev/app-data/iris.db`。**

本文件记录当前开发实例的非敏感可用性事实，不记录 endpoint、transport 配置、credential refs、API Key、查询参数或 Provider 原始输出。

## 1. 环境事实

| 项目                       | 当前值            |
| -------------------------- | ----------------- |
| 代码分支                   | `branch-v1.3.0`   |
| 审计提交                   | `bb600a7e`        |
| 普通 Web Provider          | AnySearch、Tavily |
| 普通 Web search/fetch      | 已配置            |
| migration 072 领域字段     | 开发库存在        |
| 外部 operation Configured  | 0/11              |
| 外部 operation Operational | 0/11              |

## 2. Operation Readiness

| Operation                   | Binding | Preview | Health   | Production Run | 当前状态     | 安全说明                 |
| --------------------------- | ------- | ------- | -------- | -------------- | ------------ | ------------------------ |
| `weather.current`           | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得输出当前天气         |
| `weather.forecast`          | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得输出确定性预报       |
| `news.search`               | 无      | 未执行  | Web 可用 | 普通 Web 路径  | WebFallback  | 仍需发布日期和来源       |
| `finance.quote`             | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得输出当前价格         |
| `finance.metrics`           | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得输出当前指标         |
| `finance.news`              | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得冒充结构化公司新闻   |
| `entertainment.now_playing` | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得把全国档期当本地排片 |
| `entertainment.upcoming`    | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得从模型记忆列近期影片 |
| `entertainment.streaming`   | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得猜测平台可用性       |
| `sports.schedule`           | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得输出未核验赛程       |
| `sports.score`              | 无      | 未执行  | 未知     | 未执行         | Unconfigured | 不得输出未核验比分       |

`system_time_now` 不依赖外部 binding，当前状态为 Operational。

## 3. 更新纪律

一个 operation 只有按顺序取得以下证据，才可以更新状态：

1. Binding：管理中心保存了只读、受信、hash-current 的 operation mapping。
2. Preview：真实响应通过 output mapping 和 DTO validator。
3. Health：最近探测成功且未熔断。
4. Production Run：正式 intake、snapshot、evidence、finalization 和恢复通过。

状态变化规则：

```text
Unconfigured
  -> NeedsReview      保存 binding，尚未通过真实预览
  -> Ready            预览与健康通过
  -> Operational      正式 Run 与恢复通过
  -> Degraded         主路由失败但存在安全 fallback/冻结备用
  -> Unhealthy        持续失败或熔断
```

禁止：

- 只因工具出现在目录就更新为 Ready；
- 只因 fixture 通过就更新当前实例；
- 在表中记录 endpoint、token、参数或原始响应；
- 通过手工编辑数据库制造 binding；
- 未通过某个 operation 的验收，却用同 Provider 的其他 operation 成功代替。

## 4. 安装版记录

当前 macOS 安装版观察到旧 binding schema，只允许 `external.read`。升级到包含 migration 072 的构建后，必须重新填写以下记录：

| 检查项                          | 当前状态 |
| ------------------------------- | -------- |
| 应用实际数据目录已确认          | 未验证   |
| 059→072 migration 成功          | 未验证   |
| 旧 `external.read` binding 保留 | 未验证   |
| 未自动生成虚假领域 binding      | 未验证   |
| readiness 与开发版规则一致      | 未验证   |

安装版未完成上述检查前，不得用开发库状态替代安装实例状态。
