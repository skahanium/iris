# 02. 当前状态、生产缺陷与技术债

> **文档状态**：现行
> **文档类型**：当前事实审计
> **事实基线**：2026-09-05，审计起点 `70c929ac`

> 本轮施工事实补充至 2026-09-08 工作树；未提交代码不得被当作已部署能力。

## 可保留基础

- Run 的幂等受理、持久化终态、恢复和同 Run UI 投影。
- 冻结 capability、预算、权限重检、工具审计和有界写入确认。
- Run-local evidence ledger 与 `ProvenancePolicy`；来源身份不依赖数据库全局 ID。
- 一个 `AgentToolLoop`，含模型/工具总量与分类上限、重复抑制、无进展停止和最终综合。

## 已确认的系统性缺陷

1. Intake 曾用当前句关键词与 `conversation_meta` 把对话强制离线；即使用户质疑旧事实或要求核实，也可能没有搜索工具。
2. `WebPreferred + Direct` 曾让已授权 Web 对模型不可见。放宽 8/24 预算不会自动形成一次工具调用。
3. 长对话测试只验证固定答案、窗口和生命周期；旧关键词摘要每字段 220 字，不能证明目标、纠正和未完成事项仍在上下文。
4. `SourceGroupFallback` 只能说明“检索过”，曾被当作正文来源支持；`W#`、展示 `[C#]` 和 ledger ID 也曾被重复解释。
5. 领域执行器把写作/法规/小说等词汇带进核心路由，并以范文字符串匹配伪装为语义事实验证。
6. 压力探针曾把 24/25 边界的语义写反；完整评测失败后仍可能留下旧的 48/48 报告。
7. Web 搜索片段曾在模型选择来源前直接写入最终 evidence ledger；一次模型响应又可连续派发任意数量发现调用，导致低质量结果先占满 12 条证据容量。
8. Provider 成功返回空正文/空工具时曾被当作有效响应；协议错误又映射为能力不足，既不重试同一路由，也无法向下一轮解释真实失败。
9. MCP fetch 曾只检查顶层 transport 成功，并把 `content[].text` 中的 429 应用错误、搜索包装或标题重复当成网页正文；自然严格路径又会在来源验证前流出草稿，修复时以 `AnswerReset` 整段撤回。该缺陷记为 INC-HR-006，并重新打开 Web 候选/正文分层的真实验收。
10. INC-HR-006 回正后，生产仍暴露出五个相互关联的执行缺口：模型以 `urls-only` 调用重载 `web_search` 会被必填 `query` 拒绝；Broker 曾用同一 `max_search_results` 同时截断搜索候选和显式抓取 URL，导致关闭 fetch 内部发现时也把待读 URL 截成 0；普通时效事实被官方/双域名门槛过度阻断；现代限制回答在历史加载时会重新挂回整 Run 来源；确定性 full 报告未核对汇总计数，36/48 也可能以成功退出。该组问题记为 INC-HR-007。
11. 旧 live v2 评测把 `404`/fixture 文本当作公开事实 oracle，并将每条路由平均切为 24 模型轮次/18 Web 调用；失败报告又可能在落盘前被拒绝。它既不能证明真实语义质量，也无法解释两条路由在同一总预算内的差异。该组问题记为 INC-HR-008。
12. 2026-09-02 首次 v3 Campaign 又暴露了两个评测器出口缺陷：遥测把模型提出但未执行的 tool call 记为 Web 业务调用，误触发 `campaign_budget_exhausted`；顶层提前返回使两份 v3 报告未落盘。真实额度不重跑；该事故只用于本地回归，修复后须由下一次单独授权的 Campaign 重新校准。
13. INC-HR-009 的生产复现说明“工具已授权”仍不等于“工具实际发生”：`WebRequired` 能被模型直接答复绕过；未派发的工具提议曾写入 canonical assistant/tool transcript 并锁定 Provider；fetch 的 8 秒整批预算与 20 秒单后端超时互相矛盾，且 search 成功会掩盖同 Provider fetch 连续失败；旧会话摘要仍是每字段 220 字的关键词摘录。它们共同造成无正文限制、错误的备用模型提示和长对话纠正丢失。
14. INC-HR-010 审计确认同一 Run 的执行事实仍被多处重复解释：评测收紧策略曾只到 executor、ToolLoop/Direct Gateway 又读取持久化原策略；Host 起步存在执行后才发现额度不足的路径；全拒绝工具提议可能跳过综合并清除已接纳 continuation；历史按 token 投影但摘要按消息数覆盖，长消息会无声丢失尾部更正；评测也曾从会话最后一条 assistant 读取答案。它们不是模型或领域问题，而是模块间事实源不唯一。

