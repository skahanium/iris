# 六类常用当前事实能力 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 在不建设万能数据平台的前提下，为时间、天气、新闻、金融、影视和体育提供稳定只读工具、统一 DTO、确定性验证和低配置 provider 选择。

**Architecture:** 时间继续使用本机 runtime；另外五类工具以稳定 Iris schema 暴露给模型。执行优先使用经过审核并冻结的领域 MCP mapping，否则复用 WebEvidenceBroker；两条路径都先规范化为附录 D DTO，再进入当前 Run evidence/finalization。通用 `external.read` 的逐 Run 授权保持不变，领域映射使用独立 `web.domain.read` capability 并仅受 Web 开关授权。

**Tech Stack:** Rust、SQLite migration 072、Tauri IPC、React 19 管理中心、现有 MCP host/credential store/WebEvidenceBroker。

**Status:** Partial；DTO、目录和基础 Provider mapping 已落地，但生产 intake、结构化证据登记、领域终局渲染、缺参恢复和统一 Provider failover 尚未闭环。必须继续执行 [`plans/04-current-fact-production-closure.md`](04-current-fact-production-closure.md) 后才能标记 Completed。

**Dependencies:** 依赖计划 02 的 `FreshFactPolicy`、`EvidenceGap`、研究预算和当前事实终局接口稳定；不得并行发明第二套分类或最终化协议。

## Global Constraints

- 不创建 worktree，不新增第三方依赖，不硬编码 provider endpoint、API key 或商业服务商。
- 不新增 provider registry、证据表、地点表或管理中心；复用现有实体。
- `external.read` 不得自动授权；只有 `web.domain.read` 的已审核稳定 operation 在 Web 开启时可自动冻结。
- 所有 provider 输出按白名单 JSON path 映射，不执行脚本、模板代码或 provider 返回的指令。
- 每个领域任务先写失败测试；只运行定向测试，最终 PR 收口再运行全量门禁。
- Commit 使用中文 Conventional Commit。

---

### Task 1: 增加领域 binding/snapshot schema

**Files:**

- Create: `src-tauri/migrations/072_agent_domain_capability_mappings.sql`
- Create: `src-tauri/migrations/072_agent_domain_capability_mappings.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_external_tools.rs`
- Modify: `src/lib/ipc.ts`
- Modify: `src/types/ipc.ts`

**Interfaces:**

