# Harness 时效性检索与工具恢复验收

日期：2026-09-08。范围：当前工作树中的 Harness 修复；不是已部署版本或 HR-7 产品门通过声明。

## 结论

已落地最低真实检索、调用修复、预算分离、取消保护、稳定来源标签和脱敏诊断。受控生产入口回归能够保证：截图原句不追加“联网搜索”也执行搜索和候选正文读取；格式错误不算工具执行或搜索无进展；关闭联网、仅本地和保密域不扩张权限。

**实际检索、正常结束、答案正确分别验收。** 双路 12 Run 回放中所有 Run 均提交回复，10 个联网场景均搜索并读取正文，2 个离线场景均零外发；但公开答案审查仍发现日期、地区、引用和工具协议泄漏问题。这些结果不能记为 12 个质量通过。后续针对性补验及剩余边界见下文。

未新增依赖、数据库表、Tauri IPC、研究引擎或 worktree；未重放或改写事故 Run。历史 Run 缺少诊断时按缺失读取。

## 事故与修复证据

事故 Run `5bdd17c3-d277-4613-9dff-21181d9dbc74` 冻结为 `WebPreferred / volatile_external_fact / verification none`：3 次模型轮次中有 2 次工具提议，但工具执行、审计和网页证据为零。旧记录没有拒绝原因，不能归责于具体模型或适配器。

| 已复现问题                   | 修复与失败先行证据                                                                                                           |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 日常时效没有最低检索保证     | `ToolSurfacePlan.requires_web_observation` 独立于严格证据要求；生产原句、模型直接回答、指定 URL 均覆盖                       |
| 本机时间或转换词吞掉外部核实 | Intake 检查剩余请求，区分指令与转换材料；本机时间、当前应用版本仍为负例                                                      |
| Markdown URL 未直读          | 从当前用户文本解析 HTTPS URL，URL 的 `://` 不再被误认成材料分隔符                                                            |
| 取消后仍执行 Host 检索       | Host 观察前及每次执行前检查取消；取消回归先出现 2 次错误执行，再通过零执行断言                                               |
| 无效调用吞掉工具/研究预算    | 缺失标识、JSON、schema、未知工具、预算和成功重复调用在提议阶段返回明确原因；连续两轮修复与服务失败、无进展独立               |
| 读取未获准 URL 导致整轮失败  | `web_url_not_in_current_run` 在派发前反馈，模型可改查再读取；真实失败转为生产回归                                            |
| 待修复正文清掉 Provider 绑定 | 真实工具执行后的路由绑定保留到 Run 结束，不因尚未提交的文字答复隐式跨模型续接                                                |
| 每次 fetch 从 W1 重新编号    | executor 从当前 Run ledger 读取稳定标签；重复读取第二个 URL 仍为 W2 的回归先失败再通过                                       |
| 追问的 W 编号被会话编号覆盖  | 普通终态链接和来源表统一读取 Run 编号，仅保存正文选择的 Web 来源；跨轮生产回归先失败，再通过正确 URL、单一来源和历史不变断言 |
| 无服务被计为完成观察         | 没有配置服务时记录能力阻断，`observationPerformed=false`；不包装成空搜索结果                                                 |
| 模型工具协议出现在离线正文   | 覆盖直接协议、分块前缀、带前言的 XML/JSON 协议；不执行嵌入内容，进入现有有界 Provider 恢复；代码示例和普通 HTML 保留         |
| 要求引用又禁止引用标记       | 区分模型提交绑定和最终 UI 表达，移除普通正文合同中的冲突；仍使用现有徽章和唯一来源区                                         |

提示词同时要求按事件日期、目标时间范围和来源实际地区作答。该约束不等于实现了自由文本事实校验。

## 确定性回归与质量检查

只运行受影响子集，不运行无关全量测试。最终计数和检查状态在本次交付时记录。

| Rust 过滤器                |                              通过数 |
| -------------------------- | ----------------------------------: |
| `agent_tool_loop_tests`    |                                  53 |
| `normal_run_service_tests` | 34（另有 1 项真实服务测试默认忽略） |
| `run_tool_loop::tests`     |                                  34 |
| `run_intake_tests`         |                                  83 |
| `tool_surface::`           |                                   5 |
| `run_context_tests`        |                                  29 |
| `timeliness_tests`         |                                   3 |
| `prompt_contract::`        |                                  12 |
| `llm_failover_guard_tests` |                                   5 |
| `text_support::`           |                                  23 |
| `model_gateway::`          |                                  68 |