这些不是搜索服务、某个模型或电影领域的单点问题。它们说明控制面、上下文投影、来源合同和验收报告没有同步收敛。

## 当前施工事实

### 零回复死胡同收敛（2026-09-13）

2026-09-10 会话的 4 次终态失败全部以「用户什么也看不到」结束。复核代码后确认，这些失败落在**五处一次性硬错误出口**，且此前**没有任何测试覆盖**：

- 严格运行连续两次以散文作答 → 硬错误，正文被丢弃；
- 草稿连续两次不完整、续写契约被违反 → 硬错误，草稿被丢弃；
- 模型轮次被连续拒绝的提议烧尽 → 硬错误，已有证据与观察全部作废。

同族的第四条路径（来源绑定失败）返回的是 Host 撰写的**有界限制说明**，可正常发布。本轮把那五处统一为该形态：终态不再有「无可见输出」的形状，真实成因（`agent_run_incomplete_output`、`agent_run_tool_loop_limit`）保留在 `toolLoop` 诊断的 `exhausted` 事件中，供运维区分轮次耗尽与正常收束。

分档语义：严格当前证据契约的运行**不发布无来源散文**，只发布中立的「本轮未能完成可核验回答」限制说明；普通运行的「未验证正文 + 显式标签」档在 `requires_natural_source_binding()` 恢复为真之前**没有可达触发点**（该谓词在生产中恒为 `false`，`run_tool_loop.rs:2355`），因此不新增空转机制。

回归：`strict_submission_exhaustion_publishes_a_bounded_limitation` 与 `model_turn_exhaustion_publishes_a_bounded_limitation` 先失败后通过；`from_policy_preserves_the_direct_one_model_zero_tool_budget`、`child_policy_executes_six_tools_and_rejects_the_seventh`、`partial_visible_stream_recovery_rejects_a_business_tool_call` 改为直接断言「工具未执行」这一真实不变量——它们此前以「返回 Err」作替身断言，调用计数断言本就存在且保持不变。

前端侧另一半（同一缺陷族）：投影在 `failed` 时**删除**该 Run 的空助手槽，而失败脚注
（`AiMessageList` 的「本次请求未完成，未纳入后续对话上下文。」）只在携带
`turnState: "failed"` 的**用户行**下渲染——删掉的那一行正是唯一能承载标记的行，因此失败
轮在转录区里彻底消失，与「从未提问」无法区分。现改为在用户行标记 `turnState: "failed"`，
空助手槽仍然丢弃（停止态不发布未提交候选，且助手分支无条件渲染，保留只会多一个空气泡）；
带 `contentRef` 的行不算空行，其已发布正文不会被丢弃。

该处的既有测试只断言数据层，未断言渲染，因而无法发现「标记放错行」。新增
`tests/assistant-failed-turn-visibility.test.tsx` 把投影与 `AiMessageList` 连起来渲染，
直接断言脚注文本出现在转录区；该测试在修复前失败。

覆盖缺口修复的其余项（2026-09-13）：

- **地区不匹配检测**（文档 03 §4 已要求、此前未实现）：Web 观测（候选与正文两侧）现在附带
  `sourceDomains`（去重、小写、有序、上限 8）与常驻 `scopeCheck` 指令「核对来源域名所属市场
  是否与本轮适用范围一致；不一致时继续换源」。Host 只陈述事实，**不携带域名到市场的映射、
  不携带地区词典**，市场判断仍归模型与 `## GeographicScope` 合同，因此不会形成第二套范围权威。
- **降级路径链接校验**：此前 `validate_web_urls_against_allowed` 只在 `requires_web_evidence()`
  时执行，非严格联网运行可发布模型自造链接。现对「本轮实际发生过联网动作」的非严格运行**剥离**
  未登记链接而非拒绝整篇：正文保留，只去掉无法核实的指针（拒绝会以「编造引用」换「零回复」，
  与既定原则冲突）。登记在案的本轮来源链接不受影响；未联网的运行完全不处理，用户自己消息里的
  链接不会被误伤。自然澄清与证据受限答复**同样排除**——后者由 Host 撰写且刻意披露它确实看到的
  未核实线索及其 URL，剥离会毁掉披露本身。该排除与严格分支自身的守卫一致。