- Consumes: 059 的 `mcp_capability_bindings`、`agent_run_mcp_tool_snapshots`。
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DomainOperation {
    #[serde(rename = "weather.current")]
    WeatherCurrent,
    #[serde(rename = "weather.forecast")]
    WeatherForecast,
    #[serde(rename = "news.search")]
    NewsSearch,
    #[serde(rename = "finance.quote")]
    FinanceQuote,
    #[serde(rename = "finance.metrics")]
    FinanceMetrics,
    #[serde(rename = "finance.news")]
    FinanceNews,
    #[serde(rename = "entertainment.now_playing")]
    EntertainmentNowPlaying,
    #[serde(rename = "entertainment.upcoming")]
    EntertainmentUpcoming,
    #[serde(rename = "entertainment.streaming")]
    EntertainmentStreaming,
    #[serde(rename = "sports.schedule")]
    SportsSchedule,
    #[serde(rename = "sports.score")]
    SportsScore,
}
```

`McpCapabilityBindingInput/Summary` 增加：

```ts
domainOperation?: DomainOperation;
outputMapping?: DomainOutputMapping;
```

`DomainOutputMapping` 只允许：

```ts
interface DomainOutputMapping {
  recordsPath: string;
  fields: Record<string, string>;
}
```

path 只支持 `$`、点属性和非负数组下标；禁止表达式、过滤器、函数和递归 descent。

- [x] **Step 1: 写 migration up/down 失败测试**

在 `migrate.rs` 增加测试：升级后两张表含 `domain_operation`、`output_mapping_json`；同 provider/operation 重复映射被唯一索引拒绝；down 后恢复 059 列集合和 `external.read` CHECK。

- [x] **Step 2: 编写 072 up migration**

SQLite 重建两张表，把 capability CHECK 改为：

```sql
CHECK (capability IN ('external.read', 'web.domain.read'))
```

新增：

```sql
domain_operation   TEXT,
output_mapping_json TEXT NOT NULL DEFAULT '{}',
CHECK (
  (capability = 'external.read' AND domain_operation IS NULL) OR
  (capability = 'web.domain.read' AND domain_operation IS NOT NULL)
)
```

建立 partial unique index：

```sql
CREATE UNIQUE INDEX idx_mcp_domain_binding_provider_operation
ON mcp_capability_bindings(provider_id, domain_operation)
WHERE domain_operation IS NOT NULL;
```

Run snapshot 同步保存 capability、operation 和 output mapping；旧数据复制为 `external.read/NULL/{}`。

- [x] **Step 3: 编写 down migration**

先删除领域 snapshot/binding，再以 059 原 schema 重建并只复制 `capability='external.read'` 的行，恢复原索引。Down 不得留下违反旧 CHECK 的数据。

- [x] **Step 4: 扩展 Rust/TypeScript DTO 与 hash**

binding hash 和 snapshot integrity hash 必须包含 `domain_operation` 与规范化后的 `output_mapping_json`；字段变化使旧 snapshot/config 失效。

- [x] **Step 5: 运行 migration 和 binding 测试**

```bash
cargo test --manifest-path src-tauri/Cargo.toml migration_072 -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml mcp_external_tools -- --nocapture
npm run typecheck
```

Expected: PASS。

- [x] **Step 6: 提交 schema**

```bash
git add src-tauri/migrations/072_agent_domain_capability_mappings.sql src-tauri/migrations/072_agent_domain_capability_mappings.down.sql src-tauri/src/storage/migrate.rs src-tauri/src/ai_runtime/mcp_external_tools.rs src/lib/ipc.ts src/types/ipc.ts
git commit -m "feat(ai): 扩展领域服务商冻结映射"
```

### Task 2: 建立统一领域 DTO 与验证器

**Files:**

- Create: `src-tauri/src/ai_runtime/fresh_domains/mod.rs`
- Create: `src-tauri/src/ai_runtime/fresh_domains/contracts.rs`
- Create: `src-tauri/src/ai_runtime/fresh_domains/validation.rs`
- Create: `src-tauri/src/ai_runtime/fresh_domains/tests.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`

**Interfaces:**

- Consumes: 附录 D、Task 1 的 `DomainOperation`。
- Produces:

```rust
pub(crate) struct EvidenceOrigin {
    pub(crate) evidence_id: i64,
    pub(crate) provider_id: String,
    pub(crate) source_url: String,
    pub(crate) source_title: String,
    pub(crate) observed_at: String,
}

pub(crate) enum FreshDomainRecord {
    Weather(WeatherRecord),
    News(NewsRecord),
    Finance(FinanceRecord),
    Entertainment(EntertainmentRecord),
    Sports(SportsRecord),
}

pub(crate) fn validate_domain_record(
    operation: DomainOperation,
    requested_at: chrono::DateTime<chrono::Utc>,
    record: &FreshDomainRecord,
) -> AppResult<()>;
```

每个 record 的必需字段与阈值严格来自附录 D；所有用户可见字符串设置现有字符预算，列表遵守工具 `max_results`。

- [x] **Step 1: 写各领域最小成功夹具**

每个 operation 至少一个通过夹具，固定 `requested_at=2026-08-18T08:00:00Z`。断言有效记录通过且保留 origin。

- [x] **Step 2: 写字段/时效拒绝表**

至少覆盖：天气 observation 超 3 小时、forecast issue 超 12 小时、新闻无 publishedAt、金融无 currency/asOf、影视无 region/channel/date、live score checkedAt 超 15 分钟、所有领域无 HTTPS source。

- [x] **Step 3: 写金融分析输入限制测试**

分析层只接受已验证 `FinanceRecord` ID 列表；输出中出现未在输入记录出现的数字时返回 `finance_analysis_unsupported_number`。

- [x] **Step 4: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_domains -- --nocapture
```

Expected: FAIL，模块尚不存在。

- [x] **Step 5: 实现 DTO 和确定性验证**

时间解析失败、单位未知、URL 非 HTTPS、领域 variant 与 operation 不符都返回稳定安全码。验证器不调用模型、不访问网络、不写数据库。

