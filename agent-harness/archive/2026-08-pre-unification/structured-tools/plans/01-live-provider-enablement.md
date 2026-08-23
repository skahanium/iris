# 结构化领域 Provider 生产启用 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让五个外部领域工具的 11 个 operation 具备 operation-specific readiness、真实 Provider 配置、健康路由和双重验收。

**Architecture:** 复用现有 MCP provider registry、capability binding、Run snapshot、WebEvidenceBroker 和 evidence ledger。readiness 由现有数据库事实派生；Run 只暴露并冻结本轮可执行 operation；Provider 输出经白名单 mapping 和 DTO validator 后登记现有 evidence，再由 Host/结构化协议最终化。若 07 OD-002 允许 REST adapter，则新增 adapter 但仍复用现有 registry、snapshot、evidence 和 dispatch 边界。

**Tech Stack:** Rust、SQLite migration 072、Tauri 2、React 19、现有 MCP host、Vitest、Rust tests；不新增第三方依赖。

## Global Constraints

- 不创建 worktree；不新增第二套 Provider registry、evidence 仓库或健康真相源。
- 是否允许新增数据库表/列/migration，由 `04-implementation-roadmap.md` 决策门 1/3 和 `07-provider-landing-and-decision-process.md` 确认；**未确认前不得新增 schema**。
- 不硬编码 Provider endpoint、API key、token 或商业服务商。
- 普通 `web.search/web.fetch` mapping 不能自动升级为 `web.domain.read`。
- operation 是 readiness、授权、snapshot、dispatch 和验收的最小单位。
- Provider 原始参数、输出、transport 和凭证不得进入持久化诊断。
- News 保留 Web fallback；其他领域没有合规结构化记录时失败关闭。
- 每项先写失败测试，只运行定向测试；最终阶段再运行完整质量门。
- 每个 Task 使用独立中文 Conventional Commit。
- 施工前必须阅读 `07-provider-landing-and-decision-process.md`；遇到 DECISION REQUIRED 必须停下提问。

---

### Task 1：建立 operation-specific readiness

**Files:**