- **降级横幅文案**：原文案称「已继续生成受约束答复」，但 Host 按设计不约束也不包裹模型正文
  （`apply_required_web_degradation_notice` 是有意的空操作接缝，且**在生产中确实被调用**，
  见 `run_engine/mod.rs:929`、`:1445`）。改为「已继续生成未经联网核实的答复」，如实描述。
- **`## GeographicScope` 编译断言**：该节此前是唯一没有编译断言的合同节，重构丢弃它不会让任何
  测试失败；现已补上存在性与两条关键句断言。

上述修复的自查发现（2026-09-13）：

- **载荷体积核算漏算新字段**：`serialized_web_tool_payload_chars` 只按 packet 形状预测体积，
  新加的 `sourceDomains`（最多 8×255 字符）会把真实载荷推过 `MAX_WEB_TOOL_RESULT_CHARS`。
  现把范围块纳入该函数，并把单个域名收敛到 80 字符，使该块的贡献可预测。
- **剥离器留下 Markdown 残渣**：带标题的链接会残留标题文本与右括号、图片链接会残留图片
  标记、自动链接会残留一对空尖括号。三例均先写失败用例再修：标题随目标一并消费，图片标记
  在解包时一并去掉，自动链接连尖括号整体删除。空白不做规范化——删除是忠实的，留下的两个
  空格是原文分隔符。

### 技术债清算（2026-09-13，行为不变）

清债的前提是先把「死」证死，结论与最初判断不同，记在这里以免下次重复误判：

- **`WebSourceRank` 的五个变体不是死代码，是读取兼容契约**。`f997127f`/`d5fa17ba` 时期生产
  代码确实按域名分类并写入过 `Official`/`Academic`/`Media`/`Community`；分类器随统一前 Web
  核心退役后，现有全部生产构造点（`tool_dispatcher`、`web_evidence_broker` 六处）都只写
  `Unknown`，但这些变体仍要能反序列化旧的 `WebEvidenceMeta` 行。**不得删除**。
- **`RunEventType::WebVerificationFailed` 同理**：`0262ea28`（HR-6/HR-7）删除了它唯一的
  生产写入点，因此新 Run 不会再产生该事件，但已在发布版本中运行过的实例可能留有该行，
  仓储解码分支必须保留。仓库 tests 仍在做写入/回滚往返。**不得删除**。
- **删掉的是真正冗余的部分**：`RunWebEvidenceState::has_official_source` 与其在交叉印证门槛
  中的析取项不可达（无任何生产赋值点，唯一写入是 `|= item.source_rank == Official`），已随
  `corroborated_source_threshold_met(independent_domains)` 一起删除；模型可见的
  `remaining_evidence_requirement` 也据实从 `official_source_or_second_independent_domain`
  改为 `second_independent_domain`，不再声明一个不存在的能力。
- **遥测的「未发生」计数走另一条路径**：`TruncationOutcome::{None, FinalOutputRejected}` 与
  `BudgetOutcome::WithinBudget` 从未被传入，但 `truncations.none` / `budgets.within` 两个报告
  字段仍由 `record_final_output_validation` 正确累加，属重复 API 而非缺陷，已删除冗余变体。
  `EvalFault` 则相反：五个变体没有测试注入，但 `execute_headless_core_case_*` 里都有对应处理
  分支，属**故障注入面**，保留并加窄 `allow(dead_code, reason=...)` 说明。
- **一个 `#[allow(dead_code)]` 掩盖了 147 条告警**，其理由字符串（"Task 2 stages the evaluator
  contract for the Task 3 runner"）早已过时。经逐条核对，其中 146 条只被同文件的测试模块使用
  （该文件 71% 行是 `#[cfg(test)]`），真死代码只有一处 `LiveHydrationTransportProof` 证明夹具。
  现改为 `#[cfg_attr(not(test), allow(dead_code, reason=...))]`：生产构建仍安静，而
  `clippy --all-targets -D warnings` 能重新发现「连测试都不用」的代码。
- **文件长度预算进 CI**：新增 `npm run size:check`（`scripts/file-size-budget.mjs`），默认
  2000 行，超限文件进入 `SPLIT_QUEUE` 并按当前规模钉住，只许变短；文件缩回默认以内却不删除
  条目会让门禁失败。`agent_capacity_eval.rs`（13.4k 行）排队拆分，方案见下。

### 外部评审（GPT6）十条的处置（2026-09-13）

