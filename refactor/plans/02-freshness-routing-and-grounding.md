# 时效分类、有界研究与证据化收口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让本机日期不误联网，让当前外部事实在首批证据不足时可以有界继续研究，并阻止来源组存在但事实无支持的自由文本完成。

**Architecture:** 在既有 `ExecutionEnvelope` 中冻结一个向后兼容的 fresh fact policy；新建纯规则分类/查询规划模块，避免继续膨胀 `run_intake.rs`。复用现有 ToolLoop、WebEvidenceBroker、evidence ledger 和 `submit_final_answer`，不新建 Router、证据表或评测 runner。

**Tech Stack:** Rust、SQLite 现有 schema、Tauri 2、现有 LLM tool loop/WebEvidenceBroker、Vitest 仅用于用户可见错误投影。

**Status:** Planned；对应 `ROUTE-003`、`WEB-001`、`EVID-005`、`EVAL-002`。测试通过并更新附录 A、B 前，不得写入 `ARCHITECTURE.md` 作为已实现事实。

**Dependencies:** 后端实现不依赖计划 01 的代码，但 PR 顺序应先完成跨 Run 投影隔离，避免评测期间混淆回答内容所有权。

## Global Constraints

- 不创建 worktree，不新增依赖，不新增 migration。
- Web 开关仍是外部网络唯一授权；classified/local-only 保持离线。
- 当前事实证据不足时失败关闭；不使用 LLM judge 作为成功门。
- 先写失败测试并运行单个测试；阶段收口前再运行相关模块测试、fmt 和 clippy。
- 实现不能改变通用写入确认、classified 持久化或普通 `external.read` 授权语义。
- Commit 使用中文 Conventional Commit。

---

### Task 1: 在 envelope 冻结当前事实策略

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_contract.rs`
- Modify: `src-tauri/src/ai_runtime/run_contract_tests.rs`
- Create: `src-tauri/src/ai_runtime/fresh_fact_classifier.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`

**Interfaces:**

- Consumes: `ExecutionEnvelope`、当前消息、接受 Run 时的绝对时间。
- Produces:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FreshFactDomain {
    #[default]
    None,
    Runtime,
    Weather,
    News,
    Finance,
    Entertainment,
    Sports,
    GenericWeb,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LocationRequirement {
    #[default]
    None,
    Country,
    City,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FreshFactPolicy {
    pub(crate) schema_version: u8,
    pub(crate) domain: FreshFactDomain,
    pub(crate) window_start: Option<String>,
    pub(crate) window_end: Option<String>,
    pub(crate) location_requirement: LocationRequirement,
}
```

`ExecutionEnvelope` 增加 `#[serde(default)] pub(crate) fresh_fact: FreshFactPolicy`。`Default` 必须产生 `schema_version=1`、domain none、无窗口和无地点要求。

- [ ] **Step 1: 写历史 JSON 兼容测试**

构造不含 `freshFact` 的旧 envelope JSON，反序列化后断言：

```rust
assert_eq!(envelope.fresh_fact.domain, FreshFactDomain::None);
assert_eq!(envelope.fresh_fact.schema_version, 1);
```

- [ ] **Step 2: 写领域分类表驱动测试**

在新模块中先锁定接口：

```rust
pub(crate) fn classify_fresh_fact(
    message: &str,
    accepted_at: chrono::DateTime<chrono::Utc>,
) -> FreshFactPolicy;
```

至少覆盖：

```rust
[("今天是几月几日", Runtime),
 ("最近有什么好看的电影", Entertainment),
 ("上海未来一周天气", Weather),
 ("今天有什么重要新闻", News),
 ("苹果现在股价多少", Finance),
 ("今晚湖人比赛几点", Sports),
 ("解释量子计算", None)]
```

并断言新闻默认 72 小时、影视为过去 30 天至未来 60 天、体育为当天至未来 7 天、天气为当天至未来 7 天。时间由测试固定为 `2026-08-18T08:00:00Z`，不得读取真实当前日期。