- [x] **Step 6: 运行测试并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_domains -- --nocapture
git add src-tauri/src/ai_runtime/fresh_domains src-tauri/src/ai_runtime/mod.rs
git commit -m "feat(ai): 建立当前事实领域契约与验证"
```

### Task 3: 注册五个稳定 Iris 工具

**Files:**

- Create: `src-tauri/src/ai_runtime/tool_catalog/fresh_domains.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/groups.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/capability.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/tests.rs`
- Create: `src-tauri/src/ai_runtime/tool_dispatch/fresh_domains.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch_impl.rs`
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
- Modify: `src/lib/tool-display-names.ts`

**Interfaces:**

- Consumes: Task 2 DTO/validator。
- Produces tools：

```text
weather_lookup       { operation, location?, days? }
news_lookup          { topic?, location?, start?, end?, limit? }
finance_lookup       { operation, instrument, assetKind? }
entertainment_lookup { operation, title?, location?, channel? }
sports_lookup        { operation, competition?, participant?, date? }
```

全部 `ToolAccessLevel::Network`、无需变更确认、`web.domain.read` capability、`bounded_packets` output policy 和 `current_run_domain` evidence policy。

- [x] **Step 1: 写 catalog 与 surface 失败测试**

断言五个工具名称唯一、schema `additionalProperties=false`、实现为 Dispatchable、Web 关闭时均不进入 surface、伪造调用到不了 dispatch。

- [x] **Step 2: 写运行时参数负例**

覆盖 unknown operation、limit/days 超预算、finance 缺 instrument、天气/附近影院缺城市、日期不在冻结窗口。

- [x] **Step 3: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml tool_catalog -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fresh_domain_tool -- --nocapture
```

- [x] **Step 4: 注册目录和 dispatch**

dispatch 只把规范化参数传给 Task 4 的 `FreshDomainService`；不能直接拼 provider 请求。`capabilities_read` 通过现有 surface 逻辑自然报告工具，不增加特例。

- [x] **Step 5: 同步权限和前端名称**

权限映射为 `web.domain.read`，其授权判断要求 frozen envelope Web 开启；不能映射成 `external.read` 或 `vault.search`。

- [x] **Step 6: 运行测试并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml tool_catalog -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml fresh_domain_tool -- --nocapture
npm run typecheck
git add src-tauri/src/ai_runtime/tool_catalog/fresh_domains.rs src-tauri/src/ai_runtime/tool_catalog/groups.rs src-tauri/src/ai_runtime/tool_catalog/capability.rs src-tauri/src/ai_runtime/tool_catalog/tests.rs src-tauri/src/ai_runtime/tool_dispatch/fresh_domains.rs src-tauri/src/ai_runtime/tool_dispatch_impl.rs src-tauri/src/ai_runtime/agent_permissions.rs src/lib/tool-display-names.ts
git commit -m "feat(ai): 注册五类当前事实只读工具"
```

### Task 4: 实现 provider 解析、冻结和通用 Web fallback

**Files:**

- Create: `src-tauri/src/ai_runtime/fresh_domains/provider.rs`
- Create: `src-tauri/src/ai_runtime/fresh_domains/service.rs`
- Modify: `src-tauri/src/ai_runtime/capability_resolver.rs`
- Modify: `src-tauri/src/ai_runtime/mcp_external_tools.rs`
- Modify: `src-tauri/src/ai_runtime/run_intake.rs`
- Modify: `src-tauri/src/ai_runtime/run_contract.rs`
- Modify: `src-tauri/src/ai_runtime/run_contract_tests.rs`
- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/tests.rs`

**Interfaces:**

- Consumes: `DomainOperation`、健康 provider/binding、WebEvidenceBroker、当前 Run snapshot。
- Produces:

```rust
pub(crate) enum DomainProviderRoute {
    FrozenMcp(FrozenMcpToolSnapshot),
    WebEvidence,
}

pub(crate) fn resolve_domain_provider(
    db: &Database,
    operation: DomainOperation,
    selected_web_provider_id: Option<&str>,
) -> AppResult<DomainProviderRoute>;

pub(crate) struct FreshDomainService;

impl FreshDomainService {
    pub(crate) async fn execute(
        &self,
        request: FreshDomainRequest,
        context: &ToolDispatchContext,
    ) -> AppResult<Vec<FreshDomainRecord>>;
}
```

