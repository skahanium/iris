# Iris Agent Harness 重构文档集

> 状态：**目标规格，尚未实施**  
> 适用范围：Iris Agent Harness 及其直接依赖的 AI 面板、IPC、持久化和策略边界  
> 不代表 `ARCHITECTURE.md` 中的当前实现事实

本目录是 Agent Harness 重构的唯一施工规格。后续 AI 或开发者不得只依据某一份专题文档施工；必须先阅读本页，再按当前阶段读取对应规格。

## 1. 文档导航

| 顺序 | 文档                                                               | 解决的问题                                          |
| ---- | ------------------------------------------------------------------ | --------------------------------------------------- |
| 1    | [01-product-contract.md](./01-product-contract.md)                 | 产品目标、用户体验、核心用途和明确非目标            |
| 2    | [02-target-architecture.md](./02-target-architecture.md)           | 唯一控制平面、Execution Envelope、状态机和路由原则  |
| 3    | [03-lifecycle-and-evidence.md](./03-lifecycle-and-evidence.md)     | Conversation、Turn、Run、checkpoint、证据和保留策略 |
| 4    | [04-policy-and-security.md](./04-policy-and-security.md)           | 文档权限、遵循等级、写回确认、涉密域和提示注入防护  |
| 5    | [05-capability-system.md](./05-capability-system.md)               | Tools、Skills、MCP、联网和 LLM Provider 的统一边界  |
| 6    | [06-domain-behavior.md](./06-domain-behavior.md)                   | 公文写作、工作分析、小说写作的材料使用合同          |
| 7    | [07-api-and-data-migration.md](./07-api-and-data-migration.md)     | 新 IPC、事件协议、目标数据模型和一次性迁移          |
| 8    | [08-implementation-plan.md](./08-implementation-plan.md)           | 可逐阶段执行的施工顺序、依赖、退出条件和提交边界    |
| 9    | [09-verification-and-rollout.md](./09-verification-and-rollout.md) | 测试矩阵、评测、性能 SLO、发布和回滚门槛            |
| 10   | [10-ai-construction-playbook.md](./10-ai-construction-playbook.md) | 后续 AI 的阅读、编码、验证、汇报和停止规则          |

## 2. 规范优先级

发生冲突时按以下顺序处理：

1. `AGENTS.md` 的硬约束、安全和施工纪律。
2. `ROADMAP.md` 的产品边界与版本事实。
3. 本目录的产品合同和安全合同。
4. 本目录的架构、接口与数据规格。
5. 本目录的实施顺序和施工建议。
6. 现有代码；现有代码只能证明当前状态，不能推翻目标规格。

任何实现发现上层规格无法同时满足时，必须停止该部分施工并请求用户裁决，不得自行选择一种解释。

## 3. 已锁定决策

- 一个对话入口、一个 Run 状态机、一个权限策略引擎、一个普通域证据账本。
- 当前编辑器文档与 Agent Session 在结构上硬解绑。
- 前端不再通过互斥 scene/intent 选择工作流。
- 普通用户不需要理解 Execution Envelope、Skill、MCP 或权限原子。
- 公文主要学习范文样本的结构与表达；工作分析必须重视规范依据。
- 小说不得自动读取仓库文档，所有文档资料必须由用户明确 `@` 或显式编辑器动作提供。
- 课题研究场景不在本次范围，旧专用入口退出新 Harness。
- 只读执行可自动升级；写回以“变更计划”为确认单位。
- 联网关闭即硬离线；联网开启代表积极使用，而不只是被动授权。
- Skills 保持 prompt-only；MCP 只能经类型化 capability adapter 接入。
- 简单问答采用单主调用；规划、反思和验证按风险自适应触发。
- 旧执行入口在同一次切换中删除，不保留双写或第二套生命周期。

## 4. 严格范围

### 本次包含

- Rust Harness 控制平面、状态机和领域 executor 接口。
- AI IPC、统一事件流和前端对话面板接入。
- Agent 会话、Run、证据、权限和安全 checkpoint 的数据库迁移。
- Tool、Skill、MCP/Web、Provider 与 Harness 的连接边界。
- 与 Agent 直接相关的涉密会话结构和流转约束。
- 测试、评测、性能观测和发布门槛。

### 本次不包含

- 重写 RAG 排序、向量模型、知识图谱或 Markdown 编辑器。
- 通用插件 API、插件市场或任意代码执行能力。
- 课题研究专用工作流、ResearchState、研究 UI 或自动研究 Agent。
- 重新设计加密算法和凭据存储。
- 当前文档与 Session 的任何隐式绑定。

## 5. 完成定义

只有同时满足以下条件，Harness 重构才可以声称完成：

- 所有生产请求只经过新 Run 状态机。
- 旧执行 IPC、旧前端路由和旧恢复入口已删除。
- 数据迁移和 down 回滚测试通过，旧消息与已完成结果可读取。
- 未授权写入、隐式当前文档读取和跨安全域泄漏的测试结果为零。
- 公文、工作分析、小说和普通问答的行为评测达到门槛。
- 严格关键路径性能 SLO 达标。
- `AGENTS.md` 要求的完整 Rust、TypeScript、测试、E2E 和审计命令全部通过。
