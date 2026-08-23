# 05. 实施路线图

本路线图只规定 Harness 内部依赖和退出条件，不对应产品版本。任何版本安排必须先进入根 `ROADMAP.md`。

## 1. 阶段总览

| 阶段                    | 状态   | 目标                                           | 同阶段必须完成的删减                        |
| ----------------------- | ------ | ---------------------------------------------- | ------------------------------------------- |
| AH-0 可信评测基线       | 已验证 | Windows 与 POSIX 使用一致的安全和 fixture 合同 | 删除跨平台误判和 fixture 漂移               |
| AH-1 文档与事实源统一   | 已验证 | 单一入口、状态账本、目标合同和归档             | 移除根目录旧入口和两套现行路线              |
| AH-2 自适应研究循环     | 计划中 | 接通搜索、抓取、修复、轮次和 deadline 预算     | 删除 `max_fetches: 0` 与无效预算字段        |
| AH-3 按任务形态重构路由 | 计划中 | 精确快路径与研究路径协调                       | 删除无 binding 的领域级前置阻断和 News 特例 |
| AH-4 性能、质量与清理   | 计划中 | 建立 profile 基线并清除不可达代码              | 移除 dead-code 许可、旧测试和残余双轨       |
| AH-5 真实 Provider 试点 | 延期   | 仅按真实需求接一个已批准 Provider              | 不建设 11/11 readiness 平台                 |

## 2. AH-0：可信评测基线

已完成：

- 为 credential metadata 增加明确平台语义；POSIX 保留 owner/mode 检查，Windows 不解释 POSIX mode bits。
- 保留 absolute path、realpath、非 filesystem root、目录/文件类型和 source DB/data root 绑定。
- 修复 PowerShell 5 对无 BOM 非 ASCII fixture 的解析失败，并使 Windows/POSIX dated snippet 一致。
- 通过脚本测试 8/8、单一 Web evidence case 和 smoke 24/24。

退出条件：full 48-case 已通过；完整 Rust 与前端门禁仍须在最终交付前重新运行。结果写入附录 A，不修改历史版本化报告，除非显式使用受控更新开关。

## 3. AH-1：文档与事实源统一

实施内容：

- 通过 Git rename 将旧目录和根入口迁入带日期归档。
- 创建本文档体系，并更新 `docs/README.md` 的唯一入口。
- 文档事实校验必须验证相对链接、必需文件、归档边界和旧根路径不存在。
- 状态采用“实现状态 + 处置方式”，旧通过数不自动继承。

退出条件：`docs:check` 与文档测试已通过；全量格式检查纳入最终质量门。当前文档不引用归档作为规范。

## 4. AH-2：接通自适应研究循环

测试先行：

1. 为 Quick/Standard/Deep 的搜索、抓取、模型轮次、证据和 deadline 编写表驱动失败测试。
2. 为 subsequent call 的 `gap`、current-Run URL provenance、重复 URL/query 和两轮无新增证据编写测试。
3. 为单轮最多 3 个并发抓取、取消和恢复预算编写测试。

实现与同步删除：

- 复用 `web_search` 的 `query`、`gap`、`urls`，让 Host 在一个工具合同内执行搜索与深抓取。
- 将 `ResearchBudget.max_fetches`、`max_repairs` 和剩余 deadline 接入执行器及恢复状态。
- 把模型工具循环的 `max_fetches: 0` 替换为当前 profile 剩余预算。
- 只有明确证据充分时保留 Direct 提前结束；需要继续研究时进入同一 bounded loop。
- 删除不再产生控制效果的预算字段、重复 planner helper 和对应旧测试。

退出条件：预算不可越界、Deep 不会静默启用、Web 关闭无外发、搜索和抓取共用 current-Run evidence ledger。

## 5. AH-3：按任务形态重构路由

测试先行：

- 精确报价有 binding 时走结构化快路径；无 binding 但 Web 字段充分时仍可回答。
- 市场原因、新闻综述、体育前瞻和影视推荐即使属于现有领域，也走 Web research。
- 无 binding 且 Web 不足时返回稳定不足结果，不在模型前统一失败，也不编造精确值。
- News 不再拥有独占的 Web fallback 架构特例。

实现与同步删除：

- Intake 增加内部 task-shape 决策，保留领域字段合同。
- 移除 `domain_operation_is_executable` 的“无 binding 即模型前失败”职责。
- 移除 `constrain_domain_tool_surface` 中隐藏通用 `web_search` 的领域特例。
- 删除断言十个非 News operation 必须在模型前失败的旧测试，替换为任务形态矩阵。
- 保留结构化 DTO、validator、renderer、fixtures 和 migration 072。

退出条件：模型、capabilities、executor 与恢复路径看到同一冻结表面；不存在领域级双轨最终化。

## 6. AH-4：性能、质量与不可达代码清理

- 为三个 profile 记录固定 provider/model/fixture 的 p50、p95、token、搜索、抓取、模型轮次和 evidence count。
- first progress event 在本地 Host 路径目标为 500ms 内。
- 同 profile p95 或 token 回退超过 20% 时阻止合并，除非质量有可量化收益并记录决定。
- 审计 `fresh_domains` 模块级 `allow(dead_code)`，删除不可达 enum、helper、adapter 和测试。
- 使用静态搜索确认只有一个研究循环、一个模型可见网络工具、一个 provider registry 和一个 evidence ledger。

退出条件：质量底线不下降，硬安全门禁零失败，活跃研究模块没有用 dead-code 许可隐藏债务。

## 7. AH-5：真实 Provider 试点（延期）

仅当重复使用场景证明结构化快路径能显著改善精确性或延迟时启动。每个试点先完成 PDR，包含许可、成本、覆盖、字段、真实预览、失败模式和删除方案。

本阶段不授权：通用 REST adapter、readiness 管理中心、多 Provider 自动 failover、新状态表或 11/11 覆盖目标。任何一项若确有必要，必须单独决策并证明不能复用现有事实源。

## 8. 阶段纪律

- 每阶段提交必须列出保留、重构、移除的代码和测试。
- 新抽象未删除旧分支时，阶段不得结案。
- migration、公共 IPC 和外部工具 schema 保持向后兼容；需要变更时同步 Rust、TypeScript wrapper、IPC 文档和 migration up/down。
- 新依赖需先完成 AGPL-3.0 兼容性与复用审计；当前路线默认不新增依赖。
