# D00 施工依赖单元与工作包索引

<!-- iris:object FILE-IMPLEMENTATION kind=rules file=true -->

本目录登记**施工依赖单元与工作包**：`D01`–`D06`。它们依据 [Agent 架构定义](../../docs/agent-architecture.md) §10.3 的 D0–D5 依赖边界展开，**不是工期或版本承诺**，也不沿用已归档的 HR 阶段完成度。

读法：

- `D01` 是入口（架构定义的 D0），其余工作包按依赖顺序展开；
- 每个工作包说明前置条件、改动范围、回归证据与回退条件；
- **可施工与可验收由计算得出**，不读历史标签：见 [治理规则](../rules/governance.md) §2.5；
- 工作包 `work.state` 以 `catalog.mjs` 为准，本索引表只作阅读入口，**不代替就绪计算**；
- 每条被替换路径由其表列组件的实现任务承担迁移与旧路径退役责任；跨组件变更必须指定一个主责组件，不能由「评测平面」代替执行责任。

依赖关系（与架构定义 §10.3 一致）：

| 工作包 | 架构定义单元              | 前置         |
| ------ | ------------------------- | ------------ |
| `D01`  | D0 建立可信基线与最小诊断 | 无（入口）   |
| `D02`  | D1 修正协议与有界纠偏     | `D01`        |
| `D03`  | D2 收清会话、权限与上下文 | `D01`        |
| `D04`  | D3 接通双路搜索完整链路   | `D02`、`D03` |
| `D05`  | D4 贯通文档任务与最终交付 | `D02`、`D03` |
| `D06`  | D5 校准整体行为及可选委派 | `D04`、`D05` |

工作包索引：

| ID    | 标题                       | 责任对象 | 前置         | 状态      |
| ----- | -------------------------- | -------- | ------------ | --------- |
| `D01` | D01 建立可信基线与最小诊断 | `M09`    | 无（入口）   | `done`    |
| `D02` | D02 修正协议与有界纠偏     | `C14`    | `D01`        | `planned` |
| `D03` | D03 收清会话、权限与上下文 | `C07`    | `D01`        | `planned` |
| `D04` | D04 接通双路搜索完整链路   | `C20`    | `D02`、`D03` | `planned` |
| `D05` | D05 贯通文档任务与最终交付 | `C23`    | `D02`、`D03` | `planned` |
| `D06` | D06 校准整体行为及可选委派 | `C15`    | `D04`、`D05` | `planned` |

正式定义位置：`D01`–`D05` 在本目录各自的文件内，文件级对象即工作包本体；`D06` 的定义位置登记为 [tests/legacy-batch-plan.md](../tests/legacy-batch-plan.md)，沿用该文件的批次命名，不改动其内容边界。

工作包的作用域以登记表为准：需求条目（`N01`–`N20`）见 [requirements.md](../requirements/requirements.md)，源码事实与未决问题（`Q*`）见 [current-baseline.md](../requirements/current-baseline.md)，缺口（`G*`）见 [open-items.md](../requirements/open-items.md)。`D01`–`D05` 各自在“前置条件”一节内重述自己的作用域、前置与关闭项。

本目录只登记依赖边界与工作包内容，不复制证据仓库：证据用途与组合要求见 [evidence.md](../testing/evidence.md)，验收矩阵见 [acceptance-matrix.md](../testing/acceptance-matrix.md)。

<!-- iris:end FILE-IMPLEMENTATION -->

<!-- iris:object FILE-IMPLEMENTATION kind=rules -->

### FILE-IMPLEMENTATION 正式定义

## 索引对象说明

本对象是施工目录的索引容器：登记 `D01`–`D06` 的身份、责任对象、前置与状态，并说明「可施工／可验收由计算得出」的读法。各工作包的正式定义在自己的文件里，本对象只做索引与读法说明。

<!-- iris:end FILE-IMPLEMENTATION -->