- [ ] **Step 3: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_fact_classifier -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml old_execution_envelope_defaults_fresh_fact_policy -- --nocapture
```

Expected: FAIL，因为类型和模块尚不存在。

- [ ] **Step 4: 实现最小确定性分类器**

实现对象词与时效词组合，不引入模型调用。runtime 必须覆盖中文“今天/现在/当前 + 几月几日/几号/星期/几点/日期/时间”和等价英文。只出现“电影”但明确询问历史影评时不进入 current entertainment；“近期/上映/院线/流媒体/现在能看”才进入当前影视。

- [ ] **Step 5: 运行测试并提交契约**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_fact_classifier -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml run_contract -- --nocapture
git add src-tauri/src/ai_runtime/run_contract.rs src-tauri/src/ai_runtime/run_contract_tests.rs src-tauri/src/ai_runtime/fresh_fact_classifier.rs src-tauri/src/ai_runtime/mod.rs
git commit -m "fix(ai): 冻结当前事实领域与时间语境"
```

Expected: tests PASS，commit 成功。

### Task 2: 将 fresh fact policy 接入 intake，并纠正 runtime 路由

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_intake.rs`
- Modify: `src-tauri/src/ai_runtime/run_intake_tests.rs`
- Modify: `src-tauri/src/ai_runtime/tool_surface.rs`

**Interfaces:**

- Consumes: Task 1 的 `classify_fresh_fact`。
- Produces: 每个 accepted envelope 的 `fresh_fact`；runtime 请求得到 `WebDecisionReason::TrustedRuntimeFact`、`Freshness::Offline`。

- [ ] **Step 1: 写截图问题失败测试**

新增：

```rust
#[tokio::test]
async fn today_date_question_uses_trusted_runtime_without_web() {
    let envelope = resolve("你好，今天是几月几日？", true).await;
    assert_eq!(envelope.fresh_fact.domain, FreshFactDomain::Runtime);
    assert_eq!(envelope.freshness, Freshness::Offline);
    assert_eq!(envelope.web_reason, WebDecisionReason::TrustedRuntimeFact);
    assert!(!envelope.required_capabilities.iter().any(|id| id.as_str() == "web.search"));
}
```

- [ ] **Step 2: 写 Web 关闭负例**

天气/新闻/影视/金融/体育在 Web 关闭时仍分类为对应 domain，但 envelope 不获得 Web capability，且 verification 不能被误标为已经满足。

- [ ] **Step 3: 运行 intake 目标测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml today_date_question_uses_trusted_runtime_without_web -- --nocapture
```

Expected: FAIL，当前会进入 WebRequired。

- [ ] **Step 4: 在 ExclusionClassifier 前后接入同一分类结果**

一次调用 `classify_fresh_fact`，把结果写入 envelope；`is_trusted_runtime_request` 改为消费 domain runtime，而不是维护第二套短语列表。`classify_time_sensitivity` 若保留，只从 fresh domain 投影 `Current/None`，不能继续独立扫描消息。

- [ ] **Step 5: 运行 intake 与 surface 测试**

```bash
cargo test --manifest-path src-tauri/Cargo.toml run_intake_tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml tool_surface -- --nocapture
```

Expected: PASS。

### Task 3: 建立绝对查询计划和有界证据缺口

**Files:**

- Create: `src-tauri/src/ai_runtime/fresh_research_plan.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`

**Interfaces:**

- Consumes: `FreshFactPolicy`、当前用户消息、明确授权材料、经确认地点。
- Produces:

```rust
pub(crate) enum EvidenceGap {
    MissingEntity,
    MissingLocation,
    LocationCoverage,
    MissingTimestamp,
    StaleObservation,
    MissingUnit,
    MissingChannel,
    MissingIndependentSource,
    SourceConflict,
}

pub(crate) struct ResearchBudget {
    pub(crate) max_searches: u8,
    pub(crate) max_fetches: u8,
    pub(crate) max_repairs: u8,
}

pub(crate) struct FreshResearchPlan {
    pub(crate) initial_query: String,
    pub(crate) budget: ResearchBudget,
    pub(crate) domain: FreshFactDomain,
}

pub(crate) fn build_fresh_research_plan(
    message: &str,
    policy: &FreshFactPolicy,
    locale: &str,
    location: Option<&ConfirmedLocation>,
) -> AppResult<FreshResearchPlan>;
```

- [ ] **Step 1: 写绝对日期与地点测试**

对 2026-08-18、上海、近期电影断言 query 同时包含 `2026-08-18`、时间窗结束日期、上海和院线/流媒体语义；不得包含历史助手回答或自动本地材料。