- [x] **Step 1: 写 provider 选择顺序测试**

覆盖：显式 operation 选择优先；当前 Web provider 映射次之；唯一健康映射自动选；多个未选择时不按名称/更新时间静默挑选而走 Web fallback；provider disabled/hash drift 拒绝。

- [x] **Step 2: 写 generic external 隔离测试**

只有 `external.read` 的 binding 不能被领域 resolver 自动冻结；`web.domain.read` 也不能出现在 Composer 普通外部工具选择列表。

- [x] **Step 3: 写通用 Web fallback 测试**

没有结构化映射时，service 使用 WebEvidenceBroker 的绝对 query；只有能解析并验证附录 D 必需字段的记录才返回。结果不足返回 `agent_run_fresh_evidence_insufficient`。

- [x] **Step 4: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain_provider -- --nocapture
```

- [x] **Step 5: 实现确定性选择与 snapshot**

Run intake 只为 envelope 所需 operation 冻结映射；冻结逻辑复用现有 provider enablement、config hash、launch hash、credential refs 和 schema 校验。运行时仍做 live disable/hash 撤销检查。

同时为 `VerificationRequirement` 增加向后兼容的 `CurrentRunDomain`：结构化 MCP 结果必须以现有 `external_tool` registration source 登记，Web fallback 继续以 `web_search` 登记；两者都只有在 Task 2 validator 通过后才满足领域验证。旧 `CurrentRunWeb` 和 `CurrentRunExternal` 语义不变。

- [x] **Step 6: 实现白名单 output mapping**

按 `recordsPath/fields` 从 JSON 读取字符串、数字、布尔和数组标量；字段类型不符、路径不存在或输出超预算时拒绝。原始 provider JSON 不进入事件、审计或 UI。

- [x] **Step 7: 运行测试并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain_provider -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml web_evidence_broker -- --nocapture
git add src-tauri/src/ai_runtime/fresh_domains/provider.rs src-tauri/src/ai_runtime/fresh_domains/service.rs src-tauri/src/ai_runtime/capability_resolver.rs src-tauri/src/ai_runtime/mcp_external_tools.rs src-tauri/src/ai_runtime/run_intake.rs src-tauri/src/ai_runtime/run_contract.rs src-tauri/src/ai_runtime/run_contract_tests.rs src-tauri/src/ai_runtime/web_evidence_broker.rs src-tauri/src/ai_runtime/fresh_domains/tests.rs
git commit -m "feat(ai): 自动解析当前事实只读服务商"
```

### Task 5: 接通常用地点与地域放宽

**Files:**

- Create: `src-tauri/src/ai_runtime/fresh_domains/location.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/memory.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/service.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/tests.rs`

**Interfaces:**

- Consumes: 当前请求地点、现有 `ai_memories` global 读取。
- Produces:

```rust
pub(crate) struct ConfirmedLocation {
    pub(crate) city: Option<String>,
    pub(crate) province: Option<String>,
    pub(crate) country: Option<String>,
}

pub(crate) fn resolve_confirmed_location(
    explicit: Option<&ConfirmedLocation>,
    memories: &[AiMemory],
) -> ConfirmedLocation;
```

- [x] **Step 1: 写优先级和禁止推断测试**

本轮明确地点覆盖 memory；只读取 `location.city/province/country`；vault scope、Web 内容、IP 字符串和任意相似 key 不进入结果。

- [x] **Step 2: 写领域地域测试**

天气/附近影院无 city 返回 `agent_run_location_required`。新闻/全国档期按 city→province→country 放宽，只有 `LocationCoverage` 缺口才能进入下一层。

- [x] **Step 3: 运行测试确认失败并实现**

```bash
cargo test --manifest-path src-tauri/Cargo.toml confirmed_location -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml location_scope_widens_city_then_province_then_country -- --nocapture
```

实现后再次运行，Expected: PASS。

- [x] **Step 4: 提交**

```bash
git add src-tauri/src/ai_runtime/fresh_domains/location.rs src-tauri/src/ai_runtime/tool_dispatch/memory.rs src-tauri/src/ai_runtime/fresh_domains/service.rs src-tauri/src/ai_runtime/fresh_domains/tests.rs
git commit -m "feat(ai): 接通确认式常用地点与地域放宽"
```

