# 07. Provider 落地与决策流程

> 本文件是阶段 5 的强制前置流程，也是所有“AI 施工中必须停下来与设计者讨论”的权威清单。
> 目标：避免施工 AI 自行选择 Provider、自行假设 MCP、自行决定覆盖范围或自行放宽安全门。

## 1. 为什么需要本文件

当前代码已经具备框架，但 11 个 operation 的真实 Provider 接入是**不可由 AI 单方面拍板**的工作：

- 候选 Provider 可能是 MCP，也可能是 REST；
- 一个 operation 可能由多个 Provider 各覆盖一部分；
- 真实字段映射、ToS、成本、限流、地域和时效都需要人工判断；
- 如果找不到合规 Provider，是否缩减支持矩阵是产品决策。

因此，**“讨论并完善方案”是施工流程的一部分，而不是额外步骤**。

## 2. 总原则

1. 没有已确认的 Provider Decision Record（PDR），任何 operation 不得进入 mapping、preview、Ready 或 Operational。
2. 任何标记为 **DECISION REQUIRED** 的项，施工 AI 必须停下来向设计者提问，并给出具体选项和推荐项；不得默认选择。
3. PDR 由施工 AI 起草，但必须由设计者/用户确认。
4. 若施工中发现新歧义，立即停止，记录到本文件第 6 节 Open Decisions，并询问设计者；不得绕过继续。
5. 找不到合规 Provider 时，允许缩减支持矩阵，但该决定必须显式记录，不能悄悄跳过。

## 3. Provider Decision Record（PDR）模板

每个 operation 一份，以下字段全部必填：

| 字段               | 说明                                                        |
| ------------------ | ----------------------------------------------------------- |
| Operation          | 例如 `weather.current`                                      |
| 目标覆盖范围       | 地域、联赛/标的/平台/频道等                                 |
| 候选 Provider 列表 | 每个 Provider 的名称/来源（不写密钥）                       |
| Provider 类型      | MCP / REST / 其他                                           |
| 接入路径           | 现有 MCP registry？需要新增 REST adapter？                  |
| 覆盖矩阵           | 每个 Provider 覆盖哪些子范围，未覆盖范围是什么              |
| 字段映射样例       | Provider 示例 JSON → DTO 字段（脱敏）                       |
| 真实预览方案       | 使用什么安全公开参数，预期返回哪些字段                      |
| 健康/限流/失败模式 | timeout、rate limit、空数据、schema drift                   |
| ToS/成本/许可      | 是否允许用于 Iris 分发，是否有 AGPL 兼容问题                |
| 安全边界           | HTTPS、无敏感参数/输出持久化                                |
| 决策               | 实施哪个/哪些 Provider，顺序；或“无合规 Provider，移除支持” |
| 确认人             | 设计者/用户确认签名或记录                                   |

## 4. 强制讨论触发器

出现以下任一情况，施工 AI **必须停止并询问设计者**，不得继续：

1. 候选 Provider 不是 MCP，而是 REST 或其他协议；
   - 当前架构没有通用 REST adapter，是否新增 adapter 属于架构决策。
2. 同一 operation 需要多个 Provider 才能覆盖目标范围；
   - 必须确认覆盖矩阵、路由规则和 readiness 展示方式。
3. 某个 Provider 只能覆盖 operation 的一部分；
   - 必须确认未覆盖范围是 Unavailable，还是收缩支持矩阵。
4. 需要新增表、列、migration 或修改 schema 才能表达 readiness/preview/coverage；
   - 必须与决策门 1/3 一致。
5. Provider 的 ToS、成本、数据许可或 AGPL 兼容性不明确；
6. 真实预览无法用安全公开参数执行，或预览可能触发收费/限流；
7. 需要新增 REST adapter、新的 Provider registry 或第二套证据/健康真相源；
8. 需要引入 `PartialReady`/`CoverageLimited` 等新状态；
9. 某个 operation 在无合规 Provider 时是否从支持矩阵移除；
10. 任何对 `05-evaluation-and-acceptance.md` 验收边界的放宽。

## 5. 施工中的“停下并提问”协议

1. 每个 Task 开始前，先检查本文件 Open Decisions 和 `02-gap-register.md`。
2. 如果当前 Task 涉及未关闭的 DECISION REQUIRED 项，**先提问，后编码**。
3. 如果编码过程中发现设计文档没有覆盖的新情况，立即停止：
   - 把问题写入本文件第 6 节；
   - 向设计者说明影响范围和候选方案；
   - 等确认后再继续。
4. 未关闭决策门的 Task 不得提交“完成”。

## 6. Open Decisions（当前未决事项）

| ID     | 决策点                                                          | 候选选项                                                            | 推荐                 | 状态 |
| ------ | --------------------------------------------------------------- | ------------------------------------------------------------------- | -------------------- | ---- |
| OD-001 | operation 级 readiness/preview 如何持久化                       | A. 现有 binding 加列；B. 新表；C. 不新增 schema 但需解释 Ready 来源 | A                    | Open |
| OD-002 | 是否允许新增 REST adapter                                       | A. 允许；B. 只支持 MCP；C. 暂缓                                     | 视 Provider 调研结果 | Open |
| OD-003 | operation coverage 如何表示                                     | A. mapping JSON 增加 coverage 元数据；B. 新表/列；C. 文档人工维护   | A（若足够）          | Open |
| OD-004 | 多 Provider 子集覆盖时是否引入 `PartialReady`/`CoverageLimited` | A. 引入；B. 不引入，按子集声明支持                                  | 视产品需要           | Open |
| OD-005 | 无合规 Provider 的 operation 是否从支持矩阵移除                 | A. 移除并缩减文档；B. 保持 Unconfigured 并隐藏                      | A                    | Open |

> 任何 AI 施工者在开始阶段 5 的某个 operation 前，必须确认对应 OD 已关闭或该 operation 不依赖该 OD。

## 7. Decision Log 模板

每个关闭的决策门记录：

```markdown
### DEC-001：operation 级 readiness 持久化

- 日期：
- 选项：A / B / C
- 选择：A
- 理由：需要可恢复的管理中心矩阵和 operation 级健康
- 影响范围：migration、readiness IPC、管理中心
- 确认人：
```