- [ ] **Step 2: 写预算与去重测试**

单一事实断言 `2/3/1`，推荐断言 `3/5/1`。同一 normalized query + 同一 gap 第二次提交必须返回 `fresh_research_duplicate_query`。

- [ ] **Step 3: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_research_plan -- --nocapture
```

- [ ] **Step 4: 实现纯查询规划**

查询构造只消费列出的显式输入。地点缺失且 `LocationRequirement::City` 时返回 `agent_run_location_required`；不访问 IP、Vault 或 provider 配置。

- [ ] **Step 5: 用 planner 替换原始 `required_web_query` 的当前事实分支**

非当前事实继续使用既有安全 query 逻辑；current fact 使用 `FreshResearchPlan.initial_query`。保留显式授权材料的 taint/sanitization 门禁。

- [ ] **Step 6: 运行相关测试并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml fresh_research_plan -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml normal_run_service_tests -- --nocapture
git add src-tauri/src/ai_runtime/fresh_research_plan.rs src-tauri/src/ai_runtime/mod.rs src-tauri/src/ai_runtime/normal_run_service.rs src-tauri/src/ai_runtime/normal_run_service_tests.rs
git commit -m "fix(ai): 以绝对语境规划有界联网研究"
```

Expected: PASS。

### Task 4: 保留证据不足路径的有界 Web 工具能力

**Files:**

- Modify: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/tool_surface.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`

**Interfaces:**

- Consumes: `FreshResearchPlan`、现有 `NormalRunToolExecutor`、WebEvidenceBroker。
- Produces: current ToolLoop 不因一次预取自动隐藏 `web_search`；每次后续搜索携带一个未解决 `EvidenceGap` 并扣减 frozen budget。

- [ ] **Step 1: 写首批不足后继续搜索测试**

模型夹具顺序：第一次 `web_search` 返回缺地域/日期的影视结果；模型随后以 `MissingLocation` 发出第二次查询；第二次返回完整证据；第三次搜索必须被预算/完成状态阻止。

- [ ] **Step 2: 写首批充分即停止测试**

第一次返回包含实体、地域、渠道、日期和来源的夹具，断言只调用一次 Web，随后直接进入终局提交。

- [ ] **Step 3: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml insufficient_first_search_triggers_bounded_refinement -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml sufficient_first_search_stops_without_extra_tool_turn -- --nocapture
```

Expected: 第一项 FAIL，当前只检索一次并隐藏工具。

- [ ] **Step 4: 分离 Direct 充分路径与 ToolLoop 研究路径**

- runtime 直接使用可信能力；
- current Direct 只有在 deterministic sufficiency 已满足时无工具最终化；
- current recommendation/news/comparison 以及首批不足统一进入现有 ToolLoop；
- planner 只把 `web_search` 放入授权 surface，不绕过 `NormalRunToolExecutor`；
- 工具循环消费 `ResearchBudget`，达到上限返回稳定不足结果。

- [ ] **Step 5: 运行 Web/surface/loop 测试并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml insufficient_first_search_triggers_bounded_refinement -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml sufficient_first_search_stops_without_extra_tool_turn -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml agent_tool_loop_tests -- --nocapture
git add src-tauri/src/ai_runtime/normal_run_service.rs src-tauri/src/ai_runtime/run_tool_loop.rs src-tauri/src/ai_runtime/tool_surface.rs src-tauri/src/ai_runtime/normal_run_service_tests.rs src-tauri/src/ai_runtime/agent_tool_loop_tests.rs
git commit -m "feat(ai): 支持证据缺口驱动的有界研究"
```

Expected: PASS。

### Task 5: 当前事实强制证据化终局

**Files:**

- Create: `src-tauri/src/ai_runtime/current_fact_finalization.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/ai_runtime/final_answer_submission.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`
- Modify: `src/types/ai.ts`

**Interfaces:**

- Consumes: `FreshFactPolicy`、`FinalAnswerSubmission`、当前 Run evidence registry。
- Produces:

```rust
pub(crate) enum CurrentFactFinalizationError {
    UnsupportedProtocol,
    InsufficientEvidence,
    UnsupportedClaim,
}