逐条核对后的状态。**①–⑦、⑩ 已修并各自带反向验证**（拆掉修复看测试变红），
⑧ 未开始，⑨ 属设计取舍：

| #   | 事项                            | 状态                                                                             |
| --- | ------------------------------- | -------------------------------------------------------------------------------- |
| ①   | 终态限制说明识别失败            | 已修：常量表 + 生产者共用常量 + 类级不变量测试                                   |
| ②   | 本地载荷盲切破坏 JSON           | 已修：JSON 感知收缩、保留 `nextStartByte`；集成级测试反向验证通过                |
| ③   | 中文索引/查询不匹配             | 已修：查询侧 `"双字词短语" OR "原样词"`；禁用扩张即复现 0 命中                   |
| ④   | 片段无答案 + 无相关性排序       | 已修：`bm25()` 排序 + 词项覆盖率选段；反转排序即选中「Introduction」             |
| ⑤   | 逐页阅读被判无进展              | 已修：身份键纳入范围；去掉范围键即失败                                           |
| ⑥   | 诊断不外传                      | 已修：回传有界 `retrievalStatus`（layer + 类型化状态），不泄漏自由文本/标识/路径 |
| ⑦   | 候选先截断后过滤范围            | 已修：路径范围下推进 FTS/元数据 SQL；禁用谓词即复现 `got [outside/note-0..3]`    |
| ⑧   | 网页正文只取前 2,000 字且无续读 | **未开始**，方案见下                                                             |
| ⑨   | Host 先搜后理解                 | **设计取舍，不按缺陷改**，见下                                                   |
| ⑩   | 工具轨迹无压缩、超预算即失败    | 已修：最旧大结果替换为有界标记；接线测试反向验证通过                             |

**⑧ 的实施方案（下一步直接照此做）**：`web_fetch` 现只接受 `urls`，正文经
`MAX_WEB_EXCERPT_CHARS`（2,000）在 `run_tool_loop.rs` 的取证与载荷两处截断，页面
后半段永远进不了上下文。要改三处并保持向后兼容：①工具 schema 接受可选的按 URL 起始
偏移（`urls` 元素保持字符串，或接受 `{url, startByte}` 对象）；②Broker 侧对已缓存页面
按偏移重新切片（`web_page_cache` 已有缓存，不必二次外发）；③载荷回传
`excerptStart`/`nextStartByte`/`totalChars`，使模型知道还有后续且能续读。同时更新
`prompt_contract`/catalog 的 schema 断言与工具描述。**不要**只把上限调大——那只是把
同一个问题推远。

**⑨ 的判断依据**：`bootstrap_required_web_observation`（`run_tool_loop.rs:1815`）按当前
问句加日期先取一次最低观察，是 INC-HR-009 明确设计（见本文件与 HR 路线中的「最低实际
观察」条目），不是疏漏。它的已知代价是：依赖历史的追问（如「那今年呢」）会先用预算
取一批可能无关的材料。可以评估的改进方向是「当请求明显是省略式追问且历史已含足够
上下文时跳过 bootstrap」，但**必须先有实测发生率**，否则就是用猜测换掉一个有意的
安全默认。定性上它是取舍，量化前不应与 ①–⑧ 并列成缺陷。

### `agent_capacity_eval.rs` 拆分计划（进行中：3/21）

该文件 407 个顶层项里只有 147 项是生产代码（约 3.9k 行），其余 260 项、约 9.6k 行是
`#[cfg(test)]` 支撑代码，真正的测试在 `agent_capacity_eval_tests.rs`。可机械拆成 21 个子模块
（每个 < 1.6k 行），父文件只留模块文档、`mod` 声明与 `pub(crate) use <child>::{...};` 门面，使
`crate::ai_runtime::agent_capacity_eval::X` 路径与 `provider_continuation_tests.rs` 的 glob
导入保持不变。关键约束：约 20 个结构体的私有字段被兄弟模块构造或读取，需要逐个 `pub(super)`；
生产/测试项在同文件内交错 25 次，必须保留每项自己的 `#[cfg(test)]`。

已完成（`75fc9323`）：`contract.rs`（567 行）、`telemetry.rs`（354 行）、`tool_class.rs`（121 行），
父文件 13,453 → 12,431 行。机制与三条硬教训：