上表 Rust 共 349 项；终态来源修改另通过 `run_engine_tests` 70 项、`run_engine::finalization::` 13 项、`normal_session_repository_tests` 14 项、`agent_evidence_repository_tests` 11 项，合计 **457 项 Rust 回归**。Vitest 5 个相关文件、49 项：`assistant-process`、`assistant-run-events`、`assistant-run-capability-degraded`、`web-capability-degradation-triage`、`harness-architecture-contract`。协议回归含原生分块参数、内容嵌入调用、缺失标识与无效 JSON。

检查命令：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run lint
npm run format:check
npm run typecheck
npm run docs:check
git diff --check
```

## 真实回放

使用现有 preflight、两条已配置匿名路由、一次性绑定及隔离 hydration，直接执行生产入口。每 Run 仍为 8 次模型逻辑调用、24 次工具、6 次网络逻辑动作；双路 12 Run 总上限为 96/72。Provider 内部尝试另行记录，不将逻辑轮次误写为 HTTP 请求数量。

固定公开问题为：电影原句两次、上海热映范围纠正、下个月上映追问、近期 NBA 动态、关闭联网的电影原句。日常测试不会启动这些真实服务。

| 回放                                     | 执行结果                                            | 质量结论                                                                                                   |
| ---------------------------------------- | --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| 首轮 12 Run                              | 7 completed、5 failed                               | 发现越界 URL 导致失败；测试驱动自身错误使用 48/36 总额度，耗尽后压低后续额度。该轮作排障证据，不作验收基线 |
| 修正额度及 URL 恢复后 12 Run             | 12 completed；10 联网均搜索和读取；2 离线零外发     | 未通过完整质量验收：日期分类、跨地区适用性、批次引用错位、离线协议泄漏                                     |
| 来源编号、日期约束和初版协议保护后 4 Run | 4 completed；2 联网均搜索和读取；2 离线零外发       | 日期分类有所改善；仍有无引用回答、来源范围不清及带前言协议泄漏，补为回归                                   |
| 引用提示和完整协议保护后 4 Run           | 4 completed；2 联网搜索和读取；2 离线零外发         | 两条联网原句出现可解析引用，离线协议未再泄漏；路由 2 仍混入较早片单且未明确地区                            |
| 同版本其余上下文 8 Run                   | 6 completed、2 failed；均有真实搜索，7 个有正文读取 | 暴露普通终态使用会话来源编号的问题，已转为回归并修正；最后两例出现服务/模型失败                            |

第二轮联网回答耗时：路由 1 最小/中位/最大为 21.937/59.726/73.979 秒；路由 2 为 32.324/69.456/83.129 秒。两路合计 40 次模型逻辑轮次、44 次网络逻辑动作。4 Run 补验中两条原句分别耗时 90.506 和 43.002 秒；这是实测延迟，不能用单测耗时代替。

清除引用提示冲突后的两条原句耗时为 40.811 和 58.725 秒。两条离线答复均零 Web 动作且未再显示工具协议；其中一条明确说明联网关闭，另一条仅询问地区偏好，后者不算完整的限制说明。随后 8 Run 上下文补验补齐了原计划的场景，但不代表质量全部通过。跨轮终态来源修复在这次真实补验之后完成，以独立生产回归和历史重载断言验证，未改写已完成的真实 Run。

回放文件在本地忽略目录 `target/agent-eval/`：

- 首轮：`timeliness-session-d50dde48b11a9b2b92a9bad3851b06f50d0d819802d1373c9e8846b829172afd.json`
- 第二轮：`timeliness-session-41be5ec33cd5b2fbc428d61f2b25b72bf951dd9699431f040b75a84eebccf530.json`
- 首次补验：`timeliness-session-f4a066d6d34533c4a1ce40c2e75f2509c18d947ebe330d2002705b986cbd4e4b.json`
- 引用与离线补验：`timeliness-session-af541e84ff404daa521fa82b74cfa7055e7a9f6ebdecaeb1db2a6e10d7eb1c1d.json`
- 上下文补验：`timeliness-session-dc466ffc3c2f9cb5fc20afd2e0ac114c7e6dc5ab8638cd179dc079ba81dbc322.json`

文件只包含固定公开问题、公开可见答案、公开来源及有界摘录、数值和安全诊断，不包含模型原始响应、隐藏推理或笔记内容。`sources` 是当前 Run 已登记来源，**不是最终正文逐条支持证明**；较新的 `selectedSources` 另记录历史加载得到的最终来源。`quality=pending_review` 不随 `completed` 自动变成通过。未上报 token 用量时写 `null`，不写 0。旧回放顶层 `modelTurns` 来自成功返回的模型轮次；发生错误时应同时查看诊断中的尝试轮次，不能用返回次数推断没有调用模型。

## 重放与诊断

真实入口为 `normal_run_service_tests::timeliness_live_natural_questions_campaign`，需要 fresh preflight 提供的 session、两路 profile 和成本确认。`IRIS_AGENT_EVAL_TIMELINESS_FOCUS=boundaries` 只重放两路原句及离线负例，共 4 Run；`continuity` 重放其余 8 Run。它复用现有运行器，不是另一套研究循环。

```bash
node scripts/diagnose-web-capability-degradation.mjs --run-id <run_id>
```

脚本通过只读连接查看冻结策略、提议/拒绝/派发累计计数、首次拒绝、首次执行失败、搜索/读取结果及退出原因。事件明细有界为 12 项，首次关键错误和累计计数不淘汰。未知工具仅记为 `unknown`，不保存查询和参数。

## 尚未通过的边界

- 完整 12 场景的地区、事件日期、推荐内容与来源支持尚未全部取得最终修复后的复验依据，HR-7 继续保持未通过。
- 普通自然正文没有新增 NLI 或逐句事实判定；存在网页正文不代表其内容可靠，也不代表所有推荐或当前状态都得到支持。
- 上下文补验最后两例出现 `agent_run_internal_execution_failed`，此前有 `web_provider_failed`。现有记录不足以判断具体模型拒绝/传输原因，不归责于适配器，也不将其伪装为无搜索结果。
- 真实延迟仍明显高于即时交互预期；本轮保持既有研究和服务预算，未以跳过必要读取换取更短耗时。
- 这是生产后端受控回放，未证明 macOS/Windows 桌面过程区逐帧表现。

## 2026-09-08 地区默认与双切换补充回归

本轮新增中国大陆默认检索范围合同：本轮显式地区优先，其次当前话题用户已确认地区；没有上述约束的地域敏感泛问使用大陆范围。全球性事实不套用该限制。Host 的确定性补词只覆盖无材料、无上下文、无具体限定的泛问，其余约束由同一模型循环解释；不新增分类模型、国家词典或地域状态机。正文证据地区不匹配时要求继续查询，不能把默认标签当作已核实事实。

只读检查最近两次运行，发现同一次 `web_fetch` 内依次记录 `web.search` 与 `web.fetch` 的相反服务切换。Broker 在零搜索额度下仍搜索是已复现原因；旧切换事件没有单次底层错误事实，不能据此归罪于特定服务。修正禁止 fetch 内隐式搜索、过滤无对应能力的切换候选，并区分和合并过程提示。抓取不再靠额外搜索补标题，显式 URL 的占位标题由实际页面标题替换。

失败证据：生产入口的显式 URL 测试发现了搜索健康记录；原句测试缺少默认地区；搜索专用候选被误报为抓取切换；前端两条能力均显示同句且重复；显式页面标题停留在 URL。分别记录在 `/tmp/iris-geography-fetch-red.log`、`/tmp/iris-failover-route-red.log`、`/tmp/iris-process-switch-red.log`、`/tmp/iris-fetch-title-red.log`。

验证：受影响 Rust 回归 231 项通过（另有 1 项真实回放忽略），Vitest 5 文件 38 项通过；Rust clippy、前端 lint/typecheck、Prettier 和文档事实检查通过。地区泛问、美国/法国/台湾/上海/全球范围、技术与科学负例、上下文防外泄均有受控回归。`geographic_` 补验覆盖用户原句及新电影、新歌、新闻的不同泛问表达。

本轮不追加付费真实模型回放，不用受控模型返回的文字证明地区理解或推荐质量通过。前述真实质量缺口和 HR-7 状态保持不变。

## 2026-09-08 追问续答与厂商对照

**事故根因尚未确定，不排除模型厂商的间歇性服务或兼容性问题。** 原事故两次 Run 在真实搜索返回后、下一次模型调用时失败；旧诊断只有 `provider_failure`，没有 HTTP 状态，不能把后来的试验状态倒填为事故原因。原提示“联网检索已完成，但模型服务暂时不可用”同时夸大了检索进度和故障归因。

已确认的独立修正：搜索只在工具完成事件后记为返回；失败提示只说明答复生成失败；工具绑定不再阻挡可重试错误的一次同模型恢复；不可重试请求拒绝、已有可见正文及恢复耗尽仍立即终止。安全诊断补充错误分类与可用 HTTP 状态，诊断脚本同时查看模型尝试和失败事件。这些修改不能作为原事故已根治的证据。

### 对照方法与结果

复用现有单次 preflight、隔离配置与加密凭据 hydration，未改写用户 Run。官方示例分支用直接 HTTPS 请求复现 [MiniMax 的函数调用示例](https://platform.minimax.io/docs/guides/text-m3-function-call)，保留完整的首次 assistant 消息；与经过 Iris `build_chat_completions_body` 序列化的同一消息比较。工具返回的是固定合成天气“24 C”，没有使用真实搜索；检查 HTTP 状态、错误信封及答案是否包含该合成事实。这里没有安装或执行官方 SDK。

| 对照                                              | 本次结果                                                           |
| ------------------------------------------------- | ------------------------------------------------------------------ |
| 完整官方消息，非流式续答                          | HTTP 200、无错误信封、回答包含合成事实                             |
| Iris 序列化同一消息，非流式续答                   | HTTP 200、无错误信封、回答包含合成事实                             |
| 完整官方消息，流式续答                            | HTTP 200、无错误信封、回答包含合成事实                             |
| Iris 序列化同一消息，流式续答                     | HTTP 200、无错误信封、回答包含合成事实                             |
| Iris 消息仅补回原始 `reasoning_content`，流式续答 | 同样成功；没有证明缺少此字段导致失败                               |
| 原追问文字，正常生产预算 8/24/6                   | 7 次模型轮次、6 次网络动作，包含搜索和读取，正常完成；没有触发重试 |

正常预算探针用时 51.32 秒，包含隔离入口开销；本次仅验证执行与终态，未做最终电影答案和来源的人工质量验收。它将原追问作为隔离 Run 输入，没有复制旧会话历史，不能替代完整上下文回放。

早期限制到两轮模型的生产探针曾返回 HTTP 422，也曾成功；该限制会提前关闭工具并追加综合指令，不能当作正常预算原样复现。独立原生工具续答探针也出现过 HTTP 400。上述差异不足以决定责任归属；一次成功对照不能证明服务持续健康。此次没有修改 MiniMax 的生产消息序列化、流解析、端点或推理配置，排障临时钩子已移除。

合成对照的安全指标在 `/tmp/iris-continuation-official-stream-mode.log`；正常预算诊断在 `/tmp/iris-continuation-default-budget.log`。文件不含原始模型响应、隐藏推理、笔记或凭据。保留默认忽略的 `live_follow_up_continuation_probe` 以便复测：`IRIS_CONTINUATION_PROBE=reference-parity` 至多五次 HTTP 请求；`production-run` 执行一个默认预算 Run。两种模式均要求 fresh preflight 提供的单次批准 profile，失败会使探针失败；成功终态仍不代表答案质量通过。

### 回归记录

失败先行：界面三项复现原错误进度/文案；后端同模型恢复与 422 诊断两项先失败。证据分别为 `/tmp/iris-continuation-ui-red.log`、`/tmp/iris-continuation-red.log`。新增负例覆盖恢复仅一次、已显示正文时不再重试。

最终验证：Provider 回归 9 项，加既有真实 HTTP 边界的恢复回归 3 项，共 12 项 Rust 测试通过；Vitest 4 个文件、50 项通过。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、前端 lint/typecheck/format、文档事实检查和差异空白检查通过。只运行相关测试，真实探针默认忽略。日志见 `/tmp/iris-continuation-rust-green.log` 和 `/tmp/iris-continuation-final-*.log`。HR-7 与原事故根因继续保持未验收。
