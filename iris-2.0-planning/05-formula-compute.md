# 05 - 公式、统计与计算引擎

## 目标

Iris 2.0 的多维表必须支持一定高级统计、计算和自定义公式，同时保持本地高速体验。不能采用每次渲染逐 cell 解释公式的慢路径。

## 核心决策

- 公式采用 Iris DSL。
- 不兼容完整 Excel。
- 不照搬 Notion formula。
- 不允许 JavaScript 或任意代码执行。
- Rust 后端是正式计算权威。
- 前端只做输入、补全、预览和展示。

## DSL 原则

1. 强类型。
2. 无副作用。
3. 可解析 AST。
4. 可分析依赖。
5. 可估算成本。
6. 可缓存结果。
7. 可增量重算。
8. 禁止网络、文件、凭据和 Agent 调用。

## 公式示例

```text
prop("单价") * prop("数量")

if(prop("状态") == "完成", 0, date_diff(prop("截止日期"), today(), "day"))

round(prop("金额") * prop("税率"), 2)

sum(relation("订单").prop("金额"))
```

## 函数范围草案

### 数值

```text
+ - * /
round
min
max
abs
clamp
```

### 文本

```text
concat
contains
starts_with
ends_with
lower
upper
trim
```

### 日期

```text
today
now
date_add
date_diff
year
month
day
```

### 逻辑

```text
if
and
or
not
coalesce
```

### 字段 / 关系

```text
prop
relation
rollup
sum
avg
count
count_if
```

## 分层计算

```text
基础字段：用户或 Agent 写入的事实值。
计算字段：行级公式。
统计字段：View / Collection 聚合。
关系 rollup：通过 relation 汇总关联对象。
```

这三层不能混为一谈。

## 重算模型

已讨论倾向：增量即时 + 后台计算。

```text
小范围修改 -> 同步局部重算
大范围影响 -> dirty 标记 + 后台计算队列
高成本公式 -> 保存前提示成本
```

## 性能机制

- formula_dependencies 记录依赖图。
- formula_value_cache 缓存公式结果。
- aggregate_cache 缓存高频聚合。
- compute_jobs 执行后台重算。
- UI 显示计算中 / stale / fresh 状态。

## 成本分级

```text
low: 单行字段、简单函数
medium: 关系 rollup、小范围聚合
high: 大范围聚合、多层依赖、大量记录重算
forbidden: 循环依赖、无限关系展开、非确定性高频重算
```

## 公式编辑体验

双入口：

- 自然语言生成公式：Agent 生成 DSL，用户确认。
- 手写 DSL：字段补全、函数提示、类型错误、依赖预览、样例结果。

两者共用同一套 parse / typecheck / dependency / preview / save 管线。

## 待讨论

- DSL 具体语法是否采用函数式、类 Notion 还是类 SQL 表达式。
- `today()` 是否造成每日自动 dirty。
- rollup 最大深度。
- 公式错误值如何显示。
- 大规模导入后的计算优先级。
- 是否支持用户定义命名公式模板。