pub(crate) fn validate_current_fact_submission(
    policy: &FreshFactPolicy,
    submission: &FinalAnswerSubmission,
    evidence: &[RegisteredEvidence],
) -> Result<(), CurrentFactFinalizationError>;
```

稳定公开错误码：

- `agent_run_grounded_finalization_unavailable`
- `agent_run_fresh_evidence_insufficient`
- `agent_run_location_required`

- [ ] **Step 1: 写来源组不能完成严格事实测试**

提交包含电影名和上映日期、但 citation binding 只有 `SourceGroupFallback`，断言 `UnsupportedClaim`。

- [ ] **Step 2: 写不支持工具协议的模型路由测试**

current fact + route 无 tools/continuation 能力时，断言返回 `agent_run_grounded_finalization_unavailable`，没有最终 assistant 消息和 Completed。

- [ ] **Step 3: 写证据外实体测试**

夹具证据只包含电影 A，submission 引入电影 B，断言拒绝；结构化修复一次仍包含 B 后返回 `agent_run_fresh_evidence_insufficient`。

- [ ] **Step 4: 运行测试确认失败**

```bash
cargo test --manifest-path src-tauri/Cargo.toml strict_current_fact_rejects_unsupported_free_text -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml source_group_fallback_cannot_complete_strict_current_fact -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml unsupported_finalization_protocol_never_falls_back_to_guessing -- --nocapture
```

- [ ] **Step 5: 实现当前事实终局门**

移除 current fact 对空 `calibrated_structured_finalization_enabled` 白名单的依赖。current fact 必须暴露内部 `submit_final_answer`；验证实体、数字和日期可在所引用 evidence 中精确或规范化定位。普通非 current free text 继续现有 uncalibrated 路径。

- [ ] **Step 6: 同步前端错误联合类型和文案**

在 `AssistantRunErrorCode` 增加三个稳定码；UI 映射：需要地点直接询问、证据不足明确不猜测、协议不可用建议更换支持 Agent 工具的模型。

- [ ] **Step 7: 运行测试并提交**

```bash
cargo test --manifest-path src-tauri/Cargo.toml current_fact -- --nocapture
npm run typecheck
git add src-tauri/src/ai_runtime/current_fact_finalization.rs src-tauri/src/ai_runtime/mod.rs src-tauri/src/ai_runtime/final_answer_submission.rs src-tauri/src/ai_runtime/normal_run_service.rs src-tauri/src/ai_runtime/agent_tool_loop_tests.rs src-tauri/src/ai_runtime/normal_run_service_tests.rs src/types/ai.ts
git commit -m "fix(ai): 对当前事实强制证据化收口"
```

Expected: PASS。

### Task 6: 固定复现场景、验证与状态更新

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs`
- Modify: `docs/eval/agent-answer-capacity.md`
- Modify after tests pass: `refactor/appendices/A-current-state-audit.md`
- Modify after tests pass: `refactor/appendices/B-issue-test-traceability.md`

**Interfaces:**

- Consumes: Tasks 1–5。
- Produces: 固定日期/近期电影/追问复现场景；ROUTE-003、WEB-001、EVID-005、EVAL-002 的真实证据。

- [ ] **Step 1: 增加固定多轮 eval**

夹具时间固定 2026-08-18，证据只允许两部带上海院线/日期的电影，并额外放入一个无日期旧电影诱饵。断言回答只引用允许实体，且工具使用后不包含“没有联网/抓取能力”。

- [ ] **Step 2: 运行新增场景**

```bash
cargo test --manifest-path src-tauri/Cargo.toml current_fact_movie_follow_up_scenario -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml agent_does_not_deny_web_after_current_run_search -- --nocapture
```

Expected: PASS。

- [ ] **Step 3: 运行阶段质量门**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml fresh_fact -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml current_fact -- --nocapture
npm run lint
npm run format:check
npm run typecheck
```

Expected: 全部 exit 0。

- [ ] **Step 4: 更新追踪表并提交**

只有对应测试真实存在且通过后，才把问题从待施工表移到实证表。

```bash
git add src-tauri/src/ai_runtime/agent_capacity_eval.rs src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs docs/eval/agent-answer-capacity.md refactor/appendices/A-current-state-audit.md refactor/appendices/B-issue-test-traceability.md
git commit -m "test(ai): 补齐当前事实可靠性评测"
```