- Modify: `src-tauri/src/ai_runtime/fresh_domains/provider.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/tests.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `docs/ipc-api-reference.md`
- If 决策门 1 选择 A/B：Create `src-tauri/migrations/073_agent_domain_operation_readiness.sql` + `.down.sql`，并 Modify `src-tauri/src/storage/migrate.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DomainReadinessState {
    Unconfigured,
    NeedsReview,
    Unhealthy,
    Ready,
    WebFallback,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainOperationReadiness {
    pub(crate) operation: DomainOperation,
    pub(crate) state: DomainReadinessState,
    pub(crate) eligible_provider_ids: Vec<String>,
    pub(crate) reason_code: Option<String>,
}
```

- [ ] **决策门 1（必须先确认）**：operation 级 readiness/preview 如何持久化？见 `04-implementation-roadmap.md` 阶段 1 决策门 1。未确认前不要写 readiness IPC 实现。

- [ ] **Step 1：写失败测试**

```rust
#[test]
fn domain_readiness_requires_an_operation_specific_binding() { /* weather 不使 finance Ready */ }

#[test]
fn domain_readiness_rejects_disabled_drifted_and_untrusted_bindings() { /* 均非 Ready */ }

#[test]
fn domain_readiness_marks_news_as_web_fallback_without_binding() { /* Web 可用时 */ }

#[test]
fn domain_readiness_uses_persisted_health_instead_of_the_healthy_label() { /* 熔断候选排除 */ }
```

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain_readiness -- --nocapture
```

Expected: FAIL，readiness 接口不存在。

- [ ] **Step 2：实现派生和只读 IPC**

从现有 Provider、binding、health 派生状态。若决策门 1 选择 A/B，先落地 migration 和 `migrate.rs` 再实现 IPC。新增 `domain_operation_readiness_list` command，只返回 operation、状态、安全 reason code 和稳定 Provider ID，不返回 transport 或 credential refs。同步 IPC 类型与参考文档。

- [ ] **Step 3：验证并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain_readiness -- --nocapture
npm run typecheck
git add src-tauri/src/ai_runtime/fresh_domains/provider.rs src-tauri/src/ai_runtime/mcp_runtime_registry.rs src-tauri/src/ai_runtime/fresh_domains/tests.rs src-tauri/src/commands/ai_commands.rs src-tauri/src/lib.rs src/types/ipc.ts src/lib/ipc.ts docs/ipc-api-reference.md
git commit -m "feat(ai): 建立领域服务可用性事实"
```

### Task 2：使授权和工具表面精确到 operation

**Files:**

- Modify: `src-tauri/src/ai_runtime/capability_resolver.rs`
- Modify: `src-tauri/src/ai_runtime/run_intake.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Modify: `src-tauri/src/ai_runtime/tool_surface.rs`
- Modify: `src-tauri/src/ai_runtime/tool_executor.rs`
- Modify: `src-tauri/src/ai_runtime/run_intake_tests.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/tests.rs`

**Interfaces:**

```rust
pub(crate) struct DomainToolGrant {
    pub(crate) operation: DomainOperation,
    pub(crate) tool_name: &'static str,
    pub(crate) route: DomainReadinessState,
}
```

- [ ] **Step 1：写失败测试**

```rust
#[test]
fn weather_binding_never_authorizes_finance_tool() { /* surface 只含 weather */ }

#[test]
fn unconfigured_entertainment_is_not_advertised_as_callable() { /* capabilities 不显示 */ }

#[test]
fn news_web_fallback_surfaces_only_news_lookup_and_web_search() { /* 不开放其他领域 */ }
```

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml operation_specific_surface -- --nocapture
```

Expected: FAIL，当前仍使用粗粒度 capability。

- [ ] **Step 2：实现 grant、snapshot 和 dispatch 复核**

intake 根据冻结 operation 生成 grant；orchestrator 只将 grant 对应工具加入 surface；dispatch 校验参数 operation、grant 和 snapshot 一致。News WebFallback 不生成伪造 MCP snapshot。

- [ ] **Step 3：验证并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml operation_specific_surface -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml run_intake -- --nocapture
git add src-tauri/src/ai_runtime/capability_resolver.rs src-tauri/src/ai_runtime/run_intake.rs src-tauri/src/ai_runtime/normal_run_service.rs src-tauri/src/ai_runtime/tool_surface.rs src-tauri/src/ai_runtime/tool_executor.rs src-tauri/src/ai_runtime/run_intake_tests.rs src-tauri/src/ai_runtime/tool_catalog/tests.rs
git commit -m "fix(ai): 按领域操作冻结真实工具表面"
```

### Task 3：增加真实 mapping 预览和管理中心矩阵

**Files:**

- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_external_tools.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_host_runtime.rs`
- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `src/components/ai/skills/McpProviderDetail.tsx`
- Modify: `src/components/ai/skills/mcpProfileParsers.ts`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `docs/ipc-api-reference.md`
- If 决策门 3 允许 coverage 元数据或 schema 变更：Create 对应 migration + `.down.sql`，并 Modify `src-tauri/src/storage/migrate.rs`
- Modify: `tests/mcp-domain-capability-mapping.test.tsx`
- Modify: `tests/mcp-profiles-diagnostics.test.tsx`

**Interfaces:**

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainBindingValidationResult {
    pub operation: DomainOperation,
    pub mapped_record_count: usize,
    pub readiness: DomainReadinessState,
    pub checked_at: String,
    pub safe_error_code: Option<String>,
}
```

- [ ] **决策门 3（必须先确认）**：预览结果如何持久化？多 Provider 覆盖同一 operation 时如何展示？是否允许 coverage 元数据？见 `04-implementation-roadmap.md` 阶段 3 决策门 3。

- [ ] **Step 1：写预览门失败测试**

覆盖非法路径、缺字段、陈旧时间、HTTP 来源、空 records、timeout 和敏感哨兵。未通过真实预览的 binding 不能显示 Ready。

```bash
npm run test -- tests/mcp-domain-capability-mapping.test.tsx tests/mcp-profiles-diagnostics.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml domain_binding_validation -- --nocapture
```

Expected: FAIL，当前保存流程没有真实记录预览门。

- [ ] **Step 2：实现受限预览和矩阵**

由用户显式触发只读 Provider 调用；Host 使用现有超时和预算，在内存完成 mapping/validation，只返回计数、时间和安全码。UI 展示当前支持矩阵内 operation 的未配置/待验证/可用/降级/不健康/WebFallback（仅 News）状态。

- [ ] **Step 3：验证并提交**

```bash
npm run test -- tests/mcp-domain-capability-mapping.test.tsx tests/mcp-profiles-diagnostics.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml domain_binding_validation -- --nocapture
npm run typecheck
git add src-tauri/src/commands/ai_commands.rs src-tauri/src/ai_runtime/mcp_external_tools.rs src-tauri/src/ai_runtime/mcp_host_runtime.rs src/components/ai/skills/McpProfilesPanel.tsx src/components/ai/skills/McpProviderDetail.tsx src/components/ai/skills/mcpProfileParsers.ts src/types/ipc.ts src/lib/ipc.ts docs/ipc-api-reference.md tests/mcp-domain-capability-mapping.test.tsx tests/mcp-profiles-diagnostics.test.tsx
git commit -m "feat(ai): 验证并展示领域服务真实状态"
```

### Task 4：统一健康、冻结备用和技术重试

**Files:**

- Modify: `src-tauri/src/ai_runtime/fresh_domains/provider.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/service.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_runtime_registry.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_external_tools.rs`
- Modify: `src-tauri/src/ai_runtime/run_intake_tests.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/tests.rs`

- [ ] **决策门 4（必须先确认）**：健康事实是 provider 级还是 operation 级？候选排序和重试预算是否按 `04-implementation-roadmap.md` 阶段 4 的默认方案？未确认前不要实现 failover。

- [ ] **Step 1：写失败测试**

```rust
#[test]
fn unhealthy_primary_is_not_frozen_ahead_of_ready_backup() { /* 健康排序 */ }

#[tokio::test]
async fn transient_primary_failure_uses_only_frozen_backup() { /* 不发现新 Provider */ }

#[tokio::test]
async fn non_news_all_provider_failure_never_masquerades_as_web_success() { /* fail closed */ }

#[tokio::test]
async fn provider_retry_does_not_consume_business_search_round() { /* 预算分离 */ }
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain_provider_failover -- --nocapture
```

Expected: FAIL，健康事实和候选冻结尚未统一。

- [ ] **Step 2：实现有序候选和调用记账**

每个 operation 冻结最多三个候选；顺序来自用户优先路由和 readiness。一次调用最多尝试三个候选；单 Provider 瞬时故障最多重试一次。所有尝试更新现有 health，不增加业务搜索轮次。

- [ ] **Step 3：验证并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain_provider_failover -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fresh_domains -- --nocapture
git add src-tauri/src/ai_runtime/fresh_domains/provider.rs src-tauri/src/ai_runtime/fresh_domains/service.rs src-tauri/src/ai_runtime/mcp_runtime_registry.rs src-tauri/src/ai_runtime/mcp_external_tools.rs src-tauri/src/ai_runtime/run_intake_tests.rs src-tauri/src/ai_runtime/fresh_domains/tests.rs
git commit -m "fix(ai): 按健康状态冻结领域服务候选"
```

### Task 5：逐 operation 配置并验证真实 Provider

**Files:**

- Modify: `structured-tools/06-instance-readiness-record.md`
- Create/Update: `structured-tools/07-provider-landing-and-decision-process.md`（PDR 与 Decision Log）

**Interfaces:**

- Consumes: Task 3 的真实预览、Task 4 的 readiness/health 和现有 MCP 管理中心。
- Produces: 当前实例每个受支持 operation 的 Binding、Preview、Health 和 Production Run 证据，以及已确认的 PDR。

- [ ] **决策门 5（每个 operation 强制，必须先确认）**

在开始任何一个 operation 前，完成 `07-provider-landing-and-decision-process.md` 中的 PDR，并由设计者确认。PDR 未确认，禁止进入 mapping、preview 或 production run。

PDR 必须回答：

- Provider 是 MCP 还是 REST？如果只有 REST，是否允许新增 adapter？
- 该 Provider 覆盖哪些子范围？未覆盖范围如何处置？
- 是否需要多个 Provider 覆盖同一 operation？覆盖矩阵是什么？
- mapping 是否能完整映射到 DTO？是否需要 schema/coverage 扩展？
- 真实预览用什么安全公开参数？预期返回什么？
- `news.search` 是否接受 WebFallback 作为完成形态，还是必须接入结构化 Provider？（见 07 OD-006）
- 如果没有合规 Provider，该 operation 是否从支持矩阵移除？

- [ ] **Step 1：Provider 调研与 PDR 起草（不写业务代码）**

对每个 operation，先做只读调研：

- 检查现有 MCP tool inventory 是否有明确领域只读工具；
- 若没有，调研外部 MCP Server 或 REST API 候选；
- 普通 `web_search`、`web_fetch` 不作为候选；
- 不硬编码 endpoint、API key 或凭证；
- 把调研结果写入 PDR，不进入实现。

- [ ] **Step 2：设计者确认覆盖矩阵与支持矩阵**

根据 PDR 和 `03-target-architecture.md` 5.1 覆盖模型，与设计者确认：

- 单 Provider 完整覆盖，还是多 Provider 子集覆盖；
- 未覆盖范围是 Unavailable，还是收缩支持矩阵；
- 是否需要新增 coverage 元数据或 schema 变更（必须与决策门 1/3 一致）。

确认前不得保存 mapping。

- [ ] **Step 3：逐 operation 建立 mapping**

按以下顺序分别保存 input/output mapping，不跨 operation 复用未经验证的字段：

```text
weather.current
weather.forecast
news.search
finance.quote
finance.metrics
finance.news
entertainment.now_playing
entertainment.upcoming
entertainment.streaming
sports.schedule
sports.score
```

Expected: 保存后状态只能是 NeedsReview，不能直接是 Ready。

- [ ] **Step 4：逐 operation 执行真实预览和健康探测**

每项使用非敏感公开参数触发一次预览，确认必需字段、时效、地域、单位和 HTTPS 来源均通过。失败项保持 NeedsReview/Unhealthy，修正 mapping 或更换 Provider；不得放宽 validator。

- [ ] **Step 5：执行真实 Production Run**

分别执行当前支持矩阵内领域（天气、新闻、金融、娱乐、体育中的受支持项）的真实场景，并验证最终消息来源、时间、地域及恢复。一个领域有多个 operation 时逐项执行，不能以领域中的一次成功代替其他 operation。

- [ ] **Step 6：更新实例记录与 Decision Log**

只在证据完成后更新 `structured-tools/06-instance-readiness-record.md` 的非敏感状态字段。没有合规 Provider 的 operation 保持 Unconfigured，并从当前支持声明中移除。同时把 PDR 决策写入 `07` 的 Decision Log。

```bash
git add structured-tools/06-instance-readiness-record.md structured-tools/07-provider-landing-and-decision-process.md
git commit -m "docs(ai): 记录结构化服务实例验收"
```

### Task 6：完成受支持 operation 软件生产门禁（目标 11 个）

**Files:**

- Modify: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs`
- Modify: `docs/eval/agent-answer-capacity.md`
- Create: `docs/testing/structured-domain-provider-readiness.md`

- [ ] **前置条件**：只有已确认 PDR 且属于当前支持矩阵的 operation 才能计入 production 测试；没有 PDR 或已从支持矩阵移除的 operation 必须先从 Task 5 补齐或明确排除。

- [ ] **Step 1：增加受支持 operation 的 production 测试（目标 11 个）**

使用 `05-evaluation-and-acceptance.md` 中的固定测试名；若决策门 6 缩减支持矩阵，则只保留支持矩阵内 operation 对应的测试。每个测试都从 intake 走到 fixture（结构化为 MCP/REST snapshot，News WebFallback 为 WebEvidenceBroker fixture）、DTO validation、evidence、最终消息和恢复。

```bash
cargo test --manifest-path src-tauri/Cargo.toml production_ -- --nocapture
```

Expected before implementation: 至少一个 operation 因生产链断点失败。

- [ ] **Step 2：只修复共享生产链**

不为单个 operation 建旁路。Provider evidence ID 必须由数据库生成；Host 最终内容只使用验证后的 DTO；恢复不能重执行 Provider。

- [ ] **Step 3：增加实例人工清单并提交**

人工清单要求管理中心 readiness、五领域真实请求、schema 版本和安全诊断均通过，但不记录凭证或原始输出。

```bash
cargo test --manifest-path src-tauri/Cargo.toml production_ -- --nocapture
npm run agent:eval:smoke
git add src-tauri/src/ai_runtime/normal_run_service_tests.rs src-tauri/src/ai_runtime/run_tool_loop.rs src-tauri/src/ai_runtime/agent_capacity_eval.rs src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs docs/eval/agent-answer-capacity.md docs/testing/structured-domain-provider-readiness.md
git commit -m "test(ai): 验收十一类结构化领域操作"
```

### Task 7：升级、错误语义与状态收口

**Files:**

- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src/components/ai/hooks/useUnifiedAssistantSend.ts`
- Modify: `tests/use-unified-assistant-send.test.tsx`
- Modify: `ARCHITECTURE.md`
- Modify: `structured-tools/01-current-state-and-evidence.md`
- Modify: `structured-tools/02-gap-register.md`
- Modify: `structured-tools/04-implementation-roadmap.md`
- Modify: `structured-tools/05-evaluation-and-acceptance.md`
- Modify: `structured-tools/06-instance-readiness-record.md`

- [ ] **Step 1：写旧库升级和错误投影测试**

覆盖 059→072、重复 migration、不伪造领域 binding，以及未配置、待验证、不健康、歧义和全部失败的可操作中文错误。

```bash
cargo test --manifest-path src-tauri/Cargo.toml migration_072 -- --nocapture
npm run test -- tests/use-unified-assistant-send.test.tsx
```

Expected before implementation: 至少一个升级或错误语义断言失败。

- [ ] **Step 2：完成最小实现和完整门禁**

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

不运行 live API eval。

- [ ] **Step 3：执行实例门禁并更新状态**

只有软件门禁和当前实例门禁都通过，才能把 DOM-AVAIL、DOM-HEALTH、DOM-SURFACE、DOM-ROUTE、DOM-LIVE、DOM-UPGRADE、DOM-DECISION、DOM-PROVIDER 和 DOM-COVERAGE 标记 Resolved。

```bash
git add src-tauri/src/storage/migrate.rs src/components/ai/hooks/useUnifiedAssistantSend.ts tests/use-unified-assistant-send.test.tsx ARCHITECTURE.md structured-tools
git commit -m "fix(ai): 对齐结构化工具生产事实与升级错误语义"
```

## 完成定义

- 软件门禁：受支持 operation 的 production matrix（目标 11 个）和完整质量检查全部通过。
- 实例门禁：当前实例受支持 operation 都是 Operational，五领域真实场景完成。
- 未找到合规 Provider 的 operation 必须保持 Unconfigured 并从支持声明中移除，不能通过放宽 validator 获得完成状态。