1. **子模块声明用 `#[path = "agent_capacity_eval/<child>.rs"] mod <child>;`**（与 `skills_impl.rs`
   同一写法），不要另建 `agent_capacity_eval/mod.rs`。子模块内用 `use super::contract::*;`
   加上 `std`/`serde` 的显式导入，不要用 `use super::*;`（父文件的 glob 再导出会被判为未使用）。
2. **可见性只升不降**：原本 `pub(crate)`、被 `agent_capacity_eval_tests.rs` 或其它模块导入的项
   必须保持 `pub(crate)`，否则 `pub(crate) use` 会报 E0603/E0364；只有仅在同文件树内使用的项才改
   `pub(super)`。用一个盲替换「先匹配空前缀再匹配 `pub(crate) `」的脚本会把 `pub(crate)` 降级成
   `pub(super)` —— 必须按 `git show HEAD:<file>` 逐个核对原可见性。
3. **父文件的再导出清单要由外部消费者反推**，不要整段 glob 或全量复制：只再导出
   `agent_capacity_eval_tests.rs` 与生产模块真正 `use` 的名字，且仅被测试使用的名字要加
   `#[cfg(test)]`，否则非测试构建会报未使用导入。**绝不要按行删除 `#[cfg(test)]` 行**——文件里
   有 267 处 item 级 `#[cfg(test)]`，误删会直接破坏结构（本轮已发生过一次，靠
   `git checkout -- <file>` 回退重来）。

4. **子模块要显式导入「留在父文件里的类型」**：按簇首/簇尾定位时，簇前的定义（例如
   `ScenarioLanguage`）不会被一起搬走，子模块必须 `use super::ScenarioLanguage;`。同理，父文件
   自己要用的 `pub(super)` 辅助函数要写**显式** `use <child>::{a, b, c};`——`use <child>::*;`
   在本仓库的实测里没有稳定带进这些项。
5. **不要在 `#[path]`/`mod` 声明前留下游离的 `#[cfg(test)]`**：文件里 267 处 item 级
   `#[cfg(test)]` 与空行交错，按行删属性极易把模块声明变成「仅测试构建存在」，表现为
   `unresolved module`。删除属性必须成对匹配到具体的 item。

（本轮 `case_matrix` 簇（819 行）就卡在第 4、5 条上，已回退到上一个全绿提交重来——**拆分期间
每个子模块都必须停在 `cargo clippy --all-targets -- -D warnings` 全绿**，回退用
`git checkout -- src-tauri/src/ai_runtime/agent_capacity_eval.rs`。）

剩余 18 个候选：`verdict`(含 quality)、`case_matrix`、`pressure`、`headless`、`scoring`、
`summary`、`boundary`、`live_capability`、`security`、`report`、`live_preflight`、`live_pilot`、
`live_result_io`、`live_attestation`、`live_state`、`mcp_contract`、`doubles`。每拆一个都要跑
`cargo clippy --all-targets -- -D warnings`，并在若干节点跑完整 `cargo test --lib`。

自查确认无问题的两处：

- 引用映射（来源区）由**账本**构建（`finalization.rs:474`），不扫描正文，因此剥离正文链接
  不会影响来源列表。
- 取消路径的重复投影不会累积气泡；已补一条回归守卫，并顺手把测试文件的 describe 名改为
  「terminal turn visibility」，因为它同时覆盖失败与取消两种终态。

**更正二**：曾据行号过滤（`awk '$2<640'`）判定 `apply_required_web_degradation_notice` 在生产中
零调用。该过滤把 `run_engine/mod.rs` 整个排除在外，结论错误；它是被生产调用的有意空操作接缝，
不删除。

**更正**：曾据本地库把 4 次失败归因于「`native.fetch` 证据不被 `has_web_evidence()` 承认」。代码不支持该机制——`bounded_page_evidence`（`run_tool_loop.rs:3653`）与 `record_web_evidence_quality`（`:2486`）只要求「非空正文 + HTTPS」，与 provider 无关，且当事运行实据为 2 条证据、2 个不同域名、正文各 2000 字。该相关为伪相关（失败运行恰好都是严格运行），不作为修复依据。

### 自然时效提问与调用恢复（2026-09-08）

事故 Run 冻结为 `WebPreferred / volatile_external_fact`，模型两次提出工具调用，但实际工具和网页证据均为零；旧记录没有保存具体拒绝原因，不能归因于某个模型或协议。

