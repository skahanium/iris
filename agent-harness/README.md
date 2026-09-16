# Agent Harness 文档体系

<!-- iris:object P01 kind=rules file=true -->

本目录是 Iris Agent／Harness 的**详细规范与施工依据**：模块、组件、工具、链路、证据要求与施工工作包。它按 [施工文档体系定稿方案](../document.md) 建立，模块／组件／工具身份派生自 [Agent 架构定义](../docs/agent-architecture.md)。

**首次阅读路径**：本文件 → [治理规则](./rules/governance.md) → [对象与标记规则](./rules/objects.md) → [需求与边界](./requirements/requirements.md) → 需要施工时看 [implementation/](./implementation/README.md)。

## 一、本体系做什么、不做什么

| 本体系负责                           | 本体系不负责                                       |
| ------------------------------------ | -------------------------------------------------- |
| 对象身份、正式定义位置、修订与指纹   | 版本排期（唯一来源是 [ROADMAP.md](../ROADMAP.md)） |
| 模块／组件／工具／链路的详细合同     | 自动证明合同合理或工具可用                         |
| 变更分类、独立复核、解封依据         | 替代真实模型验收                                   |
| 证据用途分类与验收判据               | 读取模型私有思维链                                 |
| D0–D5 依赖顺序、工作包内容与执行记录 | 虚构尚未创建的工单或负责人                         |

权威边界、状态四维、关系模型、变更分类与阻断规则见 [rules/governance.md](./rules/governance.md)。**一个对象只有一个正式定义位置**：链路文档只说明交接顺序并引用合同，模块文档说模块责任，工具卡只写工具自身合同。

## 二、目录与职责

| 区域                               | 职责                                                                                                                                                       |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 入口与维护规则                     | 本文件（阅读路径、体系总览）与 [`rules/`](./rules/governance.md)（权威、状态、复核、变更、归档规则）                                                       |
| `requirements/`                    | [需求条目 N01–N23](./requirements/requirements.md)、[当前基线与未决问题 Q\*](./requirements/current-baseline.md)、[缺口 G\*](./requirements/open-items.md) |
| `modules/`                         | 9 份模块规范，逐项展开 [`C01`–`C27`](./modules/README.md)；另有[评测系统 E01–E03](./modules/eval-system.md)                                                |
| `contracts/`                       | 共享接口、状态转换、不变量及异常合同（[`K01`–`K18`](./contracts/README.md)）                                                                               |
| `tools/`                           | [39 个业务工具的独立卡片](./tools/README.md)，以及协议入口、Planned 与外部 MCP 的区别                                                                      |
| `flows/`                           | [对话与修订、联网、编辑、格式保持、上下文与记忆、委派六条链路](./flows/README.md)                                                                          |
| `testing/`                         | [证据分类、覆盖关系、证据组合与验收要求](./testing/evidence.md)                                                                                            |
| `implementation/`                  | [D0–D5 依赖、工作包与执行记录](./implementation/README.md)                                                                                                 |
| `decisions/`                       | [架构变更、细化、复核与替代记录](./decisions/README.md)                                                                                                    |
| [`registry.json`](./registry.json) | 身份、正式定义位置、修订、指纹、类型化关联、决定与复核记录                                                                                                 |
| `catalog.mjs`                      | 登记表的人工维护部分（身份、责任归属、成熟度、关系、作用域）                                                                                               |
| `archive/`                         | 已被取代的材料，只作对照，**不得作为现行依据**                                                                                                             |

执行结果继续放在既有 `docs/eval/results/`，本体系不复制证据仓库。

## 三、对象计数

| 种类         | ID 范围     | 数量 | 正式定义位置                                |
| ------------ | ----------- | ---: | ------------------------------------------- |
| 运行时模块   | `M01`–`M09` |    9 | `modules/`                                  |
| 运行时组件   | `C01`–`C27` |   27 | `modules/`                                  |
| 评测系统组件 | `E01`–`E03` |    3 | `modules/eval-system.md`                    |
| 内置业务工具 | `T01`–`T39` |   39 | `tools/`                                    |
| 共享合同     | `K01`–`K18` |   18 | `contracts/`                                |
| 链路         | `L01`–`L06` |    6 | `flows/`                                    |
| 证据与判据   | `V01`–`V08` |    8 | `testing/`                                  |
| 工作包       | `D01`–`D06` |    6 | `implementation/`、`tests/`                 |
| 决定记录     | `R01`–`R11` |   11 | `decisions/`                                |
| 规则         | `P01`–`P07` |    7 | 本文件、`rules/`、`requirements/`、`tools/` |
| 检查器与测试 | `X01`–`X03` |    3 | `scripts/`、`tests/`、`registry.json`       |
| 未决问题     | `Q01`–`Q19` |   19 | `requirements/current-baseline.md`          |
| 缺口         | `G01`–`G06` |    6 | `requirements/open-items.md`                |

计数规则与 ID 分配见 [rules/objects.md](./rules/objects.md) §1。**计数不代表成熟度**：`M*`、`C*`、`T*` 当前多为 `draft`（已有正式定义、仍在修订），`draft` 不能支持施工依赖。

## 四、状态怎么读

四个维度互不覆盖：文档成熟度、实现状态、验证结果、工作包状态（见 [rules/governance.md](./rules/governance.md) §2）。

