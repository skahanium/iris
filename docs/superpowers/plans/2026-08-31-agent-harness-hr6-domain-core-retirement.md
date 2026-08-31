# HR-6：领域 current-fact 核心退役实施计划

> **状态：已完成**
> **基线：2026-08-31，提交 `c414894b`**
> **范围：`agent-harness/05-implementation-roadmap.md` 的 HR-6；不连接真实 Provider，不新建 worktree。**

## 目标与非目标

目标是让新 Run 只有一条由 `AgentIntent + Effect + ContextMode + Freshness + Effort + RiskClass + CapabilityId` 驱动的通用执行路径。模型经同一个 `AgentToolLoop` 选择 Web、本地读取、外部只读或运行时工具；Host 继续冻结授权、预算、来源身份与副作用。

本阶段退役的是“当前事实的 11 个领域 operation”及其 `web.domain.read` 快路径，不是泛称为 domain 的所有代码。`domain_executor` 仍负责已授权材料的隔离、范文事实防泄漏与小说上下文边界，属于通用安全层，必须保留。

不新增表、迁移、IPC 字段、Provider 或依赖；不重放旧 Run；不删除用户数据。

## 完成记录（2026-08-31）

- 已删除 `fresh_domains/`、五个领域 lookup、专用 Host 预取/renderer/finalization 与其失效 fixture；新 WebRequired Run 只经现有通用 `AgentToolLoop` 请求 `web_search`。
- 历史 `FreshFactDomain`/`DomainOperation`、migration 072、MCP snapshot 继续只读兼容。活跃旧领域 Run 会在 Provider 前安全失败，设置层拒绝新的领域 binding。
- 已通过 catalog、旧 Run 终态化、写入拒绝、受影响集成测试、格式、clippy 与文档事实检查。类型仍位于 `run_contract`，但注释和唯一调用都限定为 legacy 反序列化/终态化；未创建新路由。

## 现状与兼容结论

- 新 Intake、工具目录、终态路径和 Host 调度都不再以 `FreshFactDomain`/`DomainOperation` 决策新 Run；通用 WebRequired 只通过 `web.search` 与 `AgentToolLoop` 前进。
- migration 072、`mcp_capability_bindings.domain_operation`、冻结快照中的同名字段是历史数据格式，继续可反序列化与展示，但不再参与新 Run 的授权、工具暴露或调度。
- 带旧领域 envelope 的活动 Run 不可恢复执行：恢复/启动时应明确终态化；已终态 Run 只读展示其持久化结果。
- `domain_executor` 与 `fresh_domains` 是不同抽象：前者保留，后者和其专用 renderer/service/contract 删除。

## 实施步骤

### 1. 先建立失败回归

**文件：** `normal_run_service_tests.rs`、`run_context_tests.rs`、`tool_executor.rs` 的测试模块，以及一个最小化的迁移兼容测试。

- [x] 新 WebRequired Run 只使用 `web.search` 和通用 ToolLoop；不会构建 `web.domain.read` 快路径或按 operation 限缩工具面。
- [x] 新 Run 的 direct/ToolLoop/高风险当前事实分别走已有通用终态合同；不因历史 `FreshFactDomain` 值进入领域终态校验。
- [x] 历史 envelope/快照仍能读取；恢复一个非终态旧领域 Run 会确定、安全失败，而非 Provider 重放。
- [x] 工具 catalog 不再含 `weather_lookup`、`news_lookup`、`finance_lookup`、`entertainment_lookup`、`sports_lookup`；普通 `external.read` 快照仍可经通用工具面暴露。
- [x] 静态边界确认 `DomainOperation` 仅保留在历史契约、MCP snapshot 兼容与旧 Run 终态门；`RunContext` 仅保留一条历史提示文案，不再进入新 Run intake、工具表面、Run Engine finalization 或工具 catalog。

先运行这些测试，确认旧实现至少在“专用工具仍被暴露”与“旧活动 Run 可进入执行”两项失败，再改生产实现。

### 2. 收敛新 Run 的调度与终态

**文件：** `normal_run_service.rs`、`run_context.rs`、`run_engine/mod.rs`、`run_engine/finalization.rs`、相关测试。

- [x] 已去掉 `CurrentRunDomain` 作为新决策、`fresh_fact.domain` 的 structured-finalization 条件、确定性领域 Web 预取和专用工具限缩。
- [x] 由现有 `Freshness`、`VerificationRequirement::CurrentRunWeb/CurrentRunExternal`、`RiskClass` 决定通用工具循环和严格来源校验；不为电影、天气、金融等名称分叉。
- [x] `RunContext` 保留通用材料计划，但不再为 Provider prompt 或 Run Engine 传递 fresh-domain plan。
- [x] 终态验证只消费统一 provenance/evidence 合同；已移除 current-fact domain 的专用校验器调用。
- [x] 旧活动领域 Run 通过单一兼容门安全终态，不调用 Provider、不执行工具、不污染新 Run 会话。

### 3. 退役专用工具与实现

**文件：** `fresh_domains/`、`tool_catalog/fresh_domains_impl.rs`、`tool_catalog/groups.rs`、`tool_executor.rs`、`run_contract.rs`、`mcp_external_tools.rs`、相关单测。

- [x] 已从编译图移除 `fresh_domains`、五个专用 lookup 工具、`constrain_domain_tool_surface` 与专用 operation→tool 映射。
- [x] 已删除只为其服务的 renderer、service、provider、fixture、测试和 `web.domain.read` 运行时冻结逻辑。
- [x] 历史 SQLite 值继续以兼容 DTO 解码：未知/旧 operation 只能作为不可执行的历史元数据读取，不能注册成工具或触发领域解析。
- [x] 既有 `external.read` binding 合同继续承载可选结构化外部只读工具；其输出作为不受信任数据进入统一来源账本，不扩展领域状态。
- [x] `FreshFactDomain`/`DomainOperation` 因持久化反序列化仍保留在 `run_contract`；其注释、写入拒绝与唯一 legacy terminalization 调用将其限定为兼容元数据，禁止泄漏回新 Run 核心。

### 4. 清理文档与验收

- [x] 已更新 `agent-harness/02-current-state-and-debt.md`、`03-target-architecture.md`、`04-adaptive-agent-loop-and-tool-contracts.md`、附录 A/C 与 `ROADMAP.md`，写明旧 schema 的只读兼容和已删除的核心路径。
- [x] 静态检查证明新 Run 核心不再依赖 11 operation；保留项均注明 legacy/迁移目的。
- [x] 已运行 HR-6 精确 Rust 测试、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`npm run docs:check` 与 `git diff --check`。

## 验收证据

1. 一个通用 WebRequired mock Run 在没有任何领域枚举/工具的情况下完成并保留统一来源。
2. 通用外部只读工具能经过普通 capability 快照执行；五个旧 lookup 名称不可再获得。
3. 旧完成 Run 可读，旧非终态领域 Run 明确失败且不发生网络/Provider 调用。
4. 除历史 DTO/MCP snapshot 兼容、`RunContext` 的旧 Run 提示文案与旧 Run terminalization 外，生产核心不以 `FreshFactDomain`、`DomainOperation`、`CurrentRunDomain` 或 `web.domain.read` 决策新 Run。
5. 删除量为净减少，且不存在替代性平行路由。
