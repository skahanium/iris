# 当前事实与结构化工具生产闭环施工计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将当前事实分类、结构化 Provider、补充搜索、证据登记、确定性回答和恢复展示接成唯一可验证的生产链路。

**Architecture:** 时间由 Host runtime 直接回答；天气、金融、影视、体育和可用的新闻操作只使用 Run 接受时冻结的结构化 MCP mapping。没有结构化 Provider 的新闻/研究走现有 WebEvidenceBroker，其他领域 fail-closed。Provider 原始结果先映射为 Iris DTO、通过确定性验证并登记到现有 evidence ledger，Host 根据真实 evidence ID 渲染领域事实。

**Tech Stack:** Rust、SQLite 现有 071/072 schema、Tauri IPC、React 19、现有 MCP host、WebEvidenceBroker、Agent capacity eval。

## Global Constraints

- 不创建 worktree，不新增第三方依赖、数据库表或 migration。
- 不把 Provider 返回的 ID 当作 Iris evidence ID；不持久化原始 Provider JSON。
- 不把通用 Web 结果伪装成天气、金融、影视或体育结构化记录。
- 搜索业务轮次与 Provider 重试/failover 分开计数。
- 每个行为先写失败测试，使用定向测试；最终收口再运行完整质量门禁。
- 保持 `EVID-003` Deferred，不实现通用自由文本语义 verifier。

## 施工阶段

### 1. 文档基线与缺参协议（已施工，待生产回归）

- 将阶段 8、CAP-001 等过早 Resolved 状态改为 Partial/Planned，并登记 `INPUT-001`、`ROUTE-004`、`WEB-002`、`CAP-002`、`EVID-006`、`EVAL-003`。
- 扩展 Run 状态为 `awaiting_input`，增加 `InputRequired/InputProvided` 事件、`SubmitInput` 控制动作和 `pendingInput` 投影。
- 缺城市时等待并恢复同一 Run、同一 envelope、同一 Provider snapshot 和预算；重复提交幂等。

### 2. 当前事实生产路由（部分完成）

- 真实接通 `CurrentRunDomain`；时间使用 Host runtime；无结构化天气/金融/影视/体育 Provider 时返回稳定缺失错误。
- `ToolSurfacePlan` 只暴露当前 operation；Web 关闭时所有外部工具不可达。
- 修复 `normal_run_service` 中构建研究计划时丢失确认地点的问题。

### 3. 结构化记录、Provider 与证据（部分完成）

- 将 Provider 输出 DTO 与数据库 evidence ID 分离；验证成功后由现有 `session_evidence` 生成真实 ID。
- 每个 operation 在 Run 接受时冻结最多三个有序健康 mapping；无序多 Provider fail-closed。
- 每个业务调用最多三个 Provider 尝试：有备选时顺序切换；仅单 Provider 时瞬时故障允许一次同 Provider 重试。
- 结构化 Provider 全部失败时不生成猜测；新闻才允许走 Web fallback，且不伪造 `NewsRecord`。
- `submit_domain_answer` 及 Host 固定模板渲染仍是未完成项；当前仍使用已有 `submit_final_answer`，并由当前事实 validator 做 fail-closed 校验。

### 4. 统一研究预算（部分完成）

- 首次预搜索计入业务搜索预算；简单事实最多首次加一次补充，推荐/比较/新闻汇总最多首次加两次补充。
- 补充搜索必须携带 `EvidenceGap`；相同规范化查询/gap 不得重复；证据充分立即停止。
- 同 MCP 技术重试和冻结备用 MCP 切换不消耗业务轮次；删除 ToolLoop 外层重复 Broker retry。
- 查询哈希、gap 约束、业务轮次和 winner 已写入 `agent_run_steps.resume_state_json`，并在同一 Run 重建执行器时恢复；当前仍缺少真实 Provider 级 attempt/winner 端到端夹具，不能把“Provider 重试次数”与“业务搜索轮次”混为一谈。

### 5. UI、评测与文档收口（部分完成）

- 前端支持当前 Run 的 pending input，隔离旧 Run 的回答和事件。
- 为全部 11 个 `DomainOperation`、缺 Provider、陈旧数据、备用 Provider、补充搜索、恢复和诊断安全增加生产路径测试。
- smoke 达到 24/24，full eval 达到 48/48；只在真实测试通过后更新附录和完成状态。

## 关键接口

```rust
RunState::AwaitingInput
RunEventType::{InputRequired, InputProvided}
RunControlAction::SubmitInput { input_id: String, values: BTreeMap<String, String> }

struct DomainSourceCandidate { provider_id: String, source_url: String, source_title: String, observed_at: DateTime<Utc> }
struct RegisteredDomainRecord { evidence_id: i64, operation: DomainOperation, record: ValidatedDomainRecord }
```

```json
{"operation":"weather.forecast","evidenceIds":[123,124]}
```

## 定向测试与提交

每阶段执行失败测试 → 确认失败 → 最小实现 → 定向测试 → 中文 Conventional Commit。必须覆盖：缺参恢复、生产 CurrentRunDomain、Provider failover、Provider 不得注入 evidence ID、11 个 operation 的真实登记、简单事实补搜一次、推荐补搜两次、无 gap 禁止补搜、重复查询拒绝、技术重试不占业务预算、winner 粘性、Host 终局渲染、终态恢复不重执行和原始输出诊断哨兵。

提交顺序：

1. `docs(ai): 重置当前事实能力状态并登记补差计划`
2. `feat(ai): 增加 Run 缺参等待与恢复协议`
3. `fix(ai): 接通当前事实生产路由与验证要求`
4. `refactor(ai): 分离领域输出与证据身份`
5. `feat(ai): 冻结结构化服务商候选与故障切换`
6. `fix(ai): 统一补充搜索预算与服务商重试`
7. `feat(ai): 以已登记领域记录生成确定性回答`
8. `fix(ui): 支持当前 Run 缺参输入与恢复投影`
9. `test(ai): 补齐结构化工具生产闭环评测`
10. `docs(ai): 对齐当前事实能力证据与完成状态`

## 完成门禁

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

不运行 live API eval；不得以单元 DTO 测试替代正式 intake、ToolLoop、MCP snapshot、evidence ledger 和最终消息路径。