### Task 6: 提供低配置管理中心映射

**Files:**

- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `src/components/ai/skills/McpProviderDetail.tsx`
- Modify: `src/components/ai/skills/mcpProfileParsers.ts`
- Modify: `src/lib/ipc.ts`
- Create: `tests/mcp-domain-capability-mapping.test.tsx`

**Interfaces:**

- Consumes: Task 1 扩展的 binding IPC、现有 discovered read-only tool review。
- Produces: 每个发现工具可选择一个稳定 operation，并以字段下拉/路径输入确认 output mapping；普通用户不编辑 JSON。

- [x] **Step 1: 写 UI 失败测试**

模拟一个只读天气 MCP 工具，断言用户选择“当前天气”、映射 location/temperature/observedAt/sourceUrl 后保存的 IPC payload 包含 `domainOperation="weather.current"` 和规范化 outputMapping。

- [x] **Step 2: 写安全负例**

写操作工具、缺 source/time 映射、非法 JSON path、一个 provider 重复 operation 均不能保存；错误文案不显示 transport config 或 credential ref。

- [x] **Step 3: 运行测试确认失败**

```bash
npm run test -- tests/mcp-domain-capability-mapping.test.tsx
```

- [x] **Step 4: 实现渐进配置 UI**

默认只显示 operation 和必需字段映射；高级区显示只读 schema/hash。若当前 provider 已是唯一健康映射，保存后无需 Composer 逐轮选择。

- [x] **Step 5: 运行测试并提交**

```bash
npm run test -- tests/mcp-domain-capability-mapping.test.tsx
npm run typecheck
git add src/components/ai/skills/McpProfilesPanel.tsx src/components/ai/skills/McpProviderDetail.tsx src/components/ai/skills/mcpProfileParsers.ts src/lib/ipc.ts tests/mcp-domain-capability-mapping.test.tsx
git commit -m "feat(ai): 简化当前事实服务商映射配置"
```

### Task 7: 六领域评测、诊断安全与状态收口

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs`
- Modify: `docs/eval/agent-answer-capacity.md`
- Modify after tests pass: `refactor/appendices/A-current-state-audit.md`
- Modify after tests pass: `refactor/appendices/B-issue-test-traceability.md`
- Modify after implementation: `ARCHITECTURE.md`
- Modify if public IPC changed: `docs/ipc-api-reference.md`

**Interfaces:**

- Consumes: Tasks 1–6 和上一份计划的 grounded finalization。
- Produces: CAP-001 的六领域成功/失败夹具、诊断哨兵和最终已实现架构事实。

- [x] **Step 1: 增加领域矩阵测试**

逐项覆盖附录 B 中 CAP-001 目标测试；每个成功场景断言 DTO、evidence ID、时点和来源，每个失败场景断言无最终事实正文。

- [x] **Step 2: 增加诊断哨兵**

provider 原始 JSON 放入 `SECRET_SENTINEL`、`NOTE_SENTINEL`、`ARGUMENT_SENTINEL`，断言 Run event、tool audit、UI error 和 eval report 均不包含哨兵。

- [x] **Step 3: 运行领域测试**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_domains -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml domain_provider -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml domain_tool_diagnostics_never_expose_raw_output -- --nocapture
npm run test -- tests/mcp-domain-capability-mapping.test.tsx
```

Expected: PASS。

- [x] **Step 4: 运行阶段质量门**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run lint
npm run format:check
npm run typecheck
npm run docs:check
```

Expected: 全部 exit 0。

- [x] **Step 5: 更新事实文档与提交**

只有实现和定向测试通过后，才更新 `ARCHITECTURE.md`、附录 A/B 和 IPC 参考；不能复制计划语气冒充事实。

```bash
git add src-tauri/src/ai_runtime/agent_capacity_eval.rs src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs docs/eval/agent-answer-capacity.md refactor/appendices/A-current-state-audit.md refactor/appendices/B-issue-test-traceability.md ARCHITECTURE.md docs/ipc-api-reference.md
git commit -m "test(ai): 补齐六类当前事实可靠性契约"
```