- `registered` 只有登记，没有正式定义；它是待补齐关系的**目标**，但不能作为可施工工作的必要前置。
- `draft` 已有正式定义正文，仍在修订。
- `defined` 正式定义完整，九项合同要素齐备，可被引用为规范。
- **可施工／可验收由计算得出**，不读历史标签：历史测试通过不能支持变化后的当前验收。

## 五、检查器

```bash
npm run agent-harness:check     # 结构、指纹、关系、就绪、复核、证据、归档、索引
npm run agent-harness:test      # X02 负例自测（node --test，夹具在临时目录）
```

| 退出码 | 含义                                                       |
| ------ | ---------------------------------------------------------- |
| `0`    | 全部检查完整执行且通过                                     |
| `1`    | 结构或规范违规                                             |
| `2`    | 执行基础设施失败（注册表不可解析、文件读取失败、脚本异常） |

CI 不允许对文档门使用忽略失败或成功兜底；只有全部检查完整执行且通过才输出通过。

## 六、交付批次与当前状态

| 批次       | 内容                                                           | 状态                    |
| ---------- | -------------------------------------------------------------- | ----------------------- |
| 基础设施批 | 入口、治理规则、完整对象登记、最小已定义样例、检查器及负例测试 | 已接通（本轮）          |
| 全局规范批 | 按模块和链路逐批补齐正式定义，处理现行文档与旧计划的权威冲突   | 本轮建立全量正式定义    |
| 施工准备批 | D0 先达到可直接执行；D1–D5 按依赖逐批细化                      | 本轮建立 D01–D06 工作包 |

已执行 D01 的最小诊断与确定性评测记录，**未运行付费实网评测**。因此：

- `T*`、`M*`、`C*` 的 `implementation.state` 只描述静态源码事实；
- 只有 `X01`／`X02` 声明 `verification.state=passed`；C26／C27／E01–E03／V07 的通过与否以 `registry.json.verify` 为准，不把机械记录标成语义质量通过；
- 工作包 `D01` 的 `work.state` 以 `catalog.mjs` 为准；`D02`–`D06` 仍为 `planned`。

**本轮不把以下事项冒充为已解决**：具体供应商／模型端点的原生搜索兼容与性能；具体第三方协议库或整体 agent 框架的最终依赖选择；新预算数值、费用上限、发布分数与版本排期；每个组件未来对应多少 Rust 文件或 React 组件；现有所有工具已正确工作，或此架构已通过真实用户任务验收。

## 七、与既有文档的关系

| 既有文档                                                                          | 关系                                                         |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| [AGENT-REFORM-DISCUSSION-2026-09-14.md](../AGENT-REFORM-DISCUSSION-2026-09-14.md) | 来源对象 `SRC-DISC-2026-09-14`；解释来源与事实边界，不是规范 |
| [docs/agent-architecture.md](../docs/agent-architecture.md)                       | 来源对象 `SRC-ARCH-2026-09-15`；上位架构决定                 |
| [ARCHITECTURE.md](../ARCHITECTURE.md)                                             | 当前实现事实；本体系不重复描述当前实现                       |
| [ROADMAP.md](../ROADMAP.md)                                                       | 版本排期唯一来源                                             |
| [AGENTS.md](../AGENTS.md)                                                         | 开发规范；本体系的文档门在 AGENTS §4.6 登记                  |
| [docs/README.md](../docs/README.md)                                               | 仓库文档索引；本体系作为现行规范入口登记                     |

## 八、归档

归档只作对照，其中的模块、测试与结论可能已不存在；除本索引与 [docs/README.md](../docs/README.md) 外，现行文档不得把归档材料作为依据引用。

| 归档入口                                                                                     | 内容                                                                 |
| -------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| [archive/2026-08-pre-unification/MANIFEST.md](./archive/2026-08-pre-unification/MANIFEST.md) | 统一前的 Harness 设计材料（refactor、structured-tools、REFACTOR.md） |
| [archive/2026-09-15-pre-reform/MANIFEST.md](./archive/2026-09-15-pre-reform/MANIFEST.md)     | 重建前的施工文档体系（README + 01–06 + 附录 A/B/C）                  |

## 九、维护规则

1. 新增、改名或删除托管文件，必须同步 `catalog.mjs` 的 `files` 与 `objects`，再运行
   `node scripts/agent-harness-check.mjs --reconcile --author <你> --classification refinement --reason <理由>`。
2. 修改任何对象正文后，检查器会报“登记未接受”；**接受变化是显式动作**，必须给出分类与理由。
3. 改变责任归属、状态权威、输入输出语义、决定权、不变量、权限、副作用、兼容或用户承诺，属于
   **架构变更**，必须同步架构定义、写 `decisions/` 记录，并由独立复核者作出结论。
4. 退役材料整批移入 `archive/<日期>-<原因>/` 并在其 `MANIFEST.md` 登记；现行文档不得引用归档。
5. 本体系的规则文件（`P02`、`P03`）自身变更属于**治理变更**，按架构变更复核。

<!-- iris:end P01 -->

<!-- iris:object P01 kind=rules name=harness-entry title="Agent Harness 文档体系入口" -->

## 十、入口对象

**P01 的职责**：本文件是 Agent Harness 文档体系的唯一入口。

- `docs/README.md` 的“现行规范”表必须指向本文件；
- 本文件登记阅读路径、目录职责、对象计数、状态读法与检查器用法；
- 本文件**不承载任何对象的正式定义**——定义在各对象自己的文件里；
- 本文件正文（含本节）变化时，`README.md` 的文件级指纹随之变化，本文件内其他对象进入复核。

<!-- iris:end P01 -->