当前工作树已将 `requires_web_observation` 与严格证据门分开，由同一 `ToolSurfacePlan` 派生提示与执行要求。截图原句现在先经过现有 executor/Broker 搜索并读取最多两个候选；指定 URL（包括 Markdown 链接）优先读取。空结果或服务失败仍保留有界研究机会。调用名称、标识、JSON、schema、当前 Run URL 范围和预算在派发前返回具体原因，连续两轮全拒绝与搜索无进展分开计数。模型正文可能继续接受来源修复，因此已执行工具形成的 Provider 绑定保持到 Run 结束。取消在 Host 观察前和每次实际执行前检查。

`provider_route_summary_json.toolLoop` 保存有界诊断和累计提议/拒绝/执行计数，首次拒绝单独保留。生产入口回归覆盖原句、模型跳过工具、参数错误、URL 越界、失败后的研究机会和安全边界。真实自然问法回放已发现 URL 越界曾导致整轮失败，并补为失败回归；真实答案质量与全部路由验收仍待完成，不标记本问题已修复。

第二轮公开答案审查进一步复现：Broker 每批正文都从 `[W1]` 编号，后续 fetch 的来源在最终投影中指向了前一批网页。生产 executor 现在从同一 Run ledger 取稳定标签和资源身份，不再使用批内序号；“单独重读第二个 URL 仍为 W2”的生产入口测试先失败、后通过。日期状态判断错误另行约束：按可信当前日期和事件日期判断过去/当前/未来，不能将网页发布日期或抓取日期当作事件日期。提示约束不等于语义质量已验收。

进一步复验已清除普通正文合同中“要求引用、同时禁止 `[Wn]` 绑定”的矛盾；模型提交引用标记与界面隐藏原始协议是不同层次。直接及带前言的内容嵌入工具协议均转为有界 Provider 恢复，不作为最终答案或外部执行。无服务时单独记录能力阻断，不记为已完成实际观察。执行证据、真实延迟和仍未通过的答案质量记录在[时效性检索验收](../docs/testing/harness-timeliness-acceptance.md)。

上下文补验又复现普通终态把 Run 编号按会话编号解析的独立错误：本轮 W2 会指向本轮第一份正文。终态链接、来源表与现代 evidence 选择现统一使用当前 Run 编号，已抓取但未采用的来源不会在重载时重新出现，既有历史正文不变。该生产回归先失败再通过；扩展 Run 引擎回归也补正了已登记本轮证据的观察初始化。上下文末两例的服务/模型失败仍未取得足够原因记录，不将其归因为协议适配器或计为健康验收。

### 笔记集成审计补充（2026-09-06，入口 `c26bd35b`）

本轮先收敛公共操作与读取合同，尚未完成完整 CRUD/撤销交付。已复现并增加回归的路径包括：被拒绝补丁仍被外层记为成功、移动改写无关换行、回收正文移动后登记失败导致恢复入口不可见、新建路径在拒绝越界之前先创建目录，以及受保护写入区间内 Vault 仍可切换。

持久化审计曾发现：CAS `OnceLock` 换 Vault 后仍返回首次初始化的 store；版本通过 `versions.file_id ON DELETE CASCADE` 关联全局 `files.path` 身份，prune 或目录移动清理会带走历史。当前工作树已将 CAS 按 canonical Vault 绑定，并通过 migration 074 在原版本表记录 Vault/path/recycle 归属，去掉索引级联删除；未知旧历史保持未分配，不猜测迁移或自动删除。版本界面及排队快照请求携带 Vault 身份，版本恢复等待期间复核最新正文。这些定向回归不等同于完整笔记能力验收。

移动与永久清理现已分别使用职责受限的私有恢复检查点，不与 Harness 合并成新存储框架。移动的独立字节恢复材料、目标物理身份复验与已应用回执已有定向回归；词法路径若含符号链接组件，会在任何移动或写入副作用前以 `note_path_alias_not_allowed` 拒绝。清理在暂存、数据库提交与重启收尾之间复验精确历史归属，归属冲突不得吞成暂时元数据失败。真实差异确认、执行回执持久化及独立 Agent 撤销仍待后续完成，不得以局部通过掩盖。

适配层收敛已覆盖 create-only 资产、版本元数据读取范围和外部笔记导入的执行保护。导入原先忽略确认目标、覆盖时直接保存；`external_import_respects_confirmed_note_target_before_creating_directories` 和 `external_import_overwrite_requires_read_baseline_and_preserves_history` 先复现失败，现复用公共 `create_note`/`apply_edit`，覆盖须具备已读 hash 与保护版本。它不代表外部导入已经具备完整冻结差异或独立撤销；显式笔记操作仍须使用精确工具清单，不能顺带授权导入、资产、Git 等其他写入。

编辑器异步图片已绑定原文档基线与选择位置，避免下载完成后插入另一文档或重放原粘贴事件；保存队列保留“旧写入实际成功、新编辑仍 dirty”的真实磁盘基线，迟到失败不能污染同路径重开的记录，锁定/Vault/基线冲突也不再自动重试。`file_write` 在同一保存锁内校验发起 Vault 与已读 SHA-256 基线；空 hash 明确表示目标必须不存在，普通保存不因此生成手动版本。现代编辑器打开回执与保存回执均传递该 SHA-256，禁止把前端 FNV 缓存键当作磁盘基线；打开时冻结的 Vault 在之后换库也不会被当前选择覆盖。普通和涉密路径共用解码后的 hash 语义，涉密落盘仍加密。这仍不是跨文档会话隔离或 Agent 回执接纳的完整验收。

会话清理增加了事务与非终态拒绝回归。`cancelled` 不再单独当作工作已停：后台 worker 仍登记在途时，删除/撤回继续拒绝拆 Run 与回执。撤回删除摘要后，若剩余历史仍长于近期窗口，压缩入口会把缺摘要视为未覆盖前缀并允许一次有界首次压缩。会话撤回仍不能回滚 Markdown，删除会话也不能清除版本和回收站恢复材料。

- Intake 已开始移除会话元数据和垂直领域对 Web 权限的强制影响；授权 Web 会通过统一工具面交给模型。分类器只把安全硬边界、显式核实/联网、URL 和高利害当前事实送进 `WebRequired`；日常时效走 `WebPreferred`。
- 新自然 Web 答复不再写入 `SourceGroupFallback`；缺少 Run-local 精确来源时会获得一次修复，之后显示限制说明而不展示未验证草稿。
- 历史摘要已保留显式目标、偏好、更正、完成与待办的可验证字段；1/20/50/100 轮压力正在替换固定文本样例。当前确定性结果只代表编排/安全/上下文合同，不代表真实模型回答质量。
- `DomainExecutor` 与范文事实字符串门正在移除；通用材料投影保留来源隔离但不选择领域算法。
- `AgentToolLoop` 每个模型回合至多执行 2 个独立发现调用，超出部分以 `deferred_for_feedback` 返回且不计失败；每批结果回到模型后才能继续依赖动作。
- 当前 Run 的检索义务（普通时效或严格核实）先在同一 executor、权限、网络预算、审计和 evidence ledger 内完成 Host 最低观察：原用户问题加可信日期搜索，最多两个候选正文抓取；指定 URL 优先读取。观察作为 system data 进入首个模型回合，不伪造 assistant tool-call；模型仍可在同一循环内改写查询、换源或停止。
- 工具提议在 Host 预检后才成为执行：只有实际 `dispatched` 的调用消耗预算、写审计、进入 assistant/tool transcript 并绑定同 Provider 续轮；纯 rejected/deferred 提议只形成受控反馈，后续模型失败仍可按现有规则切换。
- Web 发现每次最多返回 4 个候选、每 Run 最多保留 8 个；候选只含有界片段且不写 evidence。模型以当前 Run 候选 URL 发起精确读取、正文抓取成功后才登记为可引用来源。
- 模型工具面已拆成 `web_search { query }` 与 `web_fetch { urls }` 两个单一职责动作；抓取传入 Broker 的搜索结果额度固定为 0，Broker 仍保留由独立 `max_fetches` 约束的显式 URL，不能在模型未观察结果时暗中重新发现。两者共享现有授权、network 预算、Broker 和冻结 Provider 顺序。普通时效事实以一份合格正文和精确引用为最低门槛；高风险、CitationCheck 或显式交叉核实才要求官方来源或两个独立域名。
- 2026-09-08 补充回归：发现旧 Broker 无视零搜索额度，在 `web_fetch` 内再次搜索，形成一次读取中的搜索/抓取双向切换。现已在 Broker 执行分支阻断，生产入口同时检查 provider health 证明没有隐式搜索；不只检查外层工具事件。过程区区分搜索与读取的备用提示，候选无该能力不算服务失败。
- 地域敏感的日常时效问题使用本轮明确范围、当前话题用户已确认范围、中国大陆默认范围的优先级。Host 仅对无上下文、无具体限定的泛问补默认地区；复杂实体、上下文和地域无关事实保留原文，由已有模型循环结合统一范围合同形成后续查询。该合同是产品偏好，不推断用户所在地，也不把海外来源自动认定为大陆事实。
- MCP fetch 现在独立解析 transport 信封与一层 JSON 应用载荷；错误信封、URL 不匹配、空正文、标题/搜索包装均不登记 evidence。抓取按当前候选来源优先、再按冻结 `web.fetch` 顺序切换；全部失败时以可行动的 fetch 失败观察返回模型。
- fetch 单候选限 5 秒、整批限 18 秒、外层工具调用限 20 秒；冻结 MCP 路由后仍有既有 native safe fetch 兜底。migration 073 只重建可再生的 `web_evidence_provider_health`，将 `web.search` 与 `web.fetch` 统计分开；runtime discovery 状态不再由业务成功/失败覆盖。
- 会话仍只用既有四个 `conversation_summaries` 字段。ToolLoop 当前可在同一 8 次模型预算内做一次无工具压缩（字段最多 500 字、合计最多 1,500 字），压缩只会推进到完整进入模型输入的连续消息，且成功摘要会立即替换当前最终回答的 system context。长 Direct 会冻结为唯一的“两次模型、零工具”合法形状，并复用同一公共准备/ToolLoop：有待压缩历史时先压缩后回答，否则保持一次回答。持久记忆与最近窗口之间若仍有未覆盖区间，prompt 会显式要求模型不得推断该段内容、必要时自然澄清；后续 Run 可继续推进覆盖。真实长聊语义校准仍未完成，不能把这些确定性合同描述为完整长对话能力。
- `CurrentRunWeb`、`CurrentRunExternal` 与结构化严格终局在验证绑定前密封正文；修复成功只发布一份最终正文，修复失败只发布无来源的“本轮未取得足够可核验来源正文”限制说明。`AnswerReset` 只保留历史事件兼容。
- Provider 首次空响应、无可见输出的瞬态错误或畸形响应会在原路由重试一次，再切换具备同工具面的候选；一旦已有可见正文、工具调用或 continuation 就不跨 Provider 暗接。
- 最近失败 Run 现在只向下一轮投影请求、终态、错误类别、模型/工具是否开始、尝试与切换计数等脱敏事实，不把失败草稿和旧来源当作证据。
- `agent:eval:contract` 只证明确定性合同。真实层的 v4 Campaign 共享 12 Run、96 模型轮次、72 Web 逻辑动作；Canary 为 4 Run、32/24。遥测只在 Host 实际执行工具时计入该账本，报告只记录可机械验证的轨迹，回答质量必须由哈希绑定的人工审阅包裁决。v2/v3 仅保留诊断兼容，不能进入产品门。此前 Campaign 无可用报告，HR-7 继续是“实测未通过”。
- 现代会话消息以 `evidence_refs_json` 的显式数组作为最终来源选择事实：空数组保持无来源，非空数组只投影所选 evidence；只有缺失该字段的旧消息可以使用 `SourceGroupFallback` 兼容读取。确定性报告使用 v2 计数，分开正常回答、预期安全拒绝与意外失败，明细和汇总不一致时命令必须非零。
- INC-HR-010 正在收敛：评测有效预算现已同时下发到 ToolLoop、executor 与 Direct Gateway；`WebRequired` 在不足两次网络动作时于派发前拒绝；模型探索轮为最终综合预留输出额度；拒绝的候选 continuation 不得删除先前已派发工具建立的 continuation；超长历史投影保留首尾并永不越过冻结 token 上限；公共 Direct 记忆准备与未覆盖历史标记已接入同一上下文快照；评测只读取当前 Run 的 assistant 正文，并单列实际 Host Web 逻辑动作。完整 v4 逐 Run 执行事实、生产入口组合验收和真实校准仍在施工，不能列为已验证能力。

## 不做的事

- 不以领域 operation、MiniMax 名称、电影/天气关键词或城市表修补核心。
- 核心循环回正不添加表、Provider、IPC 字段或第二套 Agent 状态机；笔记集成仅允许已有请求/预览/回执的必要可选字段扩展并同步类型。migration 073 仅重建既有、可再生的 Provider 健康统计。
- 不把确定性 mock、搜索成功、标题片段或来源存在夸大为逐句 NLI 事实验证或真实 Provider 质量。
- 不自动重放历史失败 Run；事实查询与写入都必须以新的用户 Run 重新授权。
